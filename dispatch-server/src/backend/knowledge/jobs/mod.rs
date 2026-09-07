//! Bounded discovery with persisted admission, checkpoints and inspectable publication.
pub(crate) mod api;
mod evaluation;
pub(crate) mod execution;
pub(crate) mod execution_files;
pub(crate) mod files;
mod model;
mod policy;
mod processing;
mod publication;
pub(crate) mod repository;
pub(crate) mod service;
mod sources;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod worker;
use dispatch_types::knowledge::jobs::*;
use files::atomic_json;
use model::{Record, Supplied};
use policy::{estimated_tokens, hash};
use rootcause::{Result, prelude::*};
use std::{fs, path::Path};

pub(crate) mod controller;
