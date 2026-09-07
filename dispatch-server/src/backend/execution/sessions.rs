use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::watch;

use crate::{
    backend::{execution::bounded_output, storage::utc_now},
    shared::view_models::{AgentRunOutputPiece, AgentToolName, ProcessSessionView},
};

#[cfg(test)]
use crate::shared::view_models::AgentRunOutputKind;

const MAX_SESSION_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct ProcessSessionRegistry {
    events: crate::backend::events::UiEventBus,
    state: Arc<Mutex<ProcessSessionState>>,
}

#[derive(Debug, Default)]
struct ProcessSessionState {
    sessions: HashMap<i64, ProcessSession>,
    project_lifecycle: HashMap<i64, DeletionState>,
    run_lifecycle: HashMap<(i64, i64), DeletionState>,
    codex_maintenance_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeletionState {
    Deleting,
    Deleted,
}

#[derive(Debug)]
pub(crate) enum ProcessSessionRegistration {
    Registered { cancellation: watch::Receiver<bool> },
    RejectedByProjectDeletion,
    RejectedByRunDeletion,
    RejectedByCodexMaintenance,
}

impl ProcessSessionRegistration {
    pub(crate) fn cancellation_requested(&self) -> bool {
        match self {
            Self::Registered { cancellation } => *cancellation.borrow(),
            Self::RejectedByRunDeletion
            | Self::RejectedByProjectDeletion
            | Self::RejectedByCodexMaintenance => true,
        }
    }

