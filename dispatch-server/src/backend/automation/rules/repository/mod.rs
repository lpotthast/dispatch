use super::model::RuleFields;
use crate::backend::{
    automation::revisions::model::RevisionActor,
    entities::{
        automation_trigger::{self, AutomationTrigger},
        work_item_event,
    },
    storage::Transaction,
};
use dispatch_types::{AutomationRevisionView, AutomationTriggerView, RevisionChangeOperation};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect,
};
pub(crate) mod defaults;
pub(crate) mod encoding;
pub(crate) mod revisions;
use encoding::{encode, model_to_view};
pub(crate) struct RuleRepository;
impl RuleRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<AutomationTriggerView>> {
        AutomationTrigger::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .order_by_asc(automation_trigger::Column::Name)
            .all(transaction.connection())
            .await
            .context("failed to list automation triggers")?
            .into_iter()
            .map(model_to_view)
            .collect()
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<AutomationTriggerView> {
        model_to_view(
            AutomationTrigger::find_by_id(id)
                .filter(automation_trigger::Column::ProjectId.eq(project_id))
                .one(transaction.connection())
                .await
                .context("failed to load automation trigger")?
                .ok_or_else(|| report!("trigger {id} does not exist in this project"))?,
        )
    }
    pub(crate) async fn find_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        reference: &str,
    ) -> Result<AutomationTriggerView> {
        let query =
            AutomationTrigger::find().filter(automation_trigger::Column::ProjectId.eq(project_id));
        let query = match reference.parse::<i64>() {
            Ok(id) => query.filter(automation_trigger::Column::Id.eq(id)),
            Err(_) => query.filter(
                sea_orm::Condition::any()
                    .add(automation_trigger::Column::Name.eq(reference))
                    .add(automation_trigger::Column::ManagedObjectKey.eq(reference)),
            ),
        };
        model_to_view(
            query
                .one(transaction.connection())
                .await
                .context("failed to load automation trigger")?
                .ok_or_else(|| {
                    report!("automation trigger '{reference}' does not exist in this project")
                })?,
        )
    }
    pub(crate) async fn name_exists_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        name: &str,
        except: Option<i64>,
    ) -> Result<bool> {
        let mut query = AutomationTrigger::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .filter(automation_trigger::Column::Name.eq(name));
        if let Some(id) = except {
            query = query.filter(automation_trigger::Column::Id.ne(id));
        }
        Ok(query
            .one(transaction.connection())
            .await
            .context("failed to check automation trigger name")?
            .is_some())
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        record: AutomationTriggerView,
    ) -> Result<AutomationTriggerView> {
        let mut active = encode(record)?.into_active_model().reset_all();
        active.id = Default::default();
        model_to_view(
            active
                .insert(transaction.connection())
                .await
                .context("failed to create automation trigger")?,
        )
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: AutomationTriggerView,
    ) -> Result<AutomationTriggerView> {
        model_to_view(
            encode(record)?
                .into_active_model()
                .reset_all()
                .update(transaction.connection())
                .await
                .context("failed to update automation trigger")?,
        )
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<u64> {
        Ok(AutomationTrigger::delete_by_id(id)
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .exec(transaction.connection())
            .await
            .context("failed to delete automation trigger")?
            .rows_affected)
    }
    pub(crate) async fn latest_item_created_event_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Option<i64>> {
        Ok(work_item_event::Entity::find()
            .filter(work_item_event::Column::ProjectId.eq(project_id))
            .filter(work_item_event::Column::EventType.eq("item_created"))
            .order_by_desc(work_item_event::Column::Id)
            .limit(1)
            .one(transaction.connection())
            .await
            .context("failed to load latest item-created event")?
            .map(|event| event.id))
    }
    pub(crate) async fn record_revision_in(
        &self,
        transaction: &Transaction,
        mut record: AutomationTriggerView,
        operation: RevisionChangeOperation,
    ) -> Result<AutomationTriggerView> {
        record.current_revision_id = Some(
            revisions::record_in_conn(
                transaction.connection(),
                &encode(record.clone())?,
                operation,
                &RevisionActor::default(),
            )
            .await?,
        );
        Ok(record)
    }
    pub(crate) async fn revisions_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<Vec<AutomationRevisionView>> {
        revisions::list_in(transaction, project_id, id).await
    }
    pub(crate) async fn revision_fields_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
        revision_id: i64,
    ) -> Result<RuleFields> {
        revisions::fields_in(transaction, project_id, id, revision_id).await
    }
}
