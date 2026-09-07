pub(crate) fn service(
    store: &crate::backend::storage::Store,
) -> std::sync::Arc<super::service::RoutingService> {
    use crate::backend::{
        automation::rules::repository::RuleRepository, items::repository::ItemRepository,
        projects::repository::ProjectRepository, storage::TransactionManager,
    };
    use std::sync::Arc;
    Arc::new(super::service::RoutingService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(RuleRepository),
        Arc::new(ItemRepository),
        crate::backend::runs::admission::tests::service(store),
    ))
}
