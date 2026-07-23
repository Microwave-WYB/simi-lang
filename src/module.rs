use std::collections::HashMap;
use std::sync::Arc;

use gc::{Gc, GcCell};

use crate::runtime::{MapKey, NativeResult, Value};
use crate::span::Span;
use crate::value::NativeFunction;

pub type NativeCallback = dyn Fn(&[Value], Span) -> NativeResult + Send + Sync + 'static;

#[derive(Clone)]
pub(crate) enum HostOperation {
    Callback {
        arity: usize,
        callback: Arc<NativeCallback>,
    },
    ListMap,
    ListFilter,
    ListFold,
    ListFind,
    ListFindIndex,
    ListAny,
    ListAll,
    ListEach,
    ListCount,
}

impl HostOperation {
    pub(crate) fn arity(&self) -> usize {
        match self {
            Self::Callback { arity, .. } => *arity,
            Self::ListFold => 3,
            Self::ListMap
            | Self::ListFilter
            | Self::ListFind
            | Self::ListFindIndex
            | Self::ListAny
            | Self::ListAll
            | Self::ListEach
            | Self::ListCount => 2,
        }
    }
}

pub(crate) enum ModuleContents {
    Direct(Vec<(String, Value)>),
    Source {
        source: Arc<str>,
        host_operations: HashMap<String, HostOperation>,
    },
}

pub struct Module {
    name: String,
    contents: ModuleContents,
}

impl Module {
    pub fn builder(name: impl Into<String>) -> ModuleBuilder {
        ModuleBuilder {
            name: name.into(),
            exports: Vec::new(),
        }
    }

    pub fn source(name: impl Into<String>, source: impl Into<Arc<str>>) -> SourceModuleBuilder {
        SourceModuleBuilder {
            name: name.into(),
            source: source.into(),
            host_operations: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_source_backed(&self) -> bool {
        matches!(self.contents, ModuleContents::Source { .. })
    }

    pub(crate) fn into_parts(self) -> (String, ModuleContents) {
        (self.name, self.contents)
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
            contents: ModuleContents::Direct(self.exports),
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

pub struct SourceModuleBuilder {
    name: String,
    source: Arc<str>,
    host_operations: HashMap<String, HostOperation>,
}

impl SourceModuleBuilder {
    pub fn host_function<F>(mut self, id: impl Into<String>, arity: usize, callback: F) -> Self
    where
        F: Fn(&[Value], Span) -> NativeResult + Send + Sync + 'static,
    {
        self.host_operations.insert(
            id.into(),
            HostOperation::Callback {
                arity,
                callback: Arc::new(callback),
            },
        );
        self
    }

    pub(crate) fn host_intrinsic(
        mut self,
        id: impl Into<String>,
        operation: HostOperation,
    ) -> Self {
        self.host_operations.insert(id.into(), operation);
        self
    }

    pub fn build(self) -> Module {
        Module {
            name: self.name,
            contents: ModuleContents::Source {
                source: self.source,
                host_operations: self.host_operations,
            },
        }
    }
}

pub(crate) fn direct_value(exports: Vec<(String, Value)>) -> Value {
    let entries = exports
        .into_iter()
        .map(|(name, value)| (MapKey::String(name), value))
        .collect();
    Value::Map(Gc::new(GcCell::new(entries)))
}
