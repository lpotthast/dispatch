//! SeaORM models for Dispatch-owned SQLite tables and CrudKit read views.
//!
//! These structs are persistence records rather than domain models. Text fields that encode enums
//! or structured settings are validated by their owning service when they cross out of this
//! module.

pub mod agent_run;
pub mod agent_tool;
pub mod automation_trigger;
pub mod comment;
pub mod personality;
pub mod project;
pub mod swim_lane;
pub mod work_item;
pub mod work_item_event;
pub mod work_item_label;
pub mod work_item_relationship;
pub mod work_item_state;
