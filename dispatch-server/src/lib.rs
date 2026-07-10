//! Dispatch's server-rendered and hydrated web application.
//!
//! With the `ssr` feature, [`backend`] owns persistence, workflow services, automation, and HTTP
//! routing. [`frontend`] contains the Leptos operator UI, while [`shared`] re-exports transport
//! types used on both native and WebAssembly targets. SQLite entity models stay inside the
//! backend; browser and API code receive validated view models instead.

#![recursion_limit = "256"]
#![cfg_attr(feature = "ssr", allow(dead_code))]

#[cfg(feature = "ssr")]
pub(crate) mod backend;
pub mod frontend;
pub mod shared;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::hydrate_body(frontend::App);
}
