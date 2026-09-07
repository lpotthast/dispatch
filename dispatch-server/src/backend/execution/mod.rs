//! Prepared agent execution, runtime adapters, tools, sessions, and workspaces.
pub(crate) mod bounded_output;
pub(crate) mod cli;
pub(crate) mod codex;
pub(crate) mod git;
pub(crate) mod identity;
pub(crate) mod model;
pub(crate) mod output;
pub(crate) mod process_identity;
pub(crate) mod prompt_text;
pub(crate) mod runtime;
pub(crate) mod service;
pub(crate) mod sessions;
pub(crate) mod tools;
pub(crate) mod workspaces;
pub(crate) use model::AgentProcessStart;
pub(crate) use runtime::{automation_failure_message, automation_log_dir, is_automation_cancelled};
pub(crate) use service::AgentExecutionService;
