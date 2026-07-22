use std::sync::Arc;

use gc::{Gc, GcCell};

use crate::runtime::{MapKey, NativeResult, Value};
use crate::span::Span;
use crate::value::NativeFunction;

pub type NativeCallback = dyn Fn(&[Value], Span) -> NativeResult + Send + Sync + 'static;

pub struct Module {
    name: String,
    exports: Vec<(String, Value)>,
}

impl Module {
    pub fn builder(name: impl Into<String>) -> ModuleBuilder {
        ModuleBuilder {
            name: name.into(),
            exports: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn into_parts(self) -> (String, Value) {
        let entries = self
            .exports
            .into_iter()
            .map(|(name, value)| (MapKey::String(name), value))
            .collect();
        (self.name, Value::Map(Gc::new(GcCell::new(entries))))
    }
}

pub struct ModuleBuilder {
    name: String,
    exports: Vec<(String, Value)>,
}

impl ModuleBuilder {
    pub fn function<F>(mut self, name: impl Into<String>, arity: usize, callback: F) -> Self
    where
        F: Fn(&[Value], Span) -> NativeResult + Send + Sync + 'static,
    {
        let name = name.into();
        let qualified_name = format!("{}.{}", self.name, name);
        let function = NativeFunction::new(qualified_name, arity, Arc::new(callback));
        self.insert(name, Value::NativeFunction(function));
        self
    }

    pub fn value(mut self, name: impl Into<String>, value: Value) -> Self {
        self.insert(name.into(), value);
        self
    }

    pub fn build(self) -> Module {
        Module {
            name: self.name,
            exports: self.exports,
        }
    }

    fn insert(&mut self, name: String, value: Value) {
        if matches!(value, Value::Nil) {
            if let Some(position) = self
                .exports
                .iter()
                .position(|(existing, _)| existing == &name)
            {
                self.exports.remove(position);
            }
        } else if let Some((_, existing)) = self
            .exports
            .iter_mut()
            .find(|(existing, _)| existing == &name)
        {
            *existing = value;
        } else {
            self.exports.push((name, value));
        }
    }
}
