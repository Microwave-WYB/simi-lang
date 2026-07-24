use std::sync::Arc;

use gc::{Gc, GcCell};

use crate::runtime::{MapKey, NativeResult, Value};
use crate::span::Span;
use crate::value::NativeFunction;

pub type NativeCallback = dyn Fn(&[Value], Span) -> NativeResult + Send + Sync + 'static;

pub(crate) enum ModuleContents {
    Direct(Vec<(String, Value)>),
    Source { source: Arc<str>, host: Value },
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
            host: direct_value(Vec::new()),
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

    /// Build the configured fields as an ordinary Simi map value.
    pub fn build_value(self) -> Value {
        direct_value(self.exports)
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
    host: Value,
}

impl SourceModuleBuilder {
    /// Replace the private `host` value installed before evaluating the facade source.
    pub fn host(mut self, host: Value) -> Self {
        self.host = host;
        self
    }

    pub fn build(self) -> Module {
        Module {
            name: self.name,
            contents: ModuleContents::Source {
                source: self.source,
                host: self.host,
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

/// Build the common map-shaped private host value for a source-backed module.
///
/// Functions are ordinary fixed-arity native values. The required `name` prefixes their rendered
/// names and runtime diagnostics; it does not add a map field. Values use normal Simi map insertion
/// rules: duplicate fields are last-wins and `nil` removes a field. An arbitrary non-map private
/// host can instead be passed directly to [`SourceModuleBuilder::host`].
#[macro_export]
macro_rules! host_value {
    (
        name: $name:expr,
        $(functions: { $($function:expr => ($arity:expr, $callback:expr)),* $(,)? },)?
        $(values: { $($value:expr => $expression:expr),* $(,)? },)?
    ) => {{
        let builder = $crate::Module::builder($name);
        $(
            $(let builder = builder.function($function, $arity, $callback);)*
        )?
        $(
            $(let builder = builder.value($value, $expression);)*
        )?
        builder.build_value()
    }};
}
