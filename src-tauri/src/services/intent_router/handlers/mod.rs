//! Per-intent handlers. Each handler owns its retrieval shape, prompt, and
//! render template; all citations route through `services::citation`
//! (constitution §XI).
//!
//! Slice 0 ships empty stubs. Handlers fill in across Slices B/C/D.

pub mod action_items;
pub mod cross_compare;
pub mod difference;
pub mod fallback;
pub mod list_children;
pub mod lookup;
pub mod procedure;
pub mod summarize;
pub mod timeline;
