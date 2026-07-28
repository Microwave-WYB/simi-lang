use gc::{Gc, GcCell};

use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::interpreter::operations::values_equal;
use crate::runtime::{List, MapKey, RuntimeError, Value};
use crate::span::Span;
use crate::value::IteratorIntrinsic;

enum IteratorStep {
    Done,
    Item(Value),
}

enum IteratorControl {
    Continue(Value),
    Break(Value),
}

impl Interpreter {
    pub(super) fn call_iterator_intrinsic(
        &mut self,
        intrinsic: IteratorIntrinsic,
        arguments: &[Value],
        span: Span,
    ) -> EvaluationResult<Value> {
        match intrinsic {
            IteratorIntrinsic::TypedIterator => Ok(arguments[0].clone()),
            IteratorIntrinsic::ValidateCount => validate_count(&arguments[0], span),
            IteratorIntrinsic::ValidateRange => validate_range(arguments, span),
            IteratorIntrinsic::DropNext => self.drop_next(arguments, span),
            IteratorIntrinsic::EnumerateNext => self.enumerate_next(arguments, span),
            IteratorIntrinsic::ZipNext => self.zip_next(arguments, span),
            IteratorIntrinsic::ZipLongestNext => self.zip_longest_next(arguments, span),
            IteratorIntrinsic::FilterNext => self.filter_next(arguments, span),
            IteratorIntrinsic::ToList => self.iterator_to_list(arguments, span),
            IteratorIntrinsic::Fold => self.iterator_fold(arguments, span),
            IteratorIntrinsic::Find => self.iterator_find(arguments, span),
            IteratorIntrinsic::FindIndex => self.iterator_find_index(arguments, span),
            IteratorIntrinsic::Contains => self.iterator_contains(arguments, span),
            IteratorIntrinsic::Any => self.iterator_any(arguments, span),
            IteratorIntrinsic::All => self.iterator_all(arguments, span),
            IteratorIntrinsic::Each => self.iterator_each(arguments, span),
            IteratorIntrinsic::Count => self.iterator_count(arguments, span),
            IteratorIntrinsic::EachWhile => self.iterator_each_while(arguments, span),
            IteratorIntrinsic::FoldWhile => self.iterator_fold_while(arguments, span),
            IteratorIntrinsic::Loop => self.iterator_loop(arguments, span),
            IteratorIntrinsic::RepeatNext => {
                self.call_value(arguments[0].clone(), Vec::new(), span)
            }
        }
    }

    fn drop_next(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let Value::Int(count) = arguments[1] else {
            return Err(contract_error(
                span,
                "std/iter.drop count must be a nonnegative integer".to_owned(),
            ));
        };
        for _ in 0..count {
            if matches!(self.pull_iterator(&arguments[0], span)?, IteratorStep::Done) {
                return Ok(step_value(IteratorStep::Done));
            }
        }
        self.pull_iterator(&arguments[0], span).map(step_value)
    }

    fn enumerate_next(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let Value::Int(index) = arguments[1] else {
            return Err(contract_error(
                span,
                "std/iter.enumerate index must be an integer".to_owned(),
            ));
        };
        let step = match self.pull_iterator(&arguments[0], span)? {
            IteratorStep::Done => IteratorStep::Done,
            IteratorStep::Item(value) => IteratorStep::Item(Value::List(
                List::new(vec![Value::Int(index), value]).into_shared(),
            )),
        };
        Ok(step_value(step))
    }

    fn zip_next(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let left = match self.pull_iterator(&arguments[0], span)? {
            IteratorStep::Done => return Ok(step_value(IteratorStep::Done)),
            IteratorStep::Item(value) => value,
        };
        let right = match self.pull_iterator(&arguments[1], span)? {
            IteratorStep::Done => return Ok(step_value(IteratorStep::Done)),
            IteratorStep::Item(value) => value,
        };
        Ok(step_value(IteratorStep::Item(Value::List(
            List::new(vec![left, right]).into_shared(),
        ))))
    }

    fn zip_longest_next(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let Value::Bool(mut left_done) = arguments[3] else {
            return Err(contract_error(
                span,
                "std/iter.zip_longest internal completion state must be boolean".to_owned(),
            ));
        };
        let Value::Bool(mut right_done) = arguments[4] else {
            return Err(contract_error(
                span,
                "std/iter.zip_longest internal completion state must be boolean".to_owned(),
            ));
        };

        let left = if left_done {
            None
        } else {
            match self.pull_iterator(&arguments[0], span)? {
                IteratorStep::Done => {
                    left_done = true;
                    None
                }
                IteratorStep::Item(value) => Some(value),
            }
        };
        let right = if right_done {
            None
        } else {
            match self.pull_iterator(&arguments[1], span)? {
                IteratorStep::Done => {
                    right_done = true;
                    None
                }
                IteratorStep::Item(value) => Some(value),
            }
        };

        let step = if left_done && right_done {
            IteratorStep::Done
        } else {
            IteratorStep::Item(Value::List(
                List::new(vec![
                    left.unwrap_or_else(|| arguments[2].clone()),
                    right.unwrap_or_else(|| arguments[2].clone()),
                ])
                .into_shared(),
            ))
        };
        Ok(longest_step_value(step, left_done, right_done))
    }

