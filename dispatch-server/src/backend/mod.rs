//! Server-owned persistence, workflow services, automation, and transport adapters.
//!
//! `entities` mirrors SQLite closely and may contain text representations required by SeaORM and
//! CrudKit. Domain services must decode those representations into the shared enum and view types
//! before applying workflow policy or returning data to clients.

pub(crate) mod agent_ids;
pub(crate) mod agent_tools;
pub(crate) mod api;
pub(crate) mod app_state;
pub(crate) mod automation;
pub(crate) mod automation_admission;
pub(crate) mod automation_bundles;
pub(crate) mod automation_cli;
pub(crate) mod automation_commit;
pub(crate) mod automation_controller;
pub(crate) mod automation_output;
pub(crate) mod automation_postconditions;
pub(crate) mod automation_prompt;
pub(crate) mod automation_revisions;
pub(crate) mod automation_routing;
pub(crate) mod automation_runtime;
pub(crate) mod automation_triggers;
pub(crate) mod automation_workspace;
pub(crate) mod codex_app_server;
pub(crate) mod comments;
pub(crate) mod crudkit_resources;
pub(crate) mod entities;
pub(crate) mod events;
pub(crate) mod http;
pub(crate) mod item_claims;
pub(crate) mod item_label_mutations;
pub(crate) mod item_label_service;
pub(crate) mod item_labels;
pub(crate) mod items;
pub(crate) mod label_conditions;
pub(crate) mod migrations;
pub(crate) mod page_data;
pub(crate) mod personalities;
pub(crate) mod process_sessions;
pub(crate) mod projects;
pub(crate) mod prompt_text;
pub(crate) mod relationship_mutations;
pub(crate) mod relationships;
pub(crate) mod request_attribution;
pub(crate) mod server;
pub(crate) mod storage;
pub(crate) mod swim_lanes;
pub(crate) mod work_item_comments;
pub(crate) mod work_item_creation;
pub(crate) mod work_item_events;
pub(crate) mod work_item_groups;
pub(crate) mod work_item_labels;
pub(crate) mod work_item_relationships;
pub(crate) mod work_item_states;
pub(crate) mod work_item_updates;
pub(crate) mod work_item_views;
pub(crate) mod work_items;
pub(crate) mod workflow_labels;
pub(crate) mod workspace;
