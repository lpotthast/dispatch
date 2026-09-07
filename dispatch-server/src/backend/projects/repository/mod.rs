//! Project persistence and storage decoding. Scoped methods only use their supplied transaction.
pub(crate) mod encoding;
mod history;

use super::{model::ProjectChangeSource, settings::project_settings_to_view};
use crate::backend::{
    entities::project::{self, Project},
    storage::Transaction,
};
use dispatch_types::{ProjectSettingsView, ProjectSystemPromptEventView, ProjectView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use std::sync::Arc;

pub(crate) struct ProjectRepository {
    db: Arc<DatabaseConnection>,
}

impl ProjectRepository {
    pub(crate) fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub(crate) async fn list(&self) -> Result<Vec<ProjectView>> {
        Project::find()
            .order_by_asc(project::Column::Name)
            .all(self.db.as_ref())
            .await
            .context("failed to list projects")?
            .into_iter()
            .map(encoding::decode)
            .collect()
    }

    pub(crate) async fn by_name(&self, name: &str) -> Result<ProjectView> {
        self.by_name_on(self.db.as_ref(), name).await
    }

    pub(crate) async fn by_id(&self, id: i64) -> Result<ProjectView> {
        let model = Project::find_by_id(id)
            .one(self.db.as_ref())
            .await
            .context_with(|| format!("failed to load project {id}"))?
            .ok_or_else(|| report!("project {id} does not exist"))?;
        encoding::decode(model)
    }

    pub(crate) async fn resolve_in(
        &self,
        transaction: &Transaction,
        reference: super::model::ProjectReference<'_>,
    ) -> Result<ProjectView> {
        match reference {
            super::model::ProjectReference::Name(name) => self.by_name_in(transaction, name).await,
            super::model::ProjectReference::Id(id) => self.by_id_in(transaction, id).await,
        }
    }

    pub(crate) async fn by_name_in(
        &self,
        transaction: &Transaction,
        name: &str,
    ) -> Result<ProjectView> {
        self.by_name_on(transaction.connection(), name).await
    }

    pub(crate) async fn by_id_in(&self, transaction: &Transaction, id: i64) -> Result<ProjectView> {
        let model = Project::find_by_id(id)
            .one(transaction.connection())
            .await
            .context_with(|| format!("failed to load project {id}"))?
            .ok_or_else(|| report!("project {id} does not exist"))?;
        encoding::decode(model)
    }

    async fn by_name_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        name: &str,
    ) -> Result<ProjectView> {
        let model = Project::find()
            .filter(project::Column::Name.eq(name))
            .one(connection)
            .await
            .context_with(|| format!("failed to load project '{name}'"))?
            .ok_or_else(|| report!("project '{name}' does not exist"))?;
        encoding::decode(model)
    }

    pub(crate) async fn name_in(&self, transaction: &Transaction, id: i64) -> Result<String> {
        Ok(Project::find_by_id(id)
            .one(transaction.connection())
            .await
            .context_with(|| format!("failed to load project {id}"))?
            .ok_or_else(|| report!("project {id} does not exist"))?
            .name)
    }
    pub(crate) async fn id_in(&self, transaction: &Transaction, name: &str) -> Result<i64> {
        // Scope resolution does not depend on settings being valid, so operators can still repair them.
        Ok(Project::find()
            .filter(project::Column::Name.eq(name))
            .one(transaction.connection())
            .await
            .context_with(|| format!("failed to load project '{name}'"))?
            .ok_or_else(|| report!("project '{name}' does not exist"))?
            .id)
    }

    pub(crate) async fn id(&self, name: &str) -> Result<i64> {
        self.find_id(name)
            .await?
            .ok_or_else(|| report!("project '{name}' does not exist"))
    }

    pub(crate) async fn find_id(&self, name: &str) -> Result<Option<i64>> {
        Ok(Project::find()
            .filter(project::Column::Name.eq(name))
            .one(self.db.as_ref())
            .await
            .context_with(|| format!("failed to load project identity for '{name}'"))?
            .map(|project| project.id))
    }

    pub(crate) async fn name(&self, id: i64) -> Result<String> {
        self.find_name(id)
            .await?
            .ok_or_else(|| report!("project {id} does not exist"))
    }

    pub(crate) async fn find_name(&self, id: i64) -> Result<Option<String>> {
        Ok(Project::find_by_id(id)
            .one(self.db.as_ref())
            .await
            .context_with(|| format!("failed to load project {id}"))?
            .map(|project| project.name))
    }

    pub(crate) async fn settings(&self, name: &str) -> Result<ProjectSettingsView> {
        Ok(project_settings_to_view(self.by_name(name).await?))
    }

    pub(crate) async fn settings_in(
        &self,
        transaction: &Transaction,
        name: &str,
    ) -> Result<ProjectSettingsView> {
        Ok(project_settings_to_view(
            self.by_name_in(transaction, name).await?,
        ))
    }

    pub(crate) async fn settings_by_id_in(
        &self,
        transaction: &Transaction,
        id: i64,
    ) -> Result<ProjectSettingsView> {
        Ok(project_settings_to_view(
            self.by_id_in(transaction, id).await?,
        ))
    }

    pub(crate) async fn settings_by_id(&self, id: i64) -> Result<ProjectSettingsView> {
        Ok(project_settings_to_view(self.by_id(id).await?))
    }

    pub(super) async fn insert_in(
        &self,
        transaction: &Transaction,
        project: ProjectView,
        memory: String,
    ) -> Result<ProjectView> {
        let mut active = encoding::encode(project);
        active.id = NotSet;
        active.memory = Set(memory);
        active.knowledge_source_lineage_id = Set(None);
        encoding::decode(
            active
                .insert(transaction.connection())
                .await
                .context("failed to create project")?,
        )
    }

    pub(super) async fn save_in(
        &self,
        transaction: &Transaction,
        project: ProjectView,
    ) -> Result<ProjectView> {
        let name = project.name.clone();
        encoding::decode(
            encoding::encode(project)
                .update(transaction.connection())
                .await
                .context_with(|| format!("failed to update project '{name}'"))?,
        )
    }

    pub(super) async fn record_prompt_in(
        &self,
        transaction: &Transaction,
        project: &ProjectView,
        operation: &str,
        source: &ProjectChangeSource,
    ) -> Result<ProjectSystemPromptEventView> {
        let event = history::record_system_prompt_changed_in_tx(
            transaction.connection(),
            project,
            operation,
            source,
        )
        .await?;
        Ok(history::system_prompt_event_to_view(&project.name, event))
    }

    pub(super) async fn prompt_history_in(
        &self,
        transaction: &Transaction,
        project: &ProjectView,
    ) -> Result<Vec<ProjectSystemPromptEventView>> {
        history::list_system_prompt_events(transaction.connection(), project.id, &project.name)
            .await
    }

    pub(super) async fn clear_prompt_history_in(
        &self,
        transaction: &Transaction,
        id: i64,
    ) -> Result<u64> {
        history::clear_system_prompt_history(transaction.connection(), id).await
    }

    pub(crate) async fn latest_prompt_event_id_in(
        &self,
        transaction: &Transaction,
        id: i64,
    ) -> Result<Option<i64>> {
        Ok(
            history::latest_system_prompt_event(transaction.connection(), id)
                .await?
                .map(|event| event.id),
        )
    }
}

