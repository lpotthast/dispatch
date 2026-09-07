pub(crate) mod controller;
pub(crate) mod transport;
use crate::{backend::storage::utc_now, shared::view_models::UiEvent};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::broadcast;

const EVENT_BUFFER_SIZE: usize = 1024;

#[derive(Clone, Debug)]
pub(crate) struct UiEventBus {
    inner: Arc<EventChannel>,
}

#[derive(Debug)]
struct EventChannel {
    sender: broadcast::Sender<UiEvent>,
    next_sequence: AtomicU64,
}

impl Default for UiEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl UiEventBus {
    pub(crate) fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        Self {
            inner: Arc::new(EventChannel {
                sender,
                next_sequence: AtomicU64::new(1),
            }),
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<UiEvent> {
        self.inner.sender.subscribe()
    }

    fn publish(&self, build: impl FnOnce(u64, String) -> UiEvent) {
        let sequence = self.inner.next_sequence.fetch_add(1, Ordering::Relaxed);
        let _ = self.inner.sender.send(build(sequence, utc_now()));
    }
    pub(crate) fn publish_project_list_changed(&self) {
        self.publish(|sequence, timestamp| UiEvent::ProjectListChanged {
            sequence,
            timestamp,
        });
    }

    pub(crate) fn publish_project_changed(&self, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::ProjectChanged {
            sequence,
            timestamp,
            project,
        });
    }

    pub(crate) fn publish_project_deleted(&self, project_id: i64, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::ProjectDeleted {
            sequence,
            timestamp,
            project_id,
            project,
        });
    }

    pub(crate) fn publish_system_prompt_changed(&self, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::SystemPromptChanged {
            sequence,
            timestamp,
            project,
        });
    }

    pub(crate) fn publish_work_item_changed(&self, project: &str, item_id: i64) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::WorkItemChanged {
            sequence,
            timestamp,
            project,
            item_id,
        });
    }

    pub(crate) fn publish_comment_changed(&self, project: &str, item_id: i64) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::CommentChanged {
            sequence,
            timestamp,
            project,
            item_id,
        });
    }

    pub(crate) fn publish_swim_lane_changed(&self, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::SwimLaneChanged {
            sequence,
            timestamp,
            project,
        });
    }

    pub(crate) fn publish_work_item_state_changed(&self, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::WorkItemStateChanged {
            sequence,
            timestamp,
            project,
        });
    }

    pub(crate) fn publish_label_key_changed(&self, project: &str, key: &str) {
        let project = project.to_owned();
        let key = key.to_owned();
        self.publish(|sequence, timestamp| UiEvent::LabelKeyChanged {
            sequence,
            timestamp,
            project,
            key,
        });
    }

    pub(crate) fn publish_agent_tool_changed(&self) {
        self.publish(|sequence, timestamp| UiEvent::AgentToolChanged {
            sequence,
            timestamp,
        });
    }

    pub(crate) fn publish_automation_changed(&self, project: &str) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::AutomationChanged {
            sequence,
            timestamp,
            project,
        });
    }

    pub(crate) fn publish_agent_run_changed(
        &self,
        project: &str,
        run_id: i64,
        item_id: Option<i64>,
    ) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::AgentRunChanged {
            sequence,
            timestamp,
            project,
            run_id,
            item_id,
        });
    }

    pub(crate) fn publish_agent_output_changed(
        &self,
        project: &str,
        run_id: i64,
        item_id: Option<i64>,
    ) {
        let project = project.to_owned();
        self.publish(|sequence, timestamp| UiEvent::AgentOutputChanged {
            sequence,
            timestamp,
            project,
            run_id,
            item_id,
        });
    }

    pub(crate) fn publish_codex_status_changed(&self) {
        self.publish(|sequence, timestamp| UiEvent::CodexStatusChanged {
            sequence,
            timestamp,
        });
    }
}
