use gc::{Gc, GcCell};

use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{BinaryOp, Block, Expr, ExprKind, Stmt, StmtKind};
use crate::runtime::{
    Environment, List, MapKey, Raised, RuntimeError, RuntimeResult, UserFunction, Value,
};
use crate::span::Span;

impl Interpreter {
    pub(super) fn evaluate_items(
        &mut self,
        items: &[Stmt],
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let mut result = Value::Nil;
        for item in items {
            result = self.evaluate_statement(item, env)?;
        }
        Ok(result)
    }

    pub(super) fn evaluate_block(
        &mut self,
        block: &Block,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        self.evaluate_items(&block.items, env)
    }

    fn evaluate_statement(
        &mut self,
        statement: &Stmt,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        match &statement.kind {
            StmtKind::Function { name, params, body } => {
                self.ensure_new_definition(env, name, statement.span)?;
                let function = UserFunction {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    closure: env.clone(),
                };
                env.define(name.clone(), Value::Function(Gc::new(function)));
                Ok(Value::Nil)
            }
            StmtKind::Let { name, value } => {
                let value = self.evaluate_expression(value, env)?;
                self.ensure_new_definition(env, name, statement.span)?;
                env.define(name.clone(), value);
                Ok(Value::Nil)
            }
            StmtKind::Expr(expression) => self.evaluate_expression(expression, env),
        }
    }

    fn ensure_new_definition(
        &self,
        env: &Environment,
        name: &str,
        span: Span,
    ) -> RuntimeResult<()> {
        if env.contains_current(name) {
            Err(RuntimeError {
                span,
                message: format!("name `{name}` is already defined in this scope"),
            })
        } else {
            Ok(())
        }
    }

