use std::error::Error;
use std::fmt;

use gc::{Gc, GcCell};

use super::{MapKey, Raised, RuntimeError, TraceFrame, Value};
use crate::span::Span;

impl Raised {
    pub(crate) fn new(value: Value, origin: Span) -> Self {
        Self {
            value,
            origin,
            frames: Vec::new(),
            cause: None,
        }
    }

    pub(crate) fn division_by_zero(origin: Span) -> Self {
        Self::new(
            Value::Map(Gc::new(GcCell::new(vec![(
                MapKey::String("error".to_owned()),
                Value::String("division_by_zero".to_owned()),
            )]))),
            origin,
        )
    }

    pub(crate) fn index_out_of_bounds(
        index: i64,
        length: usize,
        origin: Span,
    ) -> Result<Self, RuntimeError> {
        let length = i64::try_from(length)
            .map_err(|_| RuntimeError::new(origin, "list length exceeds i64"))?;
        Ok(Self::new(
            Value::Map(Gc::new(GcCell::new(vec![
                (
                    MapKey::String("error".to_owned()),
                    Value::String("index_out_of_bounds".to_owned()),
                ),
                (MapKey::String("index".to_owned()), Value::Int(index)),
                (MapKey::String("length".to_owned()), Value::Int(length)),
            ]))),
            origin,
        ))
    }

    pub(crate) fn push_frame(&mut self, frame: TraceFrame) {
        self.frames.push(frame);
    }

    pub(crate) fn append_cause(&mut self, cause: Raised) {
        let mut tail = &mut self.cause;
        while let Some(existing) = tail {
            tail = &mut existing.cause;
        }
        *tail = Some(Box::new(cause));
    }
}

struct RenderedValue<'a>(&'a Value);

impl fmt::Debug for RenderedValue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.render())
    }
}

impl fmt::Debug for Raised {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Raised")
            .field("value", &RenderedValue(&self.value))
            .field("origin", &self.origin)
            .field("frames", &self.frames)
            .field("cause", &self.cause)
            .finish()
    }
}

impl fmt::Display for Raised {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "raised {}", self.value.render())
    }
}

impl Error for Raised {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn Error + 'static))
    }
}

impl RuntimeError {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RuntimeError {}
