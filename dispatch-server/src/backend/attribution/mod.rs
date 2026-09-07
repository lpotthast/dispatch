pub(crate) mod model;
pub(crate) mod repository;
pub(crate) mod service;
pub(crate) mod transport;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn tests_service(store: &crate::backend::storage::Store) -> service::AttributionService {
    use std::sync::Arc;
    service::AttributionService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(crate::backend::projects::repository::ProjectRepository::new(store.db())),
        Arc::new(repository::AttributionRepository::new()),
    )
}
