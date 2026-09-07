use super::{
    model::{BundleRecord, BundleStatus},
    policy,
    repository::BundleRepository,
};
use crate::backend::{
    automation::{
        ownership::{ManagedDeletion, ManagedObjectKey},
        personalities::service::PersonalityService,
        rules::{
            model::{PersonalityReference, RuleFields},
            service::RuleService,
        },
    },
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{
    AutomationBundleApplyView, AutomationBundleDiffView, AutomationPersonalityInput,
    AutomationRuleInput, AutomationTriggerView, BundleDiffOperation, BundleObjectDiffView,
    InstalledAutomationBundleView, PersonalityView,
};
use rootcause::{Result, prelude::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) struct BundleService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<BundleRepository>,
    rules: Arc<RuleService>,
    personalities: Arc<PersonalityService>,
    events: UiEventBus,
}
impl BundleService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<BundleRepository>,
        rules: Arc<RuleService>,
        personalities: Arc<PersonalityService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            rules,
            personalities,
            events,
        }
    }

    async fn snapshot_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        bundle_key: Option<&str>,
    ) -> Result<(Vec<PersonalityView>, Vec<AutomationTriggerView>)> {
        let mut personalities = self
            .repository
            .personalities_in(transaction, project_id)
            .await?;
        let mut rules = self.repository.rules_in(transaction, project_id).await?;
        if let Some(key) = bundle_key {
            personalities.retain(|record| record.managed_bundle_key.as_deref() == Some(key));
            rules.retain(|record| record.managed_bundle_key.as_deref() == Some(key));
        }
        Ok((personalities, rules))
    }
    pub(crate) async fn diff(&self, project: &str, yaml: &str) -> Result<AutomationBundleDiffView> {
        let bundle = policy::validate_yaml(yaml)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let current = self
            .repository
            .latest_in(&transaction, project_id, &bundle.manifest.bundle_key)
            .await?
            .filter(|record| record.status == BundleStatus::Applied);
        let (personalities, rules) = self
            .snapshot_in(&transaction, project_id, Some(&bundle.manifest.bundle_key))
            .await?;
        let diff = policy::diff(
            &bundle,
            current.map(|record| record.manifest_hash),
            &personalities,
            &rules,
        )?;
        transaction.commit().await?;
        Ok(diff)
    }
    pub(crate) async fn apply(
        &self,
        project: &str,
        yaml: &str,
        expected_current_hash: Option<&str>,
    ) -> Result<AutomationBundleApplyView> {
        self.apply_matching(
            project,
            yaml,
            ApplyCondition::ExpectedHash(expected_current_hash),
        )
        .await
    }
    pub(crate) async fn apply_current(
        &self,
        project: &str,
        yaml: &str,
        allow_deletions: bool,
    ) -> Result<AutomationBundleApplyView> {
        self.apply_matching(project, yaml, ApplyCondition::Current { allow_deletions })
            .await
    }
    async fn apply_matching(
        &self,
        project: &str,
        yaml: &str,
        condition: ApplyCondition<'_>,
    ) -> Result<AutomationBundleApplyView> {
        let bundle = policy::validate_yaml(yaml)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let current_hash = self
            .repository
            .latest_in(&transaction, project_id, &bundle.manifest.bundle_key)
            .await?
            .filter(|record| record.status == BundleStatus::Applied)
            .map(|record| record.manifest_hash);
        if let ApplyCondition::ExpectedHash(expected) = condition
            && expected != current_hash.as_deref()
        {
            bail!(
                "bundle hash changed; expected {:?}, found {:?}",
                expected,
                current_hash
            );
        }
        let (personalities, rules) = self.snapshot_in(&transaction, project_id, None).await?;
        policy::reject_unmanaged_name_conflicts(&bundle, &personalities, &rules)?;
        let managed_personalities = personalities
            .iter()
            .filter(|record| {
                record.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let managed_rules = rules
            .iter()
            .filter(|record| {
                record.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let diff = policy::diff(
            &bundle,
            current_hash,
            &managed_personalities,
            &managed_rules,
        )?;
        if diff.has_deletions
            && matches!(
                condition,
                ApplyCondition::Current {
                    allow_deletions: false
                }
            )
        {
            bail!("bundle diff deletes managed objects; confirm deletions before applying");
        }
        let unchanged_personalities = diff
            .objects
            .iter()
            .filter(|object| {
                object.object_type == "personality"
                    && object.operation == BundleDiffOperation::Unchanged
            })
            .map(|object| object.key.as_str())
            .collect::<BTreeSet<_>>();
        let unchanged_rules = diff
            .objects
            .iter()
            .filter(|object| {
                object.object_type == "automation"
                    && object.operation == BundleDiffOperation::Unchanged
            })
            .map(|object| object.key.as_str())
            .collect::<BTreeSet<_>>();
        let wanted_rules = bundle
            .manifest
            .automations
            .iter()
            .filter_map(|input| input.key.as_deref())
            .collect::<BTreeSet<_>>();
        for rule in managed_rules.iter().filter(|record| {
            !wanted_rules.contains(record.managed_object_key.as_deref().unwrap_or_default())
        }) {
            self.rules
                .delete_managed_in(
                    &transaction,
                    project_id,
                    rule.id,
                    &bundle.manifest.bundle_key,
                )
                .await?;
        }
        let mut personality_ids = BTreeMap::new();
        for input in &bundle.manifest.personalities {
            let ownership = ManagedObjectKey::new(&bundle.manifest.bundle_key, &input.key)?;
            let existing = managed_personalities
                .iter()
                .find(|record| record.managed_object_key.as_deref() == Some(input.key.as_str()));
            let id = match existing {
                Some(record) if unchanged_personalities.contains(input.key.as_str()) => record.id,
                _ => {
                    self.personalities
                        .apply_managed_in(
                            &transaction,
                            project_id,
                            &ownership,
                            existing.map(|record| record.id),
                            AutomationPersonalityInput {
                                key: input.key.clone(),
                                name: input.name.clone(),
                                description:
                                    crate::backend::execution::prompt_text::markdown_to_html(
                                        &input.description,
                                    ),
                            },
                        )
                        .await?
                        .id
                }
            };
            personality_ids.insert(input.key.as_str(), id);
        }
        for input in &bundle.manifest.automations {
            let key = input.key.as_deref().expect("validated automation key");
            if unchanged_rules.contains(key) {
                continue;
            }
            let existing = managed_rules
                .iter()
                .find(|record| record.managed_object_key.as_deref() == Some(key));
            let personality_id = input
                .personality
                .as_ref()
                .map(|key| {
                    personality_ids
                        .get(key.as_str())
                        .copied()
                        .ok_or_else(|| report!("missing applied personality '{key}'"))
                })
                .transpose()?;
            let ownership = ManagedObjectKey::new(&bundle.manifest.bundle_key, key)?;
            self.rules
                .apply_managed_in(
                    &transaction,
                    project_id,
                    &ownership,
                    existing.map(|record| record.id),
                    managed_rule_fields(input, personality_id),
                )
                .await?;
        }
        let wanted_personalities = bundle
            .manifest
            .personalities
            .iter()
            .map(|input| input.key.as_str())
            .collect::<BTreeSet<_>>();
        for personality in managed_personalities.iter().filter(|record| {
            !wanted_personalities.contains(record.managed_object_key.as_deref().unwrap_or_default())
        }) {
            self.personalities
                .delete_managed_in(
                    &transaction,
                    project_id,
                    personality.id,
                    &bundle.manifest.bundle_key,
                    ManagedDeletion::Reconcile,
                )
                .await?;
        }
        let record = self
            .repository
            .record_in(&transaction, project_id, &diff, BundleStatus::Applied)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(project);
        Ok(apply_view(record, diff))
    }
    pub(crate) async fn list_installed(
        &self,
        project: &str,
    ) -> Result<Vec<InstalledAutomationBundleView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let latest = self
            .repository
            .latest_all_in(&transaction, project_id)
            .await?;
        let (personalities, rules) = self.snapshot_in(&transaction, project_id, None).await?;
        let mut installed = latest
            .into_iter()
            .filter(|record| record.status == BundleStatus::Applied)
            .map(|record| InstalledAutomationBundleView {
                apply_id: record.id,
                automation_count: rules
                    .iter()
                    .filter(|rule| rule.managed_bundle_key.as_deref() == Some(&record.bundle_key))
                    .count() as u64,
                personality_count: personalities
                    .iter()
                    .filter(|personality| {
                        personality.managed_bundle_key.as_deref() == Some(&record.bundle_key)
                    })
                    .count() as u64,
                bundle_key: record.bundle_key,
                display_name: record.display_name,
                manifest_hash: record.manifest_hash,
                installed_at: record.created_at,
            })
            .collect::<Vec<_>>();
        installed.sort_by(|a, b| {
            a.display_name
                .cmp(&b.display_name)
                .then_with(|| a.bundle_key.cmp(&b.bundle_key))
        });
        transaction.commit().await?;
        Ok(installed)
    }
    pub(crate) async fn remove(
        &self,
        project: &str,
        bundle_key: &str,
        expected_current_hash: Option<&str>,
    ) -> Result<AutomationBundleApplyView> {
        crate::backend::automation::rules::policy::validate_stable_key("bundle key", bundle_key)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let current = self
            .repository
            .latest_in(&transaction, project_id, bundle_key)
            .await?
            .filter(|record| record.status == BundleStatus::Applied)
            .ok_or_else(|| report!("bundle '{bundle_key}' is not installed in this project"))?;
        if expected_current_hash != Some(current.manifest_hash.as_str()) {
            bail!(
                "bundle hash changed; expected {:?}, found {:?}",
                expected_current_hash,
                Some(current.manifest_hash.as_str())
            );
        }
        let (personalities, rules) = self
            .snapshot_in(&transaction, project_id, Some(bundle_key))
            .await?;
        let mut objects = rules
            .iter()
            .map(|record| BundleObjectDiffView {
                object_type: "automation".into(),
                key: record.managed_object_key.clone().unwrap_or_default(),
                name: record.name.clone(),
                operation: BundleDiffOperation::Delete,
                changes: Vec::new(),
            })
            .chain(personalities.iter().map(|record| BundleObjectDiffView {
                object_type: "personality".into(),
                key: record.managed_object_key.clone().unwrap_or_default(),
                name: record.name.clone(),
                operation: BundleDiffOperation::Delete,
                changes: Vec::new(),
            }))
            .collect::<Vec<_>>();
        objects.sort_by(|a, b| {
            a.object_type
                .cmp(&b.object_type)
                .then_with(|| a.key.cmp(&b.key))
        });
        let diff = AutomationBundleDiffView {
            bundle_key: bundle_key.into(),
            display_name: current.display_name,
            current_hash: Some(current.manifest_hash.clone()),
            manifest_hash: current.manifest_hash,
            has_deletions: !objects.is_empty(),
            objects,
        };
        for rule in &rules {
            self.rules
                .delete_managed_in(&transaction, project_id, rule.id, bundle_key)
                .await?;
        }
        for personality in &personalities {
            self.personalities
                .delete_managed_in(
                    &transaction,
                    project_id,
                    personality.id,
                    bundle_key,
                    ManagedDeletion::RemoveBundle,
                )
                .await?;
        }
        let record = self
            .repository
            .record_in(&transaction, project_id, &diff, BundleStatus::Removed)
            .await?;
        transaction.commit().await?;
        self.events.publish_automation_changed(project);
        Ok(apply_view(record, diff))
    }
    pub(crate) async fn export(&self, project: &str, bundle_key: &str) -> Result<String> {
        crate::backend::automation::rules::policy::validate_stable_key("bundle key", bundle_key)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let current = self
            .repository
            .latest_in(&transaction, project_id, bundle_key)
            .await?
            .filter(|record| record.status == BundleStatus::Applied)
            .ok_or_else(|| report!("bundle '{bundle_key}' has not been applied to this project"))?;
        let (mut personalities, mut rules) = self
            .snapshot_in(&transaction, project_id, Some(bundle_key))
            .await?;
        personalities.sort_by(|a, b| a.managed_object_key.cmp(&b.managed_object_key));
        rules.sort_by(|a, b| a.managed_object_key.cmp(&b.managed_object_key));
        let yaml = policy::export(bundle_key, current.display_name, &personalities, rules)?;
        transaction.commit().await?;
        Ok(yaml)
    }
}
fn apply_view(record: BundleRecord, diff: AutomationBundleDiffView) -> AutomationBundleApplyView {
    AutomationBundleApplyView {
        apply_id: record.id,
        diff,
        status: record.status.as_storage().to_owned(),
        applied_at: record.created_at,
    }
}
fn managed_rule_fields(input: &AutomationRuleInput, personality_id: Option<i64>) -> RuleFields {
    RuleFields {
        name: input.name.clone(),
        enabled: input.enabled,
        activation: input.activation,
        effect: input.effect,
        schedule: input.schedule.clone(),
        tool_name: Some(input.tool_name),
        mutability: input.mutability,
        personality: personality_id.map(PersonalityReference::Id),
        prompt: crate::backend::execution::prompt_text::markdown_to_html(&input.prompt_markdown),
        selector: input.selector.clone(),
        priority: input.priority,
        exclusive: input.exclusive,
        produced_work: input.produced_work.clone(),
        execution: input.execution.clone(),
        postconditions: input.postconditions.clone(),
    }
}

enum ApplyCondition<'a> {
    ExpectedHash(Option<&'a str>),
    Current { allow_deletions: bool },
}