    pub(crate) async fn wait_for_cancellation(&mut self) {
        let Self::Registered { cancellation } = self else {
            return;
        };
        loop {
            if *cancellation.borrow() {
                return;
            }
            if cancellation.changed().await.is_err() {
                return;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_registered(&self) -> bool {
        matches!(self, Self::Registered { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionAdmissionRejection {
    InProgress,
    AlreadyDeleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodexMaintenanceAdmissionRejection {
    ActiveSessions(usize),
    InProgress,
}

#[derive(Debug)]
pub(crate) struct CodexMaintenanceAdmission {
    registry: ProcessSessionRegistry,
}

impl Drop for CodexMaintenanceAdmission {
    fn drop(&mut self) {
        self.registry.lock_state().codex_maintenance_active = false;
    }
}

#[derive(Debug)]
pub(crate) struct ProjectDeletionAdmission {
    registry: ProcessSessionRegistry,
    project_id: i64,
    completed: bool,
}

impl ProjectDeletionAdmission {
    pub(crate) fn mark_deleted(mut self) {
        self.registry.mark_project_deleted(self.project_id);
        self.completed = true;
    }
}

impl Drop for ProjectDeletionAdmission {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        self.registry.abort_project_deletion(self.project_id);
    }
}

impl ProcessSessionRegistry {
    pub fn new(events: crate::backend::events::UiEventBus) -> Self {
        Self {
            events,
            state: Arc::new(Mutex::new(ProcessSessionState::default())),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, ProcessSessionState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn begin(&self, start: ProcessSessionStart) -> ProcessSessionRegistration {
        let mut state = self.lock_state();
        if state.project_lifecycle.contains_key(&start.project_id) {
            return ProcessSessionRegistration::RejectedByProjectDeletion;
        }
        if state
            .run_lifecycle
            .contains_key(&(start.project_id, start.run_id))
        {
            return ProcessSessionRegistration::RejectedByRunDeletion;
        }
        if state.codex_maintenance_active && start.tool_name == AgentToolName::Codex.as_storage() {
            return ProcessSessionRegistration::RejectedByCodexMaintenance;
        }

        let now = utc_now();
        let project_name = start.project_name.clone();
        let run_id = start.run_id;
        if let Some(session) = state.sessions.get_mut(&run_id) {
            session.tool_name = start.tool_name;
            session.command = start.command;
            session.working_dir = start.working_dir;
            session.updated_at = now;
            return ProcessSessionRegistration::Registered {
                cancellation: session.cancel_tx.subscribe(),
            };
        }
        let (cancel_tx, cancellation) = watch::channel(false);
        let session = ProcessSession {
            run_id: start.run_id,
            project_id: start.project_id,
            project_name: start.project_name,
            tool_name: start.tool_name,
            command: start.command,
            working_dir: start.working_dir,
            process_id: None,
            output: Vec::new(),
            cancel_tx,
            started_at: now.clone(),
            updated_at: now,
        };
        state.sessions.insert(session.run_id, session);
        drop(state);
        self.events
            .publish_agent_run_changed(&project_name, run_id, None);
        ProcessSessionRegistration::Registered { cancellation }
    }

    /// Runs a synchronous project-runtime registration while holding the same lifecycle boundary
    /// used by deletion admission. Deletion either observes the completed registration and stops
    /// it, or closes admission first and causes the registration to be rejected.
    pub(crate) fn with_project_start_admitted<T>(
        &self,
        project_id: i64,
        register: impl FnOnce() -> T,
    ) -> Result<T, DeletionAdmissionRejection> {
        let state = self.lock_state();
        match state.project_lifecycle.get(&project_id) {
            None => Ok(register()),
            Some(DeletionState::Deleting) => Err(DeletionAdmissionRejection::InProgress),
            Some(DeletionState::Deleted) => Err(DeletionAdmissionRejection::AlreadyDeleted),
        }
    }

    pub fn append_output_piece(&self, run_id: i64, piece: AgentRunOutputPiece) {
        let mut state = self.lock_state();
        let project_name = if let Some(session) = state.sessions.get_mut(&run_id) {
            bounded_output::push_with_limit(&mut session.output, piece, MAX_SESSION_OUTPUT_BYTES);
            session.updated_at = utc_now();
            Some(session.project_name.clone())
        } else {
            None
        };
        drop(state);
        if let Some(project_name) = project_name {
            self.events
                .publish_agent_output_changed(&project_name, run_id, None);
        }
    }

    pub fn finish(&self, run_id: i64) {
        let session = self.lock_state().sessions.remove(&run_id);
        if let Some(session) = session {
            self.events
                .publish_agent_run_changed(&session.project_name, run_id, None);
        }
    }

    pub fn list_for_project(&self, project_id: i64) -> Vec<ProcessSessionView> {
        let mut sessions = self
            .lock_state()
            .sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .map(ProcessSessionView::from)
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.run_id);
        sessions
    }

    pub(crate) fn active_run_ids_for_project(&self, project_id: i64) -> Vec<i64> {
        let mut run_ids = self
            .lock_state()
            .sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .map(|session| session.run_id)
            .collect::<Vec<_>>();
        run_ids.sort_unstable();
        run_ids
    }

    pub fn get_for_project(&self, project_id: i64, run_id: i64) -> Option<ProcessSessionView> {
        self.lock_state()
            .sessions
            .get(&run_id)
            .filter(|session| session.project_id == project_id)
            .map(ProcessSessionView::from)
    }

    pub fn list_all(&self) -> Vec<ProcessSessionView> {
        let mut sessions = self
            .lock_state()
            .sessions
            .values()
            .map(ProcessSessionView::from)
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.run_id);
        sessions
    }

    /// Closes Codex session admission only when there are no existing Codex sessions.
    ///
    /// Holding the returned guard keeps new Codex runs from crossing the same synchronized
    /// boundary while managed-home maintenance validates and removes files.
    pub(crate) fn begin_codex_maintenance(
        &self,
    ) -> Result<CodexMaintenanceAdmission, CodexMaintenanceAdmissionRejection> {
        let mut state = self.lock_state();
        if state.codex_maintenance_active {
            return Err(CodexMaintenanceAdmissionRejection::InProgress);
        }
        let active_sessions = state
            .sessions
            .values()
            .filter(|session| session.tool_name == AgentToolName::Codex.as_storage())
            .count();
        if active_sessions > 0 {
            return Err(CodexMaintenanceAdmissionRejection::ActiveSessions(
                active_sessions,
            ));
        }
        state.codex_maintenance_active = true;
        Ok(CodexMaintenanceAdmission {
            registry: self.clone(),
        })
    }

    #[cfg(test)]
    pub(crate) fn codex_maintenance_active(&self) -> bool {
        self.lock_state().codex_maintenance_active
    }

    pub fn cancel_project(&self, project_id: i64) -> usize {
        let senders = self
            .lock_state()
            .sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .map(|session| session.cancel_tx.clone())
            .collect::<Vec<_>>();
        for sender in &senders {
            let _ = sender.send(true);
        }
        senders.len()
    }

    /// Prevents newly registered sessions for this project from starting and cancels existing
    /// sessions. The returned admission reopens session admission if it is dropped before deletion
    /// succeeds; successful deletion permanently closes admission for the immutable project id.
    pub(crate) fn begin_project_deletion(
        &self,
        project_id: i64,
    ) -> Result<ProjectDeletionAdmission, DeletionAdmissionRejection> {
        let mut state = self.lock_state();
        match state.project_lifecycle.entry(project_id) {
            Entry::Vacant(entry) => {
                entry.insert(DeletionState::Deleting);
            }
            Entry::Occupied(entry) => {
                return Err(match entry.get() {
                    DeletionState::Deleting => DeletionAdmissionRejection::InProgress,
                    DeletionState::Deleted => DeletionAdmissionRejection::AlreadyDeleted,
                });
            }
        }
        let senders = state
            .sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .map(|session| session.cancel_tx.clone())
            .collect::<Vec<_>>();
        drop(state);
        for sender in &senders {
            let _ = sender.send(true);
        }
        Ok(ProjectDeletionAdmission {
            registry: self.clone(),
            project_id,
            completed: false,
        })
    }

    fn mark_project_deleted(&self, project_id: i64) {
        self.lock_state()
            .project_lifecycle
            .insert(project_id, DeletionState::Deleted);
    }

    fn abort_project_deletion(&self, project_id: i64) {
        let mut state = self.lock_state();
        if state.project_lifecycle.get(&project_id) == Some(&DeletionState::Deleting) {
            state.project_lifecycle.remove(&project_id);
        }
    }

    pub fn cancel_all(&self) -> usize {
        let senders = self
            .lock_state()
            .sessions
            .values()
            .map(|session| session.cancel_tx.clone())
            .collect::<Vec<_>>();
        for sender in &senders {
            let _ = sender.send(true);
        }
        senders.len()
    }

    pub fn cancel_run(&self, project_name: &str, run_id: i64) -> bool {
        let sender = self
            .lock_state()
            .sessions
            .get(&run_id)
            .filter(|session| session.project_name == project_name)
            .map(|session| session.cancel_tx.clone());
        let Some(sender) = sender else {
            return false;
        };
        let _ = sender.send(true);
        true
    }
}

impl Default for ProcessSessionRegistry {
    fn default() -> Self {
        Self::new(crate::backend::events::UiEventBus::new())
    }
}

#[derive(Clone, Debug)]
pub struct ProcessSessionStart {
    pub run_id: i64,
    pub project_id: i64,
    pub project_name: String,
    pub tool_name: String,
    pub command: String,
    pub working_dir: String,
}

#[derive(Clone, Debug)]
struct ProcessSession {
    run_id: i64,
    project_id: i64,
    project_name: String,
    tool_name: String,
    command: String,
    working_dir: String,
    process_id: Option<i64>,
    output: Vec<AgentRunOutputPiece>,
    cancel_tx: watch::Sender<bool>,
    started_at: String,
    updated_at: String,
}

impl From<&ProcessSession> for ProcessSessionView {
    fn from(session: &ProcessSession) -> Self {
        Self {
            run_id: session.run_id,
            project_id: session.project_id,
            project_name: session.project_name.clone(),
            tool_name: session.tool_name.clone(),
            command: session.command.clone(),
            working_dir: session.working_dir.clone(),
            process_id: session.process_id,
            output: session.output.clone(),
            started_at: session.started_at.clone(),
            updated_at: session.updated_at.clone(),
        }
    }
}

#[cfg(test)]
fn test_piece(sequence: u64, body: &str) -> AgentRunOutputPiece {
    AgentRunOutputPiece {
        sequence,
        timestamp: utc_now(),
        kind: AgentRunOutputKind::ModelMessage,
        source: "test".to_owned(),
        item_id: None,
        title: "stdout".to_owned(),
        body: body.to_owned(),
        metadata: serde_json::json!({ "stream": "stdout" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[tokio::test]
    async fn session_output_is_retained_in_memory() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: "codex app-server".to_owned(),
            working_dir: "/tmp/demo".to_owned(),
        });

        sessions.append_output_piece(7, test_piece(1, "line one"));

        let active = sessions.list_for_project(1);
        assert_that!(&(active.len())).is_equal_to(1);
        assert_that!(&(active[0].output.len())).is_equal_to(1);
        assert_that!(&(active[0].output[0].kind)).is_equal_to(AgentRunOutputKind::ModelMessage);
        assert_that!(&(active[0].output[0].body)).is_equal_to("line one");
    }

    #[test]
    fn active_run_ids_do_not_materialize_session_output() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        for (run_id, project_id) in [(9, 1), (7, 1), (8, 2)] {
            sessions.begin(ProcessSessionStart {
                run_id,
                project_id,
                project_name: format!("project-{project_id}"),
                tool_name: "codex".to_owned(),
                command: "codex app-server".to_owned(),
                working_dir: "/tmp/demo".to_owned(),
            });
            sessions.append_output_piece(run_id, test_piece(1, "large session output"));
        }

        assert_that!(&(sessions.active_run_ids_for_project(1))).is_equal_to(vec![7, 9]);
    }

    #[tokio::test]
    async fn session_can_be_looked_up_and_cancelled_by_run() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let mut cancellation = sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: "codex app-server".to_owned(),
            working_dir: "/tmp/demo".to_owned(),
        });

        assert_that!(&(sessions.get_for_project(1, 7).is_some())).is_true();
        assert_that!(&(sessions.get_for_project(2, 7).is_none())).is_true();
        assert_that!(&(sessions.cancel_run("demo", 7))).is_true();
        assert_that!(&(!sessions.cancel_run("other", 7))).is_true();

        cancellation.wait_for_cancellation().await;
        assert_that!(&(cancellation.cancellation_requested())).is_true();
    }

    #[tokio::test]
    async fn successful_project_deletion_permanently_rejects_old_project_sessions() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let mut existing = sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: String::new(),
            working_dir: "/tmp/demo".to_owned(),
        });

        let deletion = sessions.begin_project_deletion(1).unwrap();
        existing.wait_for_cancellation().await;
        assert_that!(&(existing.cancellation_requested())).is_true();
        assert_that!(
            &(matches!(
                sessions.begin_project_deletion(1),
                Err(DeletionAdmissionRejection::InProgress)
            ))
        )
        .is_true();
        let other_deletion = sessions.begin_project_deletion(2).unwrap();
        drop(other_deletion);

        let during_deletion = sessions.begin(ProcessSessionStart {
            run_id: 8,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: String::new(),
            working_dir: "/tmp/demo".to_owned(),
        });
        assert_that!(&(during_deletion.cancellation_requested())).is_true();
        assert_that!(&(!during_deletion.is_registered())).is_true();
        assert_that!(&(sessions.get_for_project(1, 8).is_none())).is_true();
        assert_that!(&(sessions.list_for_project(1).len())).is_equal_to(1);

        deletion.mark_deleted();
        let after_deletion = sessions.begin(ProcessSessionStart {
            run_id: 9,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: String::new(),
            working_dir: "/tmp/demo".to_owned(),
        });
        assert_that!(&(after_deletion.cancellation_requested())).is_true();
        assert_that!(&(!after_deletion.is_registered())).is_true();
        assert_that!(&(sessions.get_for_project(1, 9).is_none())).is_true();
        assert_that!(
            &(matches!(
                sessions.begin_project_deletion(1),
                Err(DeletionAdmissionRejection::AlreadyDeleted)
            ))
        )
        .is_true();

        let replacement = sessions.begin(ProcessSessionStart {
            run_id: 10,
            project_id: 3,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: String::new(),
            working_dir: "/tmp/demo-replacement".to_owned(),
        });
        assert_that!(&(!replacement.cancellation_requested())).is_true();
        assert_that!(&(replacement.is_registered())).is_true();
    }

