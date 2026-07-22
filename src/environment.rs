use std::collections::HashMap;

use gc::{Finalize, Gc, GcCell, Trace, custom_trace};

use crate::value::Value;

#[derive(Clone)]
pub struct Environment {
    frame: Gc<Frame>,
}

struct Frame {
    values: GcCell<HashMap<String, Value>>,
    parent: Option<Environment>,
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
        mark(&this.parent);
    });
}

impl Environment {
    pub fn new() -> Self {
        Self {
            frame: Gc::new(Frame {
                values: GcCell::new(HashMap::new()),
                parent: None,
            }),
        }
    }

    pub fn child(&self) -> Self {
        Self {
            frame: Gc::new(Frame {
                values: GcCell::new(HashMap::new()),
                parent: Some(self.clone()),
            }),
        }
    }

    pub fn define(&self, name: impl Into<String>, value: Value) {
        self.frame.values.borrow_mut().insert(name.into(), value);
    }

    pub(crate) fn assign(&self, name: &str, value: Value) -> bool {
        if self.frame.values.borrow().contains_key(name) {
            self.frame
                .values
                .borrow_mut()
                .insert(name.to_owned(), value);
            true
        } else if let Some(parent) = &self.frame.parent {
            parent.assign(name, value)
        } else {
            false
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.frame.values.borrow().get(name).cloned() {
            return Some(value);
        }

        self.frame
            .parent
            .as_ref()
            .and_then(|parent| parent.get(name))
    }

    pub(crate) fn contains_current(&self, name: &str) -> bool {
        self.frame.values.borrow().contains_key(name)
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
        parent.define("value", Value::Int(1));
        let child = parent.child();

        assert_eq!(int(&child, "value"), Some(1));
        child.define("value", Value::Int(2));
        assert_eq!(int(&child, "value"), Some(2));
        assert_eq!(int(&parent, "value"), Some(1));
    }

    #[test]
    fn cloned_environments_share_the_same_frame() {
        let first = Environment::new();
        let second = first.clone();

        second.define("later", Value::Int(7));

        assert_eq!(int(&first, "later"), Some(7));
    }

    #[test]
    fn closures_observe_functions_bound_after_capture() {
        let environment = Environment::new();
        let function = Gc::new(UserFunction {
            name: "recur".to_owned(),
            params: Vec::new(),
            body: Block {
                items: Vec::new(),
                span: Span::new(0, 0),
            },
            closure: environment.clone(),
        });

        environment.define("recur", Value::Function(function.clone()));

        let Some(Value::Function(observed)) = function.closure.get("recur") else {
            panic!("closure did not observe its recursive binding");
        };
        assert!(Gc::ptr_eq(&function, &observed));
    }

    #[test]
    fn assignment_updates_the_nearest_existing_shared_frame() {
        let parent = Environment::new();
        parent.define("value", Value::Int(1));
        let child = parent.child();
        let alias = parent.clone();

        assert!(child.assign("value", Value::Int(2)));
        assert_eq!(int(&parent, "value"), Some(2));
        assert_eq!(int(&alias, "value"), Some(2));

        child.define("value", Value::Int(3));
        assert!(child.assign("value", Value::Int(4)));
        assert_eq!(int(&child, "value"), Some(4));
        assert_eq!(int(&parent, "value"), Some(2));
        assert!(!child.assign("missing", Value::Nil));
    }

    #[test]
    fn local_membership_does_not_include_parent_bindings() {
        let parent = Environment::new();
        parent.define("parent_only", Value::Nil);
        let child = parent.child();

        assert!(parent.contains_current("parent_only"));
        assert!(!child.contains_current("parent_only"));
        assert!(child.get("parent_only").is_some());
    }
}
