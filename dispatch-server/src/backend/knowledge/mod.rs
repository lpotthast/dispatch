//! Deterministic current-working-copy documents and independently scheduled knowledge jobs.
mod discovery;
mod documents;
pub(crate) mod editing;
pub(crate) mod jobs;
pub(crate) mod policy;
pub(crate) mod queries;
pub(crate) mod repository;
pub(crate) mod runtime;
pub(crate) mod service;
pub(crate) mod transport;
pub(crate) use policy::{DEFAULT_KNOWLEDGE_DIRECTORY, normalize_knowledge_directory};
#[cfg(test)]
mod tests;

pub(crate) mod controller;
