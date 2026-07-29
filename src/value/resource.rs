use std::any::Any;
use std::sync::Arc;

/// An opaque host-owned value that can move through Simi without exposing its payload to scripts.
///
/// Resource payloads are constrained to `Send + Sync + 'static`, so safe resource values cannot
/// contain Simi's non-`Send` managed values as untraced edges. Native callbacks can recover a
/// payload with [`NativeResource::downcast_ref`]; Simi code can only transport and inspect it.
#[derive(Clone)]
pub struct NativeResource {
    type_label: Arc<str>,
    payload: Arc<dyn Any + Send + Sync>,
}

impl NativeResource {
    /// Creates an opaque resource with a host-defined, stable display label.
    pub fn new<T>(type_label: impl Into<Arc<str>>, payload: T) -> Self
    where
        T: Any + Send + Sync,
    {
        Self {
            type_label: type_label.into(),
            payload: Arc::new(payload),
        }
    }

    /// Returns the stable host-defined label used by [`Value::render`](super::Value::render).
    pub fn type_label(&self) -> &str {
        &self.type_label
    }

    /// Returns the payload when it has type `T`.
    ///
    /// This is a host-side API. Simi source has no resource downcast or method syntax.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }

    /// Returns whether this resource and `other` share the same host payload allocation.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.payload, &other.payload)
    }
}
