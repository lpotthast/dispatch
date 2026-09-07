use super::{
    model::{CreateLabelKey, LabelKeyRecord, UpdateLabelKey},
    repository::CatalogRepository,
};
use crate::backend::{
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{TransactionManager, utc_now},
};
use rootcause::{Result, prelude::*};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) struct CatalogService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<CatalogRepository>,
    events: UiEventBus,
}
impl CatalogService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<CatalogRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            events,
        }
    }
    pub(crate) async fn accent_colors(&self, project_id: i64) -> Result<BTreeMap<String, String>> {
        let transaction = self.transactions.begin().await?;
        let colors = self
            .repository
            .accent_colors_in(&transaction, project_id)
            .await?;
        transaction.commit().await?;
        Ok(colors)
    }
    // CrudKit adapts this read-only validation to its existing 422 response. Creation rechecks inside its write transaction.
    pub(crate) async fn exists(&self, project_id: i64, key: &str) -> Result<bool> {
        let transaction = self.transactions.begin().await?;
        let exists = self
            .repository
            .exists_in(&transaction, project_id, key)
            .await?;
        transaction.commit().await?;
        Ok(exists)
    }
    pub(crate) async fn create(
        &self,
        project_id: i64,
        input: CreateLabelKey,
    ) -> Result<LabelKeyRecord> {
        let input = input.validate()?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        if self
            .repository
            .exists_in(&transaction, project_id, &input.key)
            .await?
        {
            bail!("label key '{}' already exists", input.key);
        }
        let record = self
            .repository
            .insert_in(&transaction, project_id, input)
            .await?;
        transaction.commit().await?;
        self.events.publish_label_key_changed(&project, &record.key);
        Ok(record)
    }
    pub(crate) async fn update(
        &self,
        project_id: i64,
        id: i64,
        input: UpdateLabelKey,
    ) -> Result<LabelKeyRecord> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        let input = input.validate(record.built_in, &record.key)?;
        record.accent_color = input.accent_color;
        record.persistent = input.persistent;
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        self.repository
            .forget_if_unused_in(&transaction, project_id, &record.key)
            .await?;
        transaction.commit().await?;
        self.events.publish_label_key_changed(&project, &record.key);
        Ok(record)
    }
}