/// Persists the project's required initial catalog and automation records in its creation transaction.
pub(crate) struct ProjectDefaultsRepository;
impl ProjectDefaultsRepository {
    pub(crate) fn new() -> Self {
        Self
    }
    pub(super) async fn initialize_in(
        &self,
        transaction: &Transaction,
        project: &ProjectView,
    ) -> Result<()> {
        use crate::backend::{
            automation::rules as automation_triggers, board::lanes::repository as swim_lanes,
            items::labels::catalog::repository as label_keys,
            items::states::repository as work_item_states,
        };
        let connection = transaction.connection();
        label_keys::ensure_built_in_label_keys_in_conn(connection, project.id).await?;
        crate::backend::automation::personalities::repository::ensure_default_personality_in_conn(
            connection, project.id,
        )
        .await?;
        work_item_states::ensure_default_work_item_states_in_conn(connection, project.id).await?;
        swim_lanes::ensure_default_swim_lanes_in_conn(connection, project.id).await?;
        automation_triggers::repository::defaults::ensure_default_project_automations_in_conn(
            connection,
            project.id,
            project.default_agent_tool.as_storage(),
        )
        .await?;
        Ok(())
    }
}

impl ProjectRepository {
    pub(crate) async fn scope_by_name(&self, name: &str) -> Result<super::model::ProjectScope> {
        let model = Project::find()
            .filter(project::Column::Name.eq(name))
            .one(self.db.as_ref())
            .await
            .context_with(|| format!("failed to load project '{name}'"))?
            .ok_or_else(|| report!("project '{name}' does not exist"))?;
        Ok(super::model::ProjectScope {
            id: model.id,
            name: model.name,
            path: model.path,
        })
    }