    fn filter_next(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        loop {
            let value = match self.pull_iterator(&arguments[0], span)? {
                IteratorStep::Done => return Ok(step_value(IteratorStep::Done)),
                IteratorStep::Item(value) => value,
            };
            let accepted = self.call_value(arguments[1].clone(), vec![value.clone()], span)?;
            match accepted {
                Value::Bool(true) => return Ok(step_value(IteratorStep::Item(value))),
                Value::Bool(false) => {}
                value => return Err(predicate_error("filter", &value, span)),
            }
        }
    }

    fn iterator_to_list(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let mut values = Vec::new();
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            values.push(value);
        }
        Ok(Value::List(List::new(values).into_shared()))
    }

    fn iterator_fold(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let mut state = arguments[1].clone();
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            state = self.call_value(arguments[2].clone(), vec![state, value], span)?;
        }
        Ok(state)
    }

    fn iterator_find(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let matched = self.call_value(arguments[1].clone(), vec![value.clone()], span)?;
            match matched {
                Value::Bool(true) => return Ok(value),
                Value::Bool(false) => {}
                value => return Err(predicate_error("find", &value, span)),
            }
        }
        Ok(Value::Nil)
    }

    fn iterator_find_index(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let mut index = 0_i64;
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let matched = self.call_value(arguments[1].clone(), vec![value], span)?;
            match matched {
                Value::Bool(true) => return Ok(Value::Int(index)),
                Value::Bool(false) => {
                    index = index.checked_add(1).ok_or_else(|| {
                        RuntimeError::new(span, "std/iter.find_index exceeded integer range")
                    })?;
                }
                value => return Err(predicate_error("find_index", &value, span)),
            }
        }
        Ok(Value::Nil)
    }

    fn iterator_contains(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            if values_equal(&value, &arguments[1], span)? {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }

    fn iterator_any(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let matched = self.call_value(arguments[1].clone(), vec![value], span)?;
            match matched {
                Value::Bool(true) => return Ok(Value::Bool(true)),
                Value::Bool(false) => {}
                value => return Err(predicate_error("any", &value, span)),
            }
        }
        Ok(Value::Bool(false))
    }

    fn iterator_all(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let matched = self.call_value(arguments[1].clone(), vec![value], span)?;
            match matched {
                Value::Bool(true) => {}
                Value::Bool(false) => return Ok(Value::Bool(false)),
                value => return Err(predicate_error("all", &value, span)),
            }
        }
        Ok(Value::Bool(true))
    }

    fn iterator_each(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            self.call_value(arguments[1].clone(), vec![value], span)?;
        }
        Ok(Value::Nil)
    }

    fn iterator_count(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let mut total = 0_i64;
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let matched = self.call_value(arguments[1].clone(), vec![value], span)?;
            match matched {
                Value::Bool(true) => {
                    total = total.checked_add(1).ok_or_else(|| {
                        RuntimeError::new(span, "std/iter.count exceeded integer range")
                    })?;
                }
                Value::Bool(false) => {}
                value => return Err(predicate_error("count", &value, span)),
            }
        }
        Ok(Value::Int(total))
    }

    fn iterator_each_while(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let control = self.call_value(arguments[1].clone(), vec![value], span)?;
            match decode_control(control, "each_while", span)? {
                IteratorControl::Continue(_) => {}
                IteratorControl::Break(value) => return Ok(value),
            }
        }
        Ok(Value::Nil)
    }

    fn iterator_fold_while(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let mut state = arguments[1].clone();
        while let IteratorStep::Item(value) = self.pull_iterator(&arguments[0], span)? {
            let control =
                self.call_value(arguments[2].clone(), vec![state.clone(), value], span)?;
            match decode_control(control, "fold_while", span)? {
                IteratorControl::Continue(next_state) => state = next_state,
                IteratorControl::Break(value) => return Ok(value),
            }
        }
        Ok(state)
    }

    fn iterator_loop(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        loop {
            let control = self.call_value(arguments[0].clone(), Vec::new(), span)?;
            match decode_control(control, "loop", span)? {
                IteratorControl::Continue(_) => {}
                IteratorControl::Break(value) => return Ok(value),
            }
        }
    }

    fn pull_iterator(&mut self, iterator: &Value, span: Span) -> EvaluationResult<IteratorStep> {
        let step = self.call_value(iterator.clone(), Vec::new(), span)?;
        decode_step(step, span)
    }
}

