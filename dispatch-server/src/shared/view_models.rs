//! Transport and view-model types shared by the native server and hydrated frontend.
//!
//! The definitions live in `dispatch-types`; this module keeps server code independent of whether
//! the types are reached through the standalone crate or the Leptos application crate.

pub use dispatch_types::UiEvent;
pub use dispatch_types::*;
