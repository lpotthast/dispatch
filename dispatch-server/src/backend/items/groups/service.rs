use super::{
    policy::{normalize_group_key, normalize_group_name},
    repository::GroupRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use dispatch_types::{CreateWorkItemGroupRequest, WorkItemGroupView};
use rootcause::{Result, prelude::*};
use std::{collections::BTreeSet, sync::Arc};

pub(crate) struct GroupService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<GroupRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}
impl GroupService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<GroupRepository>,
        attribution: Arc<AttributionService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            attribution,
            events,
        }
    }
    pub(crate) async fn list(
        &self,
        project: &str,
        attribution: AttributionInput,
    ) -> Result<Vec<WorkItemGroupView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        self.attribution
            .validate_in(&transaction, project, attribution, false)
            .await?;
        let groups = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(groups)
    }
    pub(crate) async fn create(
        &self,
        project: &str,
        request: CreateWorkItemGroupRequest,
        attribution: AttributionInput,
    ) -> Result<WorkItemGroupView> {
        let key = normalize_group_key(request.key)?;
        let name = normalize_group_name(request.name)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, attribution, false)
            .await?;
        attribution.ensure_item_mutation("create work groups")?;
        let group = match self
            .repository
            .find_in(&transaction, project_id, &key)
            .await?
        {
            Some(existing) => {
                if existing.name != name {
                    bail!(
                        "work group '{key}' already exists as '{}'; use that name or choose another key",
                        existing.name
                    );
                }
                existing
            }
            None => {
                self.repository
                    .create_in(&transaction, project_id, key, name, attribution.event())
                    .await?
            }
        };
        transaction.commit().await?;
        Ok(group)
    }
    pub(crate) async fn assign(
        &self,
        project: &str,
        group_key: &str,
        item_ids: Vec<i64>,
        attribution: AttributionInput,
    ) -> Result<WorkItemGroupView> {
        let group_key = normalize_group_key(group_key.to_owned())?;
        let item_ids = item_ids.into_iter().collect::<BTreeSet<_>>();
        if item_ids.is_empty() {
            bail!("work-group assignment requires at least one item id");
        }
        if item_ids.iter().any(|id| *id <= 0) {
            bail!("work-group item ids must be positive");
        }
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, attribution, false)
            .await?;
        attribution.ensure_item_mutation("assign work-group items")?;
        let mut group = self
            .repository
            .find_in(&transaction, project_id, &group_key)
            .await?
            .ok_or_else(|| report!("work group '{group_key}' does not exist in this project"))?;
        let items = self
            .repository
            .items_in(&transaction, project_id, item_ids)
            .await?;
        let mut changed_ids = Vec::new();
        for item in items {
            if let Some(existing_group_id) = item.group_id
                && existing_group_id != group.id
            {
                bail!(
                    "item {} already belongs to work group {existing_group_id}; remove it before assigning another group",
                    item.id
                );
            }
            if item.group_id != Some(group.id) {
                changed_ids.push(item.id);
            }
        }
        self.repository
            .assign_in(
                &transaction,
                project_id,
                group.id,
                &group_key,
                &changed_ids,
                attribution.event(),
            )
            .await?;
        group.item_count = self
            .repository
            .count_in(&transaction, project_id, group.id)
            .await?;
        transaction.commit().await?;
        for id in changed_ids {
            self.events.publish_work_item_changed(project, id);
        }
        Ok(group)
    }
}
