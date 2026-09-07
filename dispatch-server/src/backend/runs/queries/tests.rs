pub(crate) fn service_with_sessions(
    sessions: crate::backend::execution::sessions::ProcessSessionRegistry,
    store: &crate::backend::storage::Store,
) -> std::sync::Arc<super::service::RunQueryService> {
    use crate::backend::{projects::repository::ProjectRepository, storage::TransactionManager};
    use std::sync::Arc;
    Arc::new(super::service::RunQueryService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(super::repository::RunQueryRepository),
        Arc::new(super::runtime::RunArtifacts),
        sessions,
        crate::backend::execution::tools::tests::service(store),
        Arc::new(crate::backend::runs::admission::repository::RunAdmissionRepository),
    ))
}

pub(crate) fn service(
    store: &crate::backend::storage::Store,
) -> std::sync::Arc<super::service::RunQueryService> {
    service_with_sessions(
        crate::backend::execution::sessions::ProcessSessionRegistry::new(
            crate::backend::events::UiEventBus::new(),
        ),
        store,
    )
}