    #[tokio::test]
    async fn dropping_project_deletion_admission_reopens_session_admission() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let deletion = sessions.begin_project_deletion(1).unwrap();

        drop(deletion);

        let after_abort = sessions.begin(ProcessSessionStart {
            run_id: 9,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: "codex".to_owned(),
            command: String::new(),
            working_dir: String::new(),
        });
        assert_that!(&(!after_abort.cancellation_requested())).is_true();
        assert_that!(&(after_abort.is_registered())).is_true();
    }

    #[tokio::test]
    async fn codex_maintenance_refuses_active_sessions() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: AgentToolName::Codex.as_storage().to_owned(),
            command: String::new(),
            working_dir: String::new(),
        });

        let admission = sessions.begin_codex_maintenance();

        assert_that!(
            &(matches!(
                admission,
                Err(CodexMaintenanceAdmissionRejection::ActiveSessions(1))
            ))
        )
        .is_true();
    }

    #[tokio::test]
    async fn codex_maintenance_holds_run_admission_until_cleanup_finishes() {
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let maintenance = sessions.begin_codex_maintenance().unwrap();
        let concurrent_sessions = sessions.clone();
        let concurrent = std::thread::spawn(move || {
            concurrent_sessions.begin(ProcessSessionStart {
                run_id: 7,
                project_id: 1,
                project_name: "demo".to_owned(),
                tool_name: AgentToolName::Codex.as_storage().to_owned(),
                command: String::new(),
                working_dir: String::new(),
            })
        })
        .join()
        .unwrap();
        assert_that!(&(!concurrent.is_registered())).is_true();
        assert_that!(&(concurrent.cancellation_requested())).is_true();

        drop(maintenance);

        let admitted = sessions.begin(ProcessSessionStart {
            run_id: 8,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: AgentToolName::Codex.as_storage().to_owned(),
            command: String::new(),
            working_dir: String::new(),
        });
        assert_that!(&(admitted.is_registered())).is_true();
        assert_that!(&(!admitted.cancellation_requested())).is_true();
    }
}