fn validate_count(value: &Value, span: Span) -> EvaluationResult<Value> {
    match value {
        Value::Int(count) if *count >= 0 => Ok(Value::Nil),
        Value::Int(count) => Err(contract_error(
            span,
            format!("std/iter count must be nonnegative, got {count}"),
        )),
        value => Err(contract_error(
            span,
            format!(
                "std/iter count must be a nonnegative integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn validate_range(arguments: &[Value], span: Span) -> EvaluationResult<Value> {
    if matches!(arguments, [Value::Int(_), Value::Int(_)]) {
        Ok(Value::Nil)
    } else {
        Err(contract_error(
            span,
            "std/iter.range bounds must be integers".to_owned(),
        ))
    }
}

fn decode_step(value: Value, span: Span) -> EvaluationResult<IteratorStep> {
    let Value::Map(entries) = value else {
        return Err(contract_error(
            span,
            format!(
                "std/iter iterator must return a step map, got {}",
                value.type_name()
            ),
        ));
    };
    let (done, item) = {
        let entries = entries.try_borrow().map_err(|_| {
            contract_error(span, "std/iter could not inspect iterator step".to_owned())
        })?;
        let done = map_string_field(&entries, "done");
        let item = map_string_field(&entries, "value").unwrap_or(Value::Nil);
        (done, item)
    };
    match done {
        Some(Value::Bool(true)) => Ok(IteratorStep::Done),
        Some(Value::Bool(false)) => Ok(IteratorStep::Item(item)),
        Some(value) => Err(contract_error(
            span,
            format!(
                "std/iter step field `done` must be boolean, got {}",
                value.type_name()
            ),
        )),
        None => Err(contract_error(
            span,
            "std/iter iterator step is missing boolean field `done`".to_owned(),
        )),
    }
}

fn decode_control(value: Value, operation: &str, span: Span) -> EvaluationResult<IteratorControl> {
    let Value::Map(entries) = value else {
        return Err(contract_error(
            span,
            format!(
                "std/iter.{operation} callback must return a control map, got {}",
                value.type_name()
            ),
        ));
    };
    let (control, payload) = {
        let entries = entries.try_borrow().map_err(|_| {
            contract_error(
                span,
                format!("std/iter.{operation} could not inspect callback control"),
            )
        })?;
        (
            map_string_field(&entries, "control"),
            map_string_field(&entries, "value").unwrap_or(Value::Nil),
        )
    };
    match control {
        Some(Value::String(control)) if control == "continue" => {
            Ok(IteratorControl::Continue(payload))
        }
        Some(Value::String(control)) if control == "break" => Ok(IteratorControl::Break(payload)),
        Some(Value::String(control)) => Err(contract_error(
            span,
            format!(
                "std/iter.{operation} callback control must be `break` or `continue`, got `{control}`"
            ),
        )),
        Some(value) => Err(contract_error(
            span,
            format!(
                "std/iter.{operation} callback field `control` must be a string, got {}",
                value.type_name()
            ),
        )),
        None => Err(contract_error(
            span,
            format!("std/iter.{operation} callback control is missing field `control`"),
        )),
    }
}

fn map_string_field(entries: &[(MapKey, Value)], name: &str) -> Option<Value> {
    entries
        .iter()
        .find(|(key, _)| matches!(key, MapKey::String(key) if key == name))
        .map(|(_, value)| value.clone())
}

fn step_value(step: IteratorStep) -> Value {
    let mut entries = vec![(
        MapKey::String("done".to_owned()),
        Value::Bool(matches!(step, IteratorStep::Done)),
    )];
    if let IteratorStep::Item(value) = step
        && !matches!(value, Value::Nil)
    {
        entries.push((MapKey::String("value".to_owned()), value));
    }
    Value::Map(Gc::new(GcCell::new(entries)))
}

fn longest_step_value(step: IteratorStep, left_done: bool, right_done: bool) -> Value {
    let Value::Map(entries) = step_value(step) else {
        unreachable!("iterator steps are maps")
    };
    {
        let mut entries = entries.borrow_mut();
        entries.push((
            MapKey::String("left_done".to_owned()),
            Value::Bool(left_done),
        ));
        entries.push((
            MapKey::String("right_done".to_owned()),
            Value::Bool(right_done),
        ));
    }
    Value::Map(entries)
}

fn predicate_error(operation: &str, value: &Value, span: Span) -> EvaluationError {
    contract_error(
        span,
        format!(
            "std/iter.{operation} predicate must return a boolean, got {}",
            value.type_name()
        ),
    )
}

fn contract_error(span: Span, message: String) -> EvaluationError {
    EvaluationError::Runtime(RuntimeError::new(span, message))
}
