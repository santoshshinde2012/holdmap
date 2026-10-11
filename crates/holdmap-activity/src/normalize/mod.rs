//! Metadata adapters share one validation and sanitization boundary.

mod hooks;
mod metadata;
mod otlp;

pub use hooks::normalize_hook;
pub use metadata::pseudonymize_id;
pub(crate) use metadata::{normalized, Batch};
pub(crate) use otlp::otlp;