/// Closes one run's registration while its deletion coordinator drains and retires it.
#[derive(Debug)]
pub(crate) struct RunDeletionAdmission {
    registry: ProcessSessionRegistry,
    project_id: i64,
    run_id: i64,
    completed: bool,
}
impl RunDeletionAdmission {
    pub(crate) fn mark_deleted(mut self) {
        self.registry
            .lock_state()
            .run_lifecycle
            .insert((self.project_id, self.run_id), DeletionState::Deleted);
        self.completed = true;
    }
}
impl Drop for RunDeletionAdmission {
    fn drop(&mut self) {
        if !self.completed {
            self.registry
                .lock_state()
                .run_lifecycle
                .remove(&(self.project_id, self.run_id));
        }
    }
}
impl ProcessSessionRegistry {
    pub(crate) fn begin_run_deletion(
        &self,
        project_id: i64,
        run_id: i64,
    ) -> Result<RunDeletionAdmission, DeletionAdmissionRejection> {
        match self.lock_state().run_lifecycle.entry((project_id, run_id)) {
            Entry::Vacant(entry) => {
                entry.insert(DeletionState::Deleting);
            }
            Entry::Occupied(entry) => {
                return Err(match entry.get() {
                    DeletionState::Deleting => DeletionAdmissionRejection::InProgress,
                    DeletionState::Deleted => DeletionAdmissionRejection::AlreadyDeleted,
                });
            }
        }
        Ok(RunDeletionAdmission {
            registry: self.clone(),
            project_id,
            run_id,
            completed: false,
        })
    }
}
