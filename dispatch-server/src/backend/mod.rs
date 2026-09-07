//! Server-owned persistence, workflow services, automation, and transport adapters.
//!
//! `entities` mirrors SQLite closely and may contain text representations required by SeaORM and
//! CrudKit. Repositories decode those representations into shared enums and typed domain records
//! before services apply workflow policy.

pub(crate) mod api;
pub(crate) mod app_state;
pub(crate) mod attribution;
pub(crate) mod automation;
pub(crate) mod board;
pub(crate) mod comments;
pub(crate) mod crudkit_resources;
pub(crate) mod entities;
pub(crate) mod events;
pub(crate) mod http;
pub(crate) mod items;
pub(crate) mod knowledge;
pub(crate) mod metrics;
pub(crate) mod migrations;
pub(crate) mod operator;
pub(crate) mod projects;
pub(crate) mod relationships;
pub(crate) mod runs;
pub(crate) mod server;
pub(crate) mod storage;

pub(crate) mod application;
pub(crate) mod entry;
pub(crate) mod execution;

#[cfg(test)]
mod architecture_tests;

#[cfg(test)]
mod boundary_tests;
