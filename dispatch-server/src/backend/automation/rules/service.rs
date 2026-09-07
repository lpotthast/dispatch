use super::{
    configuration::{
        next_evaluation_at, normalize_schedule, selector_for_activation,
        validate_trigger_configuration,
    },
    model::{CreateAutomationTrigger, PersonalityReference, RuleFields, UpdateAutomationTrigger},
    policy::validate_rule_policy,
    repository::RuleRepository,
};
use crate::backend::automation::ownership::ManagedObjectKey;
use crate::backend::{
    automation::personalities::{
        policy::DEFAULT_PERSONALITY_NAME, repository::PersonalityRepository,
    },
    events::UiEventBus,
    projects::{ProjectReference, repository::ProjectRepository},
    storage::{Transaction, TransactionManager, utc_now},
};
use dispatch_types::{
    AutomationActivation, AutomationEffect, AutomationRevisionView, AutomationRuleInput,
    AutomationTriggerView, RevisionChangeOperation,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;

pub(crate) struct RuleService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RuleRepository>,
    personalities: Arc<PersonalityRepository>,
    events: UiEventBus,
}
impl RuleService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RuleRepository>,
        personalities: Arc<PersonalityRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            personalities,
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
    pub(crate) async fn list(&self, project: &str) -> Result<Vec<AutomationTriggerView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn get(
        &self,
        project: &str,
        reference: &str,
    ) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .find_in(&transaction, project_id, reference)
            .await?;
        transaction.commit().await?;
        Ok(result)
    }
    fn normalize(mut fields: RuleFields) -> Result<RuleFields> {
        fields.schedule = normalize_schedule(fields.schedule)?;
        fields.selector = selector_for_activation(fields.activation, fields.selector)?;
        validate_trigger_configuration(
            &fields.name,
            fields.activation,
            fields.effect,
            &fields.schedule,
            fields.selector.as_ref(),
            &fields.prompt,
        )?;
        validate_rule_policy(
            fields.effect,
            fields.produced_work.as_ref(),
            &fields.execution,
            fields.postconditions.as_ref(),
        )?;
        Ok(fields)
    }
    async fn personality_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        fields: &RuleFields,
    ) -> Result<Option<i64>> {
        if fields.effect != AutomationEffect::ConsumeWork {
            return Ok(None);
        }
        let personality = match &fields.personality {
            Some(PersonalityReference::Id(id)) => {
                self.personalities
                    .get_in(transaction, project_id, *id)
                    .await?
            }
            reference => {
                let name = match reference {
                    Some(PersonalityReference::Name(name)) => name.as_str(),
                    _ => DEFAULT_PERSONALITY_NAME,
                };
                self.personalities
                    .find_named_in(transaction, project_id, name)
                    .await?
                    .ok_or_else(|| report!("personality '{name}' does not exist in this project"))?
            }
        };
        Ok(Some(personality.id))
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
            bail!("automation trigger name '{name}' already exists in this project");
        }
        Ok(())
    }
    fn ensure_unmanaged(record: &AutomationTriggerView, operation: &str) -> Result<()> {
        if record.managed_bundle_key.is_some() {
            bail!("bundle-managed automations must be detached before {operation}");
        }
        Ok(())
    }
    pub(crate) async fn validate_edit(
        &self,
        project_id: i64,
        id: Option<i64>,
        fields: RuleFields,
    ) -> Result<()> {
        let fields = Self::normalize(fields)?;
        let transaction = self.transactions.begin().await?;
        self.projects.name_in(&transaction, project_id).await?;
        if let Some(id) = id {
            Self::ensure_unmanaged(
                &self.repository.get_in(&transaction, project_id, id).await?,
                "individual editing",
            )?;
        }
        self.ensure_name_available_in(&transaction, project_id, &fields.name, id)
            .await?;
        self.personality_in(&transaction, project_id, &fields)
            .await?;
        transaction.commit().await
    }
    pub(crate) async fn validate_delete(&self, project_id: i64, id: i64) -> Result<()> {
        let transaction = self.transactions.begin().await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        if record.managed_bundle_key.is_some() {
            bail!("bundle-managed automations cannot be deleted individually");
        }
        transaction.commit().await
    }
    pub(crate) async fn create(
        &self,
        project: &str,
        input: CreateAutomationTrigger,
    ) -> Result<AutomationTriggerView> {
        self.create_configuration(ProjectReference::Name(project), input.into())
            .await
    }
    pub(crate) async fn create_from_input(
        &self,
        project: &str,
        input: AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.create_configuration(
            ProjectReference::Name(project),
            Self::operator_fields(input),
        )
        .await
    }
    fn operator_fields(input: AutomationRuleInput) -> RuleFields {
        RuleFields {
            name: input.name,
            enabled: input.enabled,
            activation: input.activation,
            effect: input.effect,
            schedule: input.schedule,
            tool_name: Some(input.tool_name),
            mutability: input.mutability,
            personality: input.personality.map(PersonalityReference::Name),
            prompt: crate::backend::execution::prompt_text::markdown_to_html(
                &input.prompt_markdown,
            ),
            selector: input.selector,
            priority: input.priority,
            exclusive: input.exclusive,
            produced_work: input.produced_work,
            execution: input.execution,
            postconditions: input.postconditions,
        }
    }
    pub(crate) async fn create_configuration(
        &self,
        project: ProjectReference<'_>,
        fields: RuleFields,
    ) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        let record = self
            .insert_configuration_in(&transaction, project_id, &project_name, fields, None)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(&project_name);
        Ok(record)
    }
    async fn insert_configuration_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        project_name: &str,
        fields: RuleFields,
        ownership: Option<&ManagedObjectKey>,
    ) -> Result<AutomationTriggerView> {
        let fields = Self::normalize(fields)?;
        self.ensure_name_available_in(transaction, project_id, &fields.name, None)
            .await?;
        let personality_id = self
            .personality_in(transaction, project_id, &fields)
            .await?;
        let tool_name = match fields.tool_name {
            Some(tool) => tool,
            None => {
                self.projects
                    .settings_in(transaction, project_name)
                    .await?
                    .default_agent_tool
            }
        };
        let next_evaluation_at = if fields.activation == AutomationActivation::Cron {
            Some(next_evaluation_at(&fields.schedule)?)
        } else {
            None
        };
        let last_event_id = if fields.activation == AutomationActivation::WorkItemCreated {
            self.repository
                .latest_item_created_event_in(transaction, project_id)
                .await?
        } else {
            None
        };
        let now = utc_now();
        let record = AutomationTriggerView {
            id: 0,
            project_id,
            name: fields.name,
            enabled: fields.enabled,
            activation: fields.activation,
            effect: fields.effect,
            schedule: fields.schedule,
            tool_name,
            mutability: fields.mutability,
            personality_id,
            personality_name: None,
            prompt: fields.prompt,
            work_item_selector: fields.selector,
            priority: fields.priority,
            exclusive: fields.exclusive,
            produced_work: fields.produced_work,
            execution: fields.execution,
            postconditions: fields.postconditions,
            current_revision_id: None,
            managed_bundle_key: ownership.map(|key| key.bundle_key().to_owned()),
            managed_object_key: ownership.map(|key| key.object_key().to_owned()),
            evaluation_count: 0,
            pending_evaluation_count: 0,
            last_evaluation_queued_at: None,
            last_evaluated_at: None,
            next_evaluation_at,
            last_event_id,
            created_at: now.clone(),
            updated_at: now,
        };
        let record = self.repository.insert_in(transaction, record).await?;
        self.repository
            .record_revision_in(
                transaction,
                record,
                if ownership.is_some() {
                    RevisionChangeOperation::BundleApply
                } else {
                    RevisionChangeOperation::Create
                },
            )
            .await
    }
    pub(crate) async fn apply_managed_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        ownership: &ManagedObjectKey,
        id: Option<i64>,
        fields: RuleFields,
    ) -> Result<AutomationTriggerView> {
        let project = self.projects.name_in(transaction, project_id).await?;
        if let Some(id) = id {
            let record = self.repository.get_in(transaction, project_id, id).await?;
            if !ownership.matches(
                record.managed_bundle_key.as_deref(),
                record.managed_object_key.as_deref(),
            ) {
                bail!("automation {id} does not belong to the requested bundle object");
            }
            let record = self
                .apply_fields_in(transaction, record, fields, true)
                .await?;
            let record = self.repository.save_in(transaction, record).await?;
            self.repository
                .record_revision_in(transaction, record, RevisionChangeOperation::BundleApply)
                .await
        } else {
            self.insert_configuration_in(transaction, project_id, &project, fields, Some(ownership))
                .await
        }
    }
    pub(crate) async fn delete_managed_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
        bundle_key: &str,
    ) -> Result<()> {
        let record = self.repository.get_in(transaction, project_id, id).await?;
        if record.managed_bundle_key.as_deref() != Some(bundle_key) {
            bail!("automation {id} does not belong to bundle '{bundle_key}'");
        }
        self.repository
            .delete_in(transaction, project_id, id)
            .await?;
        Ok(())
    }

    pub(crate) async fn update_from_input(
        &self,
        project: &str,
        id: i64,
        input: AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.update_configuration(
            ProjectReference::Name(project),
            id,
            Self::operator_fields(input),
        )
        .await
    }
    pub(crate) async fn update_configuration(
        &self,
        project: ProjectReference<'_>,
        id: i64,
        fields: RuleFields,
    ) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        Self::ensure_unmanaged(&record, "individual editing")?;
        let record = self
            .apply_fields_in(&transaction, record, fields, true)
            .await?;
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            &project_name,
            record,
            RevisionChangeOperation::Update,
        )
        .await
    }
    pub(crate) async fn update(
        &self,
        project: &str,
        id: i64,
        input: UpdateAutomationTrigger,
    ) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        Self::ensure_unmanaged(&record, "individual editing")?;
        let mut fields = RuleFields::from(&record);
        fields.name = input.name;
        fields.enabled = input.enabled;
        fields.activation = input.activation;
        fields.effect = input.effect;
        fields.schedule = input.schedule;
        fields.mutability = input.mutability;
        fields.personality = input.personality_id.map(PersonalityReference::Id);
        fields.prompt = input.prompt;
        fields.selector = input.work_item_selector;
        if let Some(priority) = input.priority {
            fields.priority = priority;
        }
        let record = self
            .apply_fields_in(&transaction, record, fields, true)
            .await?;
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            project,
            record,
            RevisionChangeOperation::Update,
        )
        .await
    }
    async fn apply_fields_in(
        &self,
        transaction: &Transaction,
        mut record: AutomationTriggerView,
        fields: RuleFields,
        reschedule: bool,
    ) -> Result<AutomationTriggerView> {
        let fields = Self::normalize(fields)?;
        self.ensure_name_available_in(
            transaction,
            record.project_id,
            &fields.name,
            Some(record.id),
        )
        .await?;
        let personality_id = self
            .personality_in(transaction, record.project_id, &fields)
            .await?;
        if reschedule {
            record.next_evaluation_at = if fields.activation == AutomationActivation::Cron {
                Some(next_evaluation_at(&fields.schedule)?)
            } else {
                None
            };
            record.last_event_id = match (record.activation, fields.activation) {
                (AutomationActivation::WorkItemCreated, AutomationActivation::WorkItemCreated) => {
                    record.last_event_id
                }
                (_, AutomationActivation::WorkItemCreated) => {
                    self.repository
                        .latest_item_created_event_in(transaction, record.project_id)
                        .await?
                }
                _ => None,
            };
        }
        record.name = fields.name;
        record.enabled = fields.enabled;
        record.activation = fields.activation;
        record.effect = fields.effect;
        record.schedule = fields.schedule;
        if let Some(tool) = fields.tool_name {
            record.tool_name = tool;
        }
        record.mutability = fields.mutability;
        record.personality_id = personality_id;
        record.prompt = fields.prompt;
        record.work_item_selector = fields.selector;
        record.priority = fields.priority;
        record.exclusive = fields.exclusive;
        record.produced_work = fields.produced_work;
        record.execution = fields.execution;
        record.postconditions = fields.postconditions;
        record.updated_at = utc_now();
        Ok(record)
    }
    pub(crate) async fn delete(&self, project: ProjectReference<'_>, id: i64) -> Result<u64> {
        let transaction = self.transactions.begin().await?;
        let (project_id, project_name) = self.scope_in(&transaction, project).await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        if record.managed_bundle_key.is_some() {
            bail!("bundle-managed automations cannot be deleted individually");
        }
        let count = self
            .repository
            .delete_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(&project_name);
        Ok(count)
    }
    pub(crate) async fn detach(&self, project: &str, id: i64) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        if record.managed_bundle_key.is_none() {
            bail!("automation trigger is not bundle-managed");
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
    pub(crate) async fn schedule(&self, project: &str, id: i64) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let mut record = self.repository.get_in(&transaction, project_id, id).await?;
        record.pending_evaluation_count = record.pending_evaluation_count.saturating_add(1);
        record.last_evaluation_queued_at = Some(utc_now());
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(project);
        Ok(record)
    }
    pub(crate) async fn revisions(
        &self,
        project: &str,
        id: i64,
    ) -> Result<Vec<AutomationRevisionView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .revisions_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn restore(
        &self,
        project: &str,
        id: i64,
        revision_id: i64,
    ) -> Result<AutomationTriggerView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let record = self.repository.get_in(&transaction, project_id, id).await?;
        Self::ensure_unmanaged(&record, "restoring a revision")?;
        let fields = self
            .repository
            .revision_fields_in(&transaction, project_id, id, revision_id)
            .await?;
        let record = self
            .apply_fields_in(&transaction, record, fields, false)
            .await?;
        let record = self.repository.save_in(&transaction, record).await?;
        self.commit_revision(
            transaction,
            project,
            record,
            RevisionChangeOperation::Restore,
        )
        .await
    }
    async fn commit_revision(
        &self,
        transaction: Transaction,
        project: &str,
        record: AutomationTriggerView,
        operation: RevisionChangeOperation,
    ) -> Result<AutomationTriggerView> {
        let record = self
            .repository
            .record_revision_in(&transaction, record, operation)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(project);
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    fn input(name: &str, prompt: &str) -> AutomationRuleInput {
        serde_json::from_value(serde_json::json!({"name":name,"enabled":true,"activation":"work_item","effect":"consume_work","schedule":"15s","prompt_markdown":prompt})).unwrap()
    }
    #[tokio::test]
    async fn configuration_revision_failures_roll_back_every_mutation() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let service = &app.state.rules;
        let original = service
            .create_from_input("demo", input("Atomic", "first"))
            .await
            .unwrap();
        let first_revision = original.current_revision_id.unwrap();
        let updated = service
            .update_from_input("demo", original.id, input("Atomic", "second"))
            .await
            .unwrap();
        let history =
            serde_json::to_value(service.revisions("demo", original.id).await.unwrap()).unwrap();
        let count = service.list("demo").await.unwrap().len();
        let db = app.state.store.db();
        db.execute(Statement::from_string(DbBackend::Sqlite, "CREATE TRIGGER fail_rule_history BEFORE INSERT ON automation_trigger_revisions BEGIN SELECT RAISE(FAIL, 'history unavailable'); END".to_owned())).await.unwrap();
        let mut events = app.state.events.subscribe();
        assert_that!(
            &service
                .create_from_input("demo", input("New", "first"))
                .await
                .is_err()
        )
        .is_true();
        assert_that!(&service.list("demo").await.unwrap().len()).is_equal_to(count);
        assert_that!(
            &service
                .update_from_input("demo", original.id, input("Changed", "third"))
                .await
                .is_err()
        )
        .is_true();
        assert_that!(
            &service
                .restore("demo", original.id, first_revision)
                .await
                .is_err()
        )
        .is_true();
        assert_that!(&serde_json::to_value(service.get("demo", "Atomic").await.unwrap()).unwrap())
            .is_equal_to(serde_json::to_value(updated).unwrap());
        assert_that!(
            &serde_json::to_value(service.revisions("demo", original.id).await.unwrap()).unwrap()
        )
        .is_equal_to(history);
        db.execute(Statement::from_string(DbBackend::Sqlite, format!("UPDATE automation_triggers SET managed_bundle_key = 'bundle', managed_object_key = 'atomic' WHERE id = {}", original.id))).await.unwrap();
        assert_that!(&service.detach("demo", original.id).await.is_err()).is_true();
        assert_that!(
            &service
                .get("demo", "Atomic")
                .await
                .unwrap()
                .managed_bundle_key
                .as_deref()
        )
        .is_equal_to(Some("bundle"));
        assert_that!(&events.try_recv().is_err()).is_true();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_rule_history".to_owned(),
        ))
        .await
        .unwrap();
        service.detach("demo", original.id).await.unwrap();
        events.try_recv().unwrap();
        let restored = service
            .restore("demo", original.id, first_revision)
            .await
            .unwrap();
        assert_that!(&restored.prompt).is_equal_to(original.prompt);
        assert_that!(&restored.current_revision_id).is_not_equal_to(Some(first_revision));
        events.try_recv().unwrap();
        let inspector = app
            .state
            .revision_queries
            .inspect_rule("demo", original.id)
            .await
            .unwrap();
        assert_that!(&inspector.revisions.len()).is_equal_to(4);
        assert_that!(&inspector.current_revision_analytics.unwrap().revision_id)
            .is_equal_to(restored.current_revision_id.unwrap());
        assert_that!(&events.try_recv().is_err()).is_true();
    }

    #[tokio::test]
    async fn queue_and_delete_failures_preserve_state_and_queue_updates_are_not_lost() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let service = &app.state.rules;
        let record = service
            .create_from_input("demo", input("Queued", ""))
            .await
            .unwrap();
        let db = app.state.store.db();
        db.execute(Statement::from_string(DbBackend::Sqlite, "CREATE TRIGGER fail_rule_queue BEFORE UPDATE ON automation_triggers WHEN NEW.pending_evaluation_count > OLD.pending_evaluation_count BEGIN SELECT RAISE(FAIL, 'queue unavailable'); END".to_owned())).await.unwrap();
        db.execute(Statement::from_string(DbBackend::Sqlite, "CREATE TRIGGER fail_rule_delete BEFORE DELETE ON automation_triggers BEGIN SELECT RAISE(FAIL, 'delete unavailable'); END".to_owned())).await.unwrap();
        let mut events = app.state.events.subscribe();
        assert_that!(&service.schedule("demo", record.id).await.is_err()).is_true();
        assert_that!(
            &service
                .delete(ProjectReference::Name("demo"), record.id)
                .await
                .is_err()
        )
        .is_true();
        assert_that!(
            &service
                .get("demo", "Queued")
                .await
                .unwrap()
                .pending_evaluation_count
        )
        .is_equal_to(0);
        assert_that!(&events.try_recv().is_err()).is_true();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_rule_queue".to_owned(),
        ))
        .await
        .unwrap();
        let second =
            super::super::tests::service_with_events(&app.state.store, app.state.events.clone());
        let (first, second) = tokio::join!(
            service.schedule("demo", record.id),
            second.schedule("demo", record.id)
        );
        first.unwrap();
        second.unwrap();
        assert_that!(
            &service
                .get("demo", "Queued")
                .await
                .unwrap()
                .pending_evaluation_count
        )
        .is_equal_to(2);
        assert_that!(&service.revisions("demo", record.id).await.unwrap().len()).is_equal_to(1);
        events.try_recv().unwrap();
        events.try_recv().unwrap();
        assert_that!(&events.try_recv().is_err()).is_true();
    }

    #[tokio::test]
    async fn crudkit_json_and_forms_use_the_same_rule_history_and_preserve_tool_policy() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let project_id = app.state.projects.id("demo").await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let router = super::super::transport::api::routes()
            .merge(super::super::transport::forms::routes())
            .merge(super::super::transport::crud::routes())
            .layer(axum::Extension(app.state.clone()))
            .layer(axum::Extension(app.contexts.automation_trigger.clone()));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let mut events = app.state.events.subscribe();
        let response = client.post(format!("{url}/api/automation_triggers/crud/create-one")).json(&serde_json::json!({"entity":{"project_id":project_id,"name":"Rule","enabled":true,"activation":"work_item","effect":"consume_work","schedule":" 15s ","tool_name":"ignored-create-tool","prompt":"first","priority":0}})).send().await.unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
        let body = response.json::<serde_json::Value>().await.unwrap();
        let id = body["entity"]["id"].as_i64().unwrap();
        let revision = body["entity"]["current_revision_id"].as_i64().unwrap();
        let created = app.state.rules.get("demo", "Rule").await.unwrap();
        assert_that!(&created.tool_name).is_equal_to(dispatch_types::AgentToolName::Codex);
        assert_that!(&created.schedule).is_equal_to("15s");
        assert_that!(&created.work_item_selector.is_some()).is_true();
        assert_that!(&created.personality_id.is_some()).is_true();
        events.try_recv().unwrap();
        let response = client
            .put(format!(
                "{url}/operator/api/projects/demo/automation/rules/{id}"
            ))
            .json(&input("Rule", "second"))
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
        events.try_recv().unwrap();
        let response = client
            .post(format!(
                "{url}/projects/demo/automation/triggers/{id}/update"
            ))
            .form(&[
                ("name", "Rule"),
                ("enabled", "on"),
                ("schedule", "15s"),
                ("prompt", "third"),
            ])
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::SEE_OTHER);
        events.try_recv().unwrap();
        assert_that!(&app.state.rules.get("demo", "Rule").await.unwrap().prompt)
            .is_equal_to("third");
        let condition =
            serde_json::json!({"All":[{"column_name":"id","operator":"=","value":{"I64":id}}]});
        let response = client.post(format!("{url}/api/automation_triggers/crud/update-one")).json(&serde_json::json!({"condition":condition,"entity":{"name":"Rule","enabled":true,"activation":"work_item","effect":"consume_work","schedule":"15s","tool_name":"ignored-update-tool","prompt":"fourth","priority":0}})).send().await.unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
        events.try_recv().unwrap();
        assert_that!(&app.state.rules.get("demo", "Rule").await.unwrap().tool_name)
            .is_equal_to(dispatch_types::AgentToolName::Codex);
        assert_that!(&app.state.rules.revisions("demo", id).await.unwrap().len()).is_equal_to(4);
        let response = client
            .post(format!(
                "{url}/operator/api/projects/demo/automation/rules/{id}/restore"
            ))
            .json(&serde_json::json!({"revision_id":revision}))
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
        events.try_recv().unwrap();
        assert_that!(&app.state.rules.get("demo", "Rule").await.unwrap().prompt)
            .is_equal_to("first");
        let mut invalid = input("Rule", "bad");
        invalid.execution.model = Some("unknown".into());
        let response = client
            .put(format!(
                "{url}/operator/api/projects/demo/automation/rules/{id}"
            ))
            .json(&invalid)
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::BAD_REQUEST);
        assert_that!(&events.try_recv().is_err()).is_true();
        let response = client
            .post(format!("{url}/api/automation_triggers/crud/delete-one"))
            .json(&serde_json::json!({"condition":condition}))
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
        events.try_recv().unwrap();
        assert_that!(&app.state.rules.get("demo", "Rule").await.is_err()).is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        server.abort();
    }
}
