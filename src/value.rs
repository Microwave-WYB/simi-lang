mod list;
mod raised;
mod render;
mod table;

pub use list::List;

use gc::{Finalize, Gc, GcCell, Trace, custom_trace};

use crate::ast::Block;
use crate::environment::Environment;
use crate::span::Span;

pub type RuntimeResult<T> = Result<T, RuntimeError>;
pub type ScriptResult = Result<Value, Raised>;
pub type NativeResult = RuntimeResult<ScriptResult>;
pub type SharedList = Gc<GcCell<List>>;
pub type SharedTable = Gc<GcCell<Vec<(TableKey, Value)>>>;
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
pub enum TableKey {
    Int(i64),
    Float(FloatKey),
    String(String),
    Bool(bool),
}

impl Finalize for TableKey {}
unsafe impl Trace for TableKey {
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
    Table(SharedTable),
    Function(SharedFunction),
    NativeFunction(NativeFunction),
}

impl Finalize for Value {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for Value {
    custom_trace!(this, {
        match this {
            Self::List(list) => mark(list),
            Self::Table(table) => mark(table),
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
}

impl Finalize for UserFunction {}
#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl Trace for UserFunction {
    custom_trace!(this, {
        mark(&this.closure);
    });
}

#[derive(Clone, Copy)]
pub struct NativeFunction {
    pub name: &'static str,
    pub arity: usize,
    pub call: NativeFn,
}

impl Finalize for NativeFunction {}
unsafe impl Trace for NativeFunction {
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
