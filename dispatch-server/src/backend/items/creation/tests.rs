use crate::backend::{
    attribution::{repository::AttributionRepository, service::AttributionService},
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{Store, TransactionManager},
};
use std::sync::Arc;
pub(crate) fn service(
    store: &Store,
    events: UiEventBus,
) -> Arc<super::service::ItemCreationService> {
    let transactions = Arc::new(TransactionManager::new(store));
    let projects = Arc::new(ProjectRepository::new(store.db()));
    let attribution = Arc::new(AttributionService::new(
        transactions.clone(),
        projects.clone(),
        Arc::new(AttributionRepository::new()),
    ));
    Arc::new(super::service::ItemCreationService::new(
        transactions,
        projects,
        Arc::new(super::repository::ItemCreationRepository),
        attribution,
        events,
    ))
}
