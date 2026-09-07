//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct ProjectController {
    pub(super) project_deletion: crate::backend::projects::deletion::ProjectDeletionService,
    pub(super) projects: Arc<crate::backend::projects::service::ProjectService>,
}
impl ProjectController {
    pub(crate) fn new(
        project_deletion: crate::backend::projects::deletion::ProjectDeletionService,
        projects: Arc<crate::backend::projects::service::ProjectService>,
    ) -> Self {
        Self {
            project_deletion,
            projects,
        }
    }
}
