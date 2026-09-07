use super::{
    policy::{DEFAULT_PERSONALITY_NAME, normalize_name},
    repository::PersonalityRepository,
};
use crate::backend::automation::ownership::{ManagedDeletion, ManagedObjectKey};
use crate::backend::{
    events::UiEventBus,
    projects::{ProjectReference, repository::ProjectRepository},
    storage::{Transaction, TransactionManager, utc_now},
};
use crate::shared::page_data::AutomationPersonalityInspectorView;
use dispatch_types::{
    AutomationPersonalityInput, PersonalityRevisionView, PersonalityView, RevisionChangeOperation,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;

pub(crate) struct PersonalityService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<PersonalityRepository>,
    events: UiEventBus,
}
impl PersonalityService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<PersonalityRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            events,
        }
    }
    async fn scope_in(
        &self,
        transaction: &Transaction,
        project: ProjectReference<'_>,
    ) -> Result<(i64, String)> {
        match project {
            ProjectReference::Name(name) => Ok((
                self.projects.id_in(transaction, name).await?,
                name.to_owned(),
            )),
            ProjectReference::Id(id) => Ok((id, self.projects.name_in(transaction, id).await?)),
        }
    }
    pub(crate) async fn list(&self, project: &str) -> Result<Vec<PersonalityView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn get(&self, project: &str, reference: &str) -> Result<PersonalityView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .find_in(&transaction, project_id, reference)
            .await?
            .ok_or_else(|| report!("personality '{reference}' does not exist in this project"))?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn get_by_id(&self, project_id: i64, id: i64) -> Result<PersonalityView> {
        let transaction = self.transactions.begin().await?;
        let result = self.repository.get_in(&transaction, project_id, id).await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn default_id(&self, project_id: i64) -> Result<i64> {
        let transaction = self.transactions.begin().await?;
        let result = self
            .repository
            .find_in(&transaction, project_id, DEFAULT_PERSONALITY_NAME)
            .await?
            .ok_or_else(|| report!("project {project_id} has no Default personality"))?;
        transaction.commit().await?;
        Ok(result.id)
    }

    pub(crate) async fn revision_id_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: Option<i64>,
    ) -> Result<Option<i64>> {
        let Some(id) = id else { return Ok(None) };
        Ok(Some(
            self.repository
                .get_in(transaction, project_id, id)
                .await?
                .current_revision_id
                .ok_or_else(|| report!("personality {id} has no current revision"))?,
        ))
    }
    pub(crate) async fn description_for_prompt_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: Option<i64>,
    ) -> Result<Option<String>> {
        let Some(id) = id else { return Ok(None) };
        let description = self
            .repository
            .get_in(transaction, project_id, id)
            .await?
            .personality_description;
        Ok((!description.trim().is_empty()).then_some(description))
    }

    async fn ensure_name_available_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        name: &str,
        except: Option<i64>,
    ) -> Result<()> {
        if self
            .repository
            .name_exists_in(transaction, project_id, name, except)
            .await?
        {
            bail!("personality name '{name}' already exists in this project");
        }
        Ok(())
    }
    pub(crate) async fn validate_edit(
        &self,
        project_id: i64,
        id: Option<i64>,
        input: &AutomationPersonalityInput,
    ) -> Result<()> {
        let name = normalize_name(input.name.clone())?;
        let transaction = self.transactions.begin().await?;
        self.projects.name_in(&transaction, project_id).await?;
        if let Some(id) = id {
            let existing = self.repository.get_in(&transaction, project_id, id).await?;
            Self::ensure_unmanaged(&existing, "individual editing")?;
        }
        self.ensure_name_available_in(&transaction, project_id, &name, id)
            .await?;
        transaction.commit().await
    }
    pub(crate) async fn create(
        &self,
        project: ProjectReference<'_>,
        input: AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        let name = normalize_name(input.name)?;
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        self.ensure_name_available_in(&transaction, project_id, &name, None)
            .await?;
        let now = utc_now();
        let record = self
            .repository
            .insert_in(
                &transaction,
                PersonalityView {
                    id: 0,
                    project_id,
                    name,
                    personality_description: input.description,
                    current_revision_id: None,
                    managed_bundle_key: None,
                    managed_object_key: None,
                    created_at: now.clone(),
                    updated_at: now,
                },
            )
            .await?;
        self.commit_revision(
            transaction,
            &project_name,
            record,
            RevisionChangeOperation::Create,
        )
        .await
    }
    pub(crate) async fn update(
        &self,
        project: ProjectReference<'_>,
        id: i64,
        input: AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        let name = normalize_name(input.name)?;
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        Self::ensure_unmanaged(&record, "individual editing")?;
        self.ensure_name_available_in(&transaction, project_id, &name, Some(id))
            .await?;
        record.name = name;
        record.personality_description = input.description;
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            &project_name,
            record,
            RevisionChangeOperation::Update,
        )
        .await
    }
    fn ensure_unmanaged(record: &PersonalityView, operation: &str) -> Result<()> {
        if record.managed_bundle_key.is_some() {
            bail!("bundle-managed personalities must be detached before {operation}");
        }
        Ok(())
    }
    async fn ensure_deletable_in(
        &self,
        transaction: &Transaction,
        record: &PersonalityView,
    ) -> Result<()> {
        if record.managed_bundle_key.is_some() {
            bail!("bundle-managed personalities cannot be deleted individually");
        }
        if record.name == DEFAULT_PERSONALITY_NAME {
            bail!("the Default personality cannot be deleted");
        }
        if let Some(rule) = self
            .repository
            .referencing_rule_in(transaction, record.project_id, record.id)
            .await?
        {
            bail!(
                "personality '{}' is referenced by automation trigger '{rule}'",
                record.name
            );
        }
        Ok(())
    }
    pub(crate) async fn validate_delete(&self, project_id: i64, id: i64) -> Result<()> {
        let transaction = self.transactions.begin().await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        self.ensure_deletable_in(&transaction, &record).await?;
        transaction.commit().await
    }
    pub(crate) async fn delete(&self, project: ProjectReference<'_>, id: i64) -> Result<u64> {
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        self.ensure_deletable_in(&transaction, &record).await?;
        let affected = self
            .repository
            .delete_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(&project_name);
        Ok(affected)
    }
    pub(crate) async fn detach(&self, project: &str, id: i64) -> Result<PersonalityView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        if record.managed_bundle_key.is_none() {
            bail!("personality is not bundle-managed");
        }
        record.managed_bundle_key = None;
        record.managed_object_key = None;
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            project,
            record,
            RevisionChangeOperation::Detach,
        )
        .await
    }
    pub(crate) async fn revisions(
        &self,
        project: &str,
        id: i64,
    ) -> Result<Vec<PersonalityRevisionView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .revisions_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn inspect(
        &self,
        project: &str,
        id: i64,
    ) -> Result<AutomationPersonalityInspectorView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let personality = self.repository.get_in(&transaction, project_id, id).await?;
        let revisions = self
            .repository
            .revisions_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        Ok(AutomationPersonalityInspectorView {
            personality,
            revisions,
        })
    }
    pub(crate) async fn restore(
        &self,
        project: &str,
        id: i64,
        revision_id: i64,
    ) -> Result<PersonalityView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        Self::ensure_unmanaged(&record, "restoring a revision")?;
        let revision = self
            .repository
            .revision_in(&transaction, project_id, id, revision_id)
            .await?;
        let name = normalize_name(revision.name)?;
        self.ensure_name_available_in(&transaction, project_id, &name, Some(id))
            .await?;
        record.name = name;
        record.personality_description = revision.personality_description;
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            project,
            record,
            RevisionChangeOperation::Restore,
        )
        .await
    }
    pub(crate) async fn apply_managed_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        ownership: &ManagedObjectKey,
        id: Option<i64>,
        input: AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        self.projects.name_in(transaction, project_id).await?;
        let name = normalize_name(input.name)?;
        self.ensure_name_available_in(transaction, project_id, &name, id)
            .await?;
        let now = utc_now();
        let record = if let Some(id) = id {
            let mut record = self.repository.get_in(transaction, project_id, id).await?;
            if !ownership.matches(
                record.managed_bundle_key.as_deref(),
                record.managed_object_key.as_deref(),
            ) {
                bail!("personality {id} does not belong to the requested bundle object");
            }
            record.name = name;
            record.personality_description = input.description;
            record.updated_at = now;
            self.repository.save_in(transaction, record).await?
        } else {
            self.repository
                .insert_in(
                    transaction,
                    PersonalityView {
                        id: 0,
                        project_id,
                        name,
                        personality_description: input.description,
                        current_revision_id: None,
                        managed_bundle_key: Some(ownership.bundle_key().into()),
                        managed_object_key: Some(ownership.object_key().into()),
                        created_at: now.clone(),
                        updated_at: now,
                    },
                )
                .await?
        };
        self.repository
            .record_revision_in(transaction, record, RevisionChangeOperation::BundleApply)
            .await
    }
    pub(crate) async fn delete_managed_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
        bundle_key: &str,
        reason: ManagedDeletion,
    ) -> Result<()> {
        let record = self.repository.get_in(transaction, project_id, id).await?;
        if record.managed_bundle_key.as_deref() != Some(bundle_key) {
            bail!("personality {id} does not belong to bundle '{bundle_key}'");
        }
        if record.name == DEFAULT_PERSONALITY_NAME {
            bail!("the Default personality cannot be deleted");
        }
        if let Some(rule) = self
            .repository
            .referencing_rule_in(transaction, project_id, id)
            .await?
        {
            match reason {
                ManagedDeletion::Reconcile => bail!(
                    "cannot delete managed personality '{}' while automation '{}' references it",
                    record.name,
                    rule
                ),
                ManagedDeletion::RemoveBundle => bail!(
                    "cannot remove bundle while automation '{}' outside the bundle references personality '{}'",
                    rule,
                    record.name
                ),
            }
        }
        self.repository
            .delete_in(transaction, project_id, id)
            .await?;
        Ok(())
    }
    async fn commit_revision(
        &self,
        transaction: Transaction,
        project: &str,
        record: PersonalityView,
        operation: RevisionChangeOperation,
    ) -> Result<PersonalityView> {
        let record = self
            .repository
            .record_revision_in(&transaction, record, operation)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(project);
        Ok(record)
    }
}
