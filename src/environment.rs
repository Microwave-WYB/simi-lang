use std::collections::HashMap;

use gc::{Finalize, Gc, GcCell, Trace, custom_trace};

use crate::value::Value;

type Binding = Gc<GcCell<Value>>;

#[derive(Clone)]
pub struct Environment {
    frame: Gc<Frame>,
}

struct Frame {
    values: GcCell<HashMap<String, Binding>>,
    version_parent: Option<Environment>,
    lexical_parent: Option<Environment>,
}

impl Finalize for Environment {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for Environment {
    custom_trace!(this, {
        mark(&this.frame);
    });
}

impl Finalize for Frame {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for Frame {
    custom_trace!(this, {
        mark(&this.values);
        mark(&this.version_parent);
        mark(&this.lexical_parent);
    });
}

impl Environment {
    pub fn new() -> Self {
        Self {
            frame: Gc::new(Frame {
                values: GcCell::new(HashMap::new()),
                version_parent: None,
                lexical_parent: None,
            }),
        }
    }

    /// Create a genuine nested lexical scope.
    pub fn child(&self) -> Self {
        Self {
            frame: Gc::new(Frame {
                values: GcCell::new(HashMap::new()),
                version_parent: None,
                lexical_parent: Some(self.clone()),
            }),
        }
    }

    /// Create a later view of the same lexical scope.
    ///
    /// Existing closures keep the earlier view, while subsequent evaluation uses the
    /// returned view. Unlike `child`, fresh names declared later in this scope are
    /// backfilled into every earlier view so forward captures keep working.
    pub(crate) fn shadow_view(&self) -> Self {
        Self {
            frame: Gc::new(Frame {
                values: GcCell::new(HashMap::new()),
                version_parent: Some(self.clone()),
                lexical_parent: None,
            }),
        }
    }

    /// Define or replace a binding in this environment's own frame. This is used by
    /// host-created environments and genuine child scopes such as function calls.
    pub fn define(&self, name: impl Into<String>, value: Value) {
        let name = name.into();
        if let Some(binding) = self.frame.values.borrow().get(&name).cloned() {
            *binding.borrow_mut() = value;
        } else {
            self.frame
                .values
                .borrow_mut()
                .insert(name, Gc::new(GcCell::new(value)));
        }
    }

    /// Install a name that is new to this lexical scope. Every existing version view
    /// receives the same cell, which gives closures the established forward-capture
    /// behavior without exposing names across a true lexical boundary.
    pub(crate) fn define_fresh(&self, name: impl Into<String>, value: Value) {
        let name = name.into();
        let binding = Gc::new(GcCell::new(value));
        self.backfill(name, binding);
    }

    /// Install a new binding version only in this view.
    pub(crate) fn define_shadow(&self, name: impl Into<String>, value: Value) {
        self.frame
            .values
            .borrow_mut()
            .insert(name.into(), Gc::new(GcCell::new(value)));
    }

    fn backfill(&self, name: String, binding: Binding) {
        self.frame
            .values
            .borrow_mut()
            .insert(name.clone(), binding.clone());
        if let Some(parent) = &self.frame.version_parent {
            parent.backfill(name, binding);
        }
    }

    pub(crate) fn assign(&self, name: &str, value: Value) -> bool {
        let Some(binding) = self.binding(name) else {
            return false;
        };
        *binding.borrow_mut() = value;
        true
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.binding(name).map(|binding| binding.borrow().clone())
    }

    fn binding(&self, name: &str) -> Option<Binding> {
        if let Some(binding) = self.frame.values.borrow().get(name).cloned() {
            return Some(binding);
        }
        if let Some(parent) = &self.frame.version_parent
            && let Some(binding) = parent.binding(name)
        {
            return Some(binding);
        }
        self.frame
            .lexical_parent
            .as_ref()
            .and_then(|parent| parent.binding(name))
    }

    pub(crate) fn contains_current(&self, name: &str) -> bool {
        if self.frame.values.borrow().contains_key(name) {
            return true;
        }
        self.frame
            .version_parent
            .as_ref()
            .is_some_and(|parent| parent.contains_current(name))
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use gc::Gc;

    use crate::ast::Block;
    use crate::span::Span;
    use crate::value::UserFunction;

    use super::*;

    fn int(environment: &Environment, name: &str) -> Option<i64> {
        match environment.get(name) {
            Some(Value::Int(value)) => Some(value),
            _ => None,
        }
    }

    #[test]
    fn children_lookup_parents_and_shadow_locally() {
        let parent = Environment::new();
        parent.define_fresh("value", Value::Int(1));
        let child = parent.child();

        assert_eq!(int(&child, "value"), Some(1));
        child.define_fresh("value", Value::Int(2));
        assert_eq!(int(&child, "value"), Some(2));
        assert_eq!(int(&parent, "value"), Some(1));
    }

    #[test]
    fn cloned_environments_share_the_same_frame() {
        let first = Environment::new();
        let second = first.clone();

        second.define_fresh("later", Value::Int(7));
        assert_eq!(int(&first, "later"), Some(7));
    }

    #[test]
    fn version_views_freeze_shadowed_cells_but_share_fresh_later_names() {
        let first = Environment::new();
        first.define_fresh("value", Value::Int(1));
        let second = first.shadow_view();
        second.define_shadow("value", Value::Int(2));
        second.define_fresh("later", Value::Int(3));

        assert_eq!(int(&first, "value"), Some(1));
        assert_eq!(int(&second, "value"), Some(2));
        assert_eq!(int(&first, "later"), Some(3));
        assert_eq!(int(&second, "later"), Some(3));

        first.assign("value", Value::Int(4));
        second.assign("later", Value::Int(5));
        assert_eq!(int(&first, "value"), Some(4));
        assert_eq!(int(&second, "value"), Some(2));
        assert_eq!(int(&first, "later"), Some(5));
    }

    #[test]
    fn assignments_walk_parents_and_update_the_nearest_scope() {
        let parent = Environment::new();
        parent.define_fresh("value", Value::Int(1));
        let child = parent.child();
        let alias = parent.clone();

        assert!(child.assign("value", Value::Int(2)));
        assert_eq!(int(&parent, "value"), Some(2));
        assert_eq!(int(&alias, "value"), Some(2));

        child.define_fresh("value", Value::Int(3));
        assert!(child.assign("value", Value::Int(4)));
        assert_eq!(int(&child, "value"), Some(4));
        assert_eq!(int(&parent, "value"), Some(2));
        assert!(!child.assign("missing", Value::Int(0)));
    }

    #[test]
    fn closure_environment_edges_are_traced() {
        let environment = Environment::new();
        let function = Value::Function(Gc::new(UserFunction {
            name: "self".to_owned(),
            params: Vec::new(),
            body: Block {
                items: Vec::new(),
                span: Span::new(0, 0),
            },
            closure: environment.clone(),
            trace_calls: true,
            module: None,
        }));
        environment.define_fresh("self", function);
        assert!(matches!(environment.get("self"), Some(Value::Function(_))));
    }
}