    pub(super) fn evaluate_expression(
        &mut self,
        expression: &Expr,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        match &expression.kind {
            ExprKind::Int(value) => Ok(Value::Int(*value)),
            ExprKind::Float(value) => Ok(Value::Float(*value)),
            ExprKind::String(value) => Ok(Value::String(value.clone())),
            ExprKind::Bool(value) => Ok(Value::Bool(*value)),
            ExprKind::Nil => Ok(Value::Nil),
            ExprKind::List(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.evaluate_expression(element, env)?);
                }
                Ok(Value::List(List::shared(values)))
            }
            ExprKind::Map(entries) => {
                let mut values = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let key_value = self.evaluate_expression(key, env)?;
                    let key = MapKey::from_value(key_value, key.span)?;
                    let value = self.evaluate_expression(value, env)?;
                    if matches!(value, Value::Nil) {
                        if let Some(position) =
                            values.iter().position(|(existing, _)| existing == &key)
                        {
                            values.remove(position);
                        }
                    } else if let Some((_, existing)) =
                        values.iter_mut().find(|(existing, _)| existing == &key)
                    {
                        *existing = value;
                    } else {
                        values.push((key, value));
                    }
                }
                Ok(Value::Map(Gc::new(GcCell::new(values))))
            }
            ExprKind::Variable(name) => env.get(name).ok_or_else(|| {
                EvaluationError::Runtime(RuntimeError {
                    span: expression.span,
                    message: format!("undefined name `{name}`"),
                })
            }),
            ExprKind::Assign { target, value } => {
                let target = self.prepare_assignment_target(target, env)?;
                let value = self.evaluate_expression(value, env)?;
                self.commit_assignment(target, value, env)
            }
            ExprKind::If {
                branches,
                else_branch,
            } => self.evaluate_if(branches, else_branch.as_ref(), env),
            ExprKind::Match { value, cases } => {
                self.evaluate_match(value, cases, expression.span, env)
            }
            ExprKind::Raise { value } => {
                let value = self.evaluate_expression(value, env)?;
                Err(EvaluationError::Raised(Raised::new(value, expression.span)))
            }
            ExprKind::Try { protected, cases } => self.evaluate_try(protected, cases, env),
            ExprKind::Loop {
                state,
                initial,
                body,
            } => self.evaluate_loop(state, initial, body, env),
            ExprKind::Continue { value } => {
                let value = self.evaluate_expression(value, env)?;
                Err(EvaluationError::Continue {
                    value,
                    span: expression.span,
                })
            }
            ExprKind::Break { value } => {
                let value = self.evaluate_expression(value, env)?;
                Err(EvaluationError::Break {
                    value,
                    span: expression.span,
                })
            }
            ExprKind::Call { callee, args } => {
                let callee = self.evaluate_expression(callee, env)?;
                let args = self.evaluate_arguments(args, env)?;
                self.call_value(callee, args, expression.span)
            }
            ExprKind::Field { object, name } => {
                let object = self.evaluate_expression(object, env)?;
                self.read_index(object, Value::String(name.clone()), expression.span)
            }
            ExprKind::Index { object, key } => {
                let object = self.evaluate_expression(object, env)?;
                let key = self.evaluate_expression(key, env)?;
                self.read_index(object, key, expression.span)
            }
            ExprKind::Unary { op, value } => {
                let value = self.evaluate_expression(value, env)?;
                self.evaluate_unary(op, value, expression.span)
                    .map_err(EvaluationError::from)
            }
            ExprKind::Binary {
                left,
                op: op @ (BinaryOp::And | BinaryOp::Or),
                right,
            } => self.evaluate_boolean_binary(left, op, right, env),
            ExprKind::Binary { left, op, right } => {
                let left = self.evaluate_expression(left, env)?;
                let right = self.evaluate_expression(right, env)?;
                self.evaluate_binary(left, op, right, expression.span)
            }
            ExprKind::Pipeline { input, stages } => self.evaluate_pipeline(input, stages, env),
        }
    }

    fn evaluate_boolean_binary(
        &mut self,
        left: &Expr,
        operator: &BinaryOp,
        right: &Expr,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let left_value = match self.evaluate_expression(left, env)? {
            Value::Bool(value) => value,
            value => {
                return Err(EvaluationError::Runtime(RuntimeError::new(
                    left.span,
                    format!(
                        "operator `{}` requires boolean operands, got {}",
                        if matches!(operator, BinaryOp::And) {
                            "and"
                        } else {
                            "or"
                        },
                        value.type_name()
                    ),
                )));
            }
        };
        if (matches!(operator, BinaryOp::And) && !left_value)
            || (matches!(operator, BinaryOp::Or) && left_value)
        {
            return Ok(Value::Bool(left_value));
        }
        let right_value = match self.evaluate_expression(right, env)? {
            Value::Bool(value) => value,
            value => {
                return Err(EvaluationError::Runtime(RuntimeError::new(
                    right.span,
                    format!(
                        "operator `{}` requires boolean operands, got {}",
                        if matches!(operator, BinaryOp::And) {
                            "and"
                        } else {
                            "or"
                        },
                        value.type_name()
                    ),
                )));
            }
        };
        Ok(Value::Bool(right_value))
    }

    fn evaluate_if(
        &mut self,
        branches: &[(Expr, Block)],
        else_branch: Option<&Block>,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        for (condition, branch) in branches {
            let condition_value = self.evaluate_expression(condition, env)?;
            match condition_value {
                Value::Bool(true) => {
                    let branch_env = env.child();
                    return self.evaluate_block(branch, &branch_env);
                }
                Value::Bool(false) => {}
                value => {
                    return Err(EvaluationError::Runtime(RuntimeError {
                        span: condition.span,
                        message: format!("if condition must be boolean, got {}", value.type_name()),
                    }));
                }
            }
        }

        match else_branch {
            Some(branch) => {
                let branch_env = env.child();
                self.evaluate_block(branch, &branch_env)
            }
            None => Ok(Value::Nil),
        }
    }

    fn evaluate_loop(
        &mut self,
        state: &str,
        initial: &Expr,
        body: &Block,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let mut next_state = self.evaluate_expression(initial, env)?;

        loop {
            let iteration_env = env.child();
            iteration_env.define(state.to_owned(), next_state);

            match self.evaluate_block(body, &iteration_env) {
                Ok(value) | Err(EvaluationError::Continue { value, .. }) => {
                    next_state = value;
                }
                Err(EvaluationError::Break { value, .. }) => return Ok(value),
                Err(error @ (EvaluationError::Runtime(_) | EvaluationError::Raised(_))) => {
                    return Err(error);
                }
            }
        }
    }
}