    pub(crate) async fn scope_by_id_in(
        &self,
        transaction: &Transaction,
        id: i64,
    ) -> Result<super::model::ProjectScope> {
        let project = Project::find_by_id(id)
            .one(transaction.connection())
            .await?
            .ok_or_else(|| report!("project {id} does not exist"))?;
        Ok(super::model::ProjectScope {
            id: project.id,
            name: project.name,
            path: project.path,
        })
    }
    pub(crate) async fn scope_by_id(&self, id: i64) -> Result<super::model::ProjectScope> {
        let model = Project::find_by_id(id)
            .one(self.db.as_ref())
            .await
            .context_with(|| format!("failed to load project {id}"))?
            .ok_or_else(|| report!("project {id} does not exist"))?;
        Ok(super::model::ProjectScope {
            id: model.id,
            name: model.name,
            path: model.path,
        })
    }

    pub(crate) async fn run_artifacts_in(
        &self,
        transaction: &Transaction,
        project: &super::model::ProjectScope,
    ) -> Result<Vec<crate::backend::runs::model::RunArtifactLocations>> {
        use crate::backend::entities::agent_run::{self, AgentRun};
        use sea_orm::QuerySelect;
        let records = AgentRun::find()
            .select_only()
            .columns([
                agent_run::Column::Id,
                agent_run::Column::BranchName,
                agent_run::Column::WorktreePath,
            ])
            .filter(agent_run::Column::ProjectId.eq(project.id))
            .into_tuple::<(i64, Option<String>, Option<String>)>()
            .all(transaction.connection())
            .await
            .context_with(|| format!("failed to load runs for project '{}'", project.name))?;
        Ok(records
            .into_iter()
            .map(|(id, branch_name, worktree_path)| {
                crate::backend::runs::model::RunArtifactLocations {
                    id,
                    branch_name,
                    worktree_path,
                }
            })
            .collect())
    }

    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project: &super::model::ProjectScope,
    ) -> Result<()> {
        let deleted = Project::delete_by_id(project.id)
            .exec(transaction.connection())
            .await
            .context_with(|| format!("failed to delete project '{}'", project.name))?;
        if deleted.rows_affected != 1 {
            bail!(
                "project '{}' changed while it was being deleted; no row was removed",
                project.name
            );
        }
        Ok(())
    }
}

impl ProjectRepository {
    pub(super) async fn by_name_for_edit(&self, name: &str) -> Result<ProjectView> {
        self.edit_record_on(self.db.as_ref(), name).await
    }
    pub(super) async fn by_name_for_edit_in(
        &self,
        transaction: &Transaction,
        name: &str,
    ) -> Result<ProjectView> {
        self.edit_record_on(transaction.connection(), name).await
    }
    async fn edit_record_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        name: &str,
    ) -> Result<ProjectView> {
        let model = Project::find()
            .filter(project::Column::Name.eq(name))
            .one(connection)
            .await
            .context_with(|| format!("failed to load project '{name}'"))?
            .ok_or_else(|| report!("project '{name}' does not exist"))?;
        encoding::decode_for_edit(model)
    }
}
