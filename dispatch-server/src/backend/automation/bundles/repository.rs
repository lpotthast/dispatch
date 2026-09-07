use super::model::{BundleRecord, BundleStatus};
use crate::backend::{
    automation::{
        personalities::repository::PersonalityRepository, rules::repository::RuleRepository,
    },
    entities::automation_bundle_apply::{
        self, AutomationBundleApply, AutomationBundleApplyActiveModel,
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::{AutomationBundleDiffView, AutomationTriggerView, PersonalityView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) struct BundleRepository {
    rules: Arc<RuleRepository>,
    personalities: Arc<PersonalityRepository>,
}
impl BundleRepository {
    pub(crate) fn new(
        rules: Arc<RuleRepository>,
        personalities: Arc<PersonalityRepository>,
    ) -> Self {
        Self {
            rules,
            personalities,
        }
    }
    pub(crate) async fn rules_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<AutomationTriggerView>> {
        self.rules.list_in(transaction, project_id).await
    }
    pub(crate) async fn personalities_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<PersonalityView>> {
        self.personalities.list_in(transaction, project_id).await
    }
    pub(crate) async fn latest_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        bundle_key: &str,
    ) -> Result<Option<BundleRecord>> {
        AutomationBundleApply::find()
            .filter(automation_bundle_apply::Column::ProjectId.eq(project_id))
            .filter(automation_bundle_apply::Column::BundleKey.eq(bundle_key))
            .order_by_desc(automation_bundle_apply::Column::Id)
            .one(transaction.connection())
            .await
            .context("failed to load latest bundle apply")?
            .map(decode)
            .transpose()
    }
    pub(crate) async fn latest_all_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<BundleRecord>> {
        let applies = AutomationBundleApply::find()
            .filter(automation_bundle_apply::Column::ProjectId.eq(project_id))
            .order_by_asc(automation_bundle_apply::Column::Id)
            .all(transaction.connection())
            .await
            .context("failed to load bundle apply history")?;
        applies
            .into_iter()
            .fold(BTreeMap::new(), |mut latest, apply| {
                latest.insert(apply.bundle_key.clone(), apply);
                latest
            })
            .into_values()
            .map(decode)
            .collect()
    }
    pub(crate) async fn record_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        diff: &AutomationBundleDiffView,
        status: BundleStatus,
    ) -> Result<BundleRecord> {
        decode(
            AutomationBundleApplyActiveModel {
                project_id: Set(project_id),
                bundle_key: Set(diff.bundle_key.clone()),
                display_name: Set(diff.display_name.clone()),
                manifest_hash: Set(diff.manifest_hash.clone()),
                applied_diff_json: Set(
                    serde_json::to_string(diff).context("failed to encode bundle diff")?
                ),
                actor_type: Set(None),
                actor_id: Set(None),
                status: Set(status.as_storage().to_owned()),
                created_at: Set(utc_now()),
                ..Default::default()
            }
            .insert(transaction.connection())
            .await
            .context("failed to record bundle operation")?,
        )
    }
}
fn decode(record: automation_bundle_apply::Model) -> Result<BundleRecord> {
    Ok(BundleRecord {
        id: record.id,
        bundle_key: record.bundle_key,
        display_name: record.display_name,
        manifest_hash: record.manifest_hash,
        status: BundleStatus::parse(&record.status)?,
        created_at: record.created_at,
    })
}
