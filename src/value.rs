mod list;
mod map;
mod raised;
mod render;

pub use list::List;

use std::sync::Arc;

use gc::{Finalize, Gc, GcCell, Trace, custom_trace};

use crate::ast::Block;
use crate::environment::Environment;
use crate::module::NativeCallback;
use crate::span::Span;

pub type RuntimeResult<T> = Result<T, RuntimeError>;
pub type ScriptResult = Result<Value, Raised>;
pub type NativeResult = RuntimeResult<ScriptResult>;
pub type SharedList = Gc<GcCell<List>>;
pub type SharedMap = Gc<GcCell<Vec<(MapKey, Value)>>>;
pub type SharedFunction = Gc<UserFunction>;
pub type NativeFn = fn(&[Value], Span) -> NativeResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FloatKey(u64);

impl FloatKey {
    pub fn new(value: f64) -> Option<Self> {
        if !value.is_finite() || value == 0.0 {
            return None;
        }
        const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
        const I64_END_F64: f64 = 9_223_372_036_854_775_808.0;
        if (I64_MIN_F64..I64_END_F64).contains(&value) {
            let integer = value as i64;
            if integer as f64 == value {
                return None;
            }
        }
        Some(Self(value.to_bits()))
    }

    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl Finalize for FloatKey {}
unsafe impl Trace for FloatKey {
    gc::unsafe_empty_trace!();
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MapKey {
    Int(i64),
    Float(FloatKey),
    String(String),
    Bool(bool),
}

impl Finalize for MapKey {}
unsafe impl Trace for MapKey {
    gc::unsafe_empty_trace!();
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Nil,
    List(SharedList),
    Map(SharedMap),
    Function(SharedFunction),
    NativeFunction(NativeFunction),
}

impl Finalize for Value {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for Value {
    custom_trace!(this, {
        match this {
            Self::List(list) => mark(list),
            Self::Map(map) => mark(map),
            Self::Function(function) => mark(function),
            Self::Int(_)
            | Self::Float(_)
            | Self::String(_)
            | Self::Bool(_)
            | Self::Nil
            | Self::NativeFunction(_) => {}
        }
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceFrame {
    pub function: String,
    pub call_span: Span,
}

#[derive(Clone)]
pub struct Raised {
    pub value: Value,
    pub origin: Span,
    pub frames: Vec<TraceFrame>,
    pub cause: Option<Box<Raised>>,
}

pub struct UserFunction {
    pub name: String,
    pub params: Vec<String>,
    pub body: Block,
    pub closure: Environment,
    pub(crate) source_domain: u64,
    pub(crate) module: Option<String>,
}

impl Finalize for UserFunction {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for UserFunction {
    custom_trace!(this, {
        mark(&this.closure);
    });
}

#[derive(Clone)]
pub struct NativeFunction {
    name: String,
    arity: usize,
    implementation: NativeImplementation,
}

#[derive(Clone)]
pub(crate) enum NativeImplementation {
    Callback(Arc<NativeCallback>),
    Require,
}

impl NativeFunction {
    pub fn new(name: impl Into<String>, arity: usize, callback: Arc<NativeCallback>) -> Self {
        Self {
            name: name.into(),
            arity,
            implementation: NativeImplementation::Callback(callback),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arity(&self) -> usize {
        self.arity
    }

    pub(crate) fn require() -> Self {
        Self {
            name: "require".to_owned(),
            arity: 1,
            implementation: NativeImplementation::Require,
        }
    }

    pub(crate) fn implementation(&self) -> &NativeImplementation {
        &self.implementation
    }
}

impl Finalize for NativeFunction {}
unsafe impl Trace for NativeFunction {
    // Callback closures are Send + Sync, which prevents safe captures of Simi's
    // non-Send managed values. The remaining require intrinsic is data-free.
    gc::unsafe_empty_trace!();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub span: Span,
    pub message: String,
}

#[cfg(test)]
mod gc_tests;
#[cfg(test)]
mod tests;
