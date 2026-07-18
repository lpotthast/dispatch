use std::collections::{BTreeMap, BTreeSet};

use pulldown_cmark::{Options, Parser, html};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use sha2::{Digest, Sha256};

use crate::{
    backend::{
        automation_admission,
        automation_revisions::{self, RevisionActor},
        automation_triggers,
        entities::{
            automation_bundle_apply::{
                self, AutomationBundleApply, AutomationBundleApplyActiveModel,
            },
            automation_trigger::{self, AutomationTrigger, AutomationTriggerActiveModel},
            personality::{self, Personality, PersonalityActiveModel},
        },
        events, item_labels, personalities, projects, prompt_text,
        storage::{Store, utc_now},
        work_item_creation::{CreateWorkItem, CreateWorkItemPlan},
    },
    shared::view_models::{
        AutomationActivation, AutomationBundleApplyView, AutomationBundleDiffView,
        AutomationBundleManifest, AutomationEffect, AutomationPersonalityInput,
        AutomationRuleInput, BundleDiffOperation, BundleObjectDiffView,
        InstalledAutomationBundleView, RevisionChangeOperation,
    },
};

#[derive(Debug)]
pub(crate) struct ValidatedBundle {
    pub(crate) manifest: AutomationBundleManifest,
    pub(crate) manifest_hash: String,
}

pub(crate) fn validate_yaml(yaml: &str) -> Result<ValidatedBundle> {
    let mut manifest = yaml_serde::from_str::<AutomationBundleManifest>(yaml)
        .context("invalid automation bundle YAML")?;
    if manifest.schema_version != 1 {
        bail!(
            "unsupported automation bundle schema_version {}; expected 1",
            manifest.schema_version
        );
    }
    automation_triggers::validate_stable_key("bundle_key", &manifest.bundle_key)?;
    if manifest.display_name.trim().is_empty() {
        bail!("bundle display_name cannot be empty");
    }
    manifest.display_name = manifest.display_name.trim().to_owned();
    let mut personality_keys = BTreeSet::new();
    let mut personality_names = BTreeSet::new();
    for (index, personality) in manifest.personalities.iter_mut().enumerate() {
        automation_triggers::validate_stable_key(
            &format!("personalities[{index}].key"),
            &personality.key,
        )?;
        if !personality_keys.insert(personality.key.clone()) {
            bail!("duplicate personality key '{}'", personality.key);
        }
        personality.name = personalities::normalize_name(std::mem::take(&mut personality.name))?;
        if !personality_names.insert(personality.name.clone()) {
            bail!("duplicate personality name '{}'", personality.name);
        }
        personality.description = canonicalize_markdown(&personality.description)?;
    }
    let mut rule_keys = BTreeSet::new();
    let mut rule_names = BTreeSet::new();
    for (index, rule) in manifest.automations.iter_mut().enumerate() {
        let key = rule
            .key
            .as_deref()
            .ok_or_else(|| report!("automations[{index}].key is required"))?;
        automation_triggers::validate_stable_key(&format!("automations[{index}].key"), key)?;
        if !rule_keys.insert(key.to_owned()) {
            bail!("duplicate automation key '{key}'");
        }
        rule.name = rule.name.trim().to_owned();
        if rule.name.is_empty() {
            bail!("automations[{index}].name cannot be empty");
        }
        if !rule_names.insert(rule.name.clone()) {
            bail!("duplicate automation name '{}'", rule.name);
        }
        rule.prompt_markdown = canonicalize_markdown(&rule.prompt_markdown)?;
        automation_triggers::validate_trigger_configuration(
            &rule.name,
            rule.activation,
            rule.effect,
            &rule.schedule,
            rule.selector.as_ref(),
            &rule.prompt_markdown,
        )?;
        automation_admission::validate_execution_policy(&rule.execution)?;
        projects::validate_agent_model_reasoning_effort(
            "automation model override",
            rule.execution.model.as_deref(),
            "automation reasoning-effort override",
            rule.execution.reasoning_effort,
        )?;
        if rule.effect == AutomationEffect::ConsumeWork {
            let personality = rule.personality.as_deref().ok_or_else(|| {
                report!("automations[{index}].personality is required for consume_work")
            })?;
            if !personality_keys.contains(personality) {
                bail!("automations[{index}].personality references unknown key '{personality}'");
            }
            if rule.produced_work.is_some() {
                bail!("automations[{index}].produced_work is only valid for produce_work");
            }
        } else {
            if rule.personality.is_some()
                || rule.selector.is_some()
                || rule.postconditions.is_some()
            {
                bail!("automations[{index}] has consume-work fields on a produce_work automation");
            }
            if let Some(spec) = &rule.produced_work {
                CreateWorkItemPlan::new(CreateWorkItem {
                    title: spec.title.clone().unwrap_or_else(|| rule.name.clone()),
                    description: rule.prompt_markdown.clone(),
                    state: spec.state.clone(),
                    agent_model_override: spec.agent_model_override.clone(),
                    agent_reasoning_effort_override: spec.agent_reasoning_effort_override,
                    initial_labels: spec.initial_labels.clone(),
                })?;
            }
        }
        validate_postconditions(rule, index)?;
    }
    manifest
        .personalities
        .sort_by(|left, right| left.key.cmp(&right.key));
    manifest.automations.sort_by(|left, right| {
        left.key
            .as_deref()
            .unwrap_or_default()
            .cmp(right.key.as_deref().unwrap_or_default())
    });
    let canonical =
        serde_json::to_vec(&manifest).context("failed to canonicalize automation bundle")?;
    let manifest_hash = format!("{:x}", Sha256::digest(&canonical));
    Ok(ValidatedBundle {
        manifest,
        manifest_hash,
    })
}

fn validate_postconditions(rule: &AutomationRuleInput, index: usize) -> Result<()> {
    let Some(postconditions) = &rule.postconditions else {
        return Ok(());
    };
    validate_postcondition_configuration(
        postconditions,
        &format!("automations[{index}].postconditions"),
    )
}

pub(crate) fn validate_postcondition_configuration(
    postconditions: &crate::shared::view_models::AutomationPostconditions,
    path: &str,
) -> Result<()> {
    if postconditions.any_of.is_empty() {
        bail!("{path}.any_of cannot be empty");
    }
    for (outcome_index, outcome) in postconditions.any_of.iter().enumerate() {
        for label in &outcome.labels {
            item_labels::normalize_key(label.key.clone())
                .context_with(|| format!("{path}.any_of[{outcome_index}] has invalid label"))?;
        }
        if let Some(created) = &outcome.created_items {
            validate_created_item_assertion(
                created,
                &format!("{path}.any_of[{outcome_index}].created_items"),
            )?;
        }
        for (assertion_index, created) in outcome.created_item_assertions.iter().enumerate() {
            validate_created_item_assertion(
                created,
                &format!(
                    "{path}.any_of[{outcome_index}].created_item_assertions[{assertion_index}]"
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_created_item_assertion(
    created: &crate::shared::view_models::CreatedItemAssertion,
    path: &str,
) -> Result<()> {
    if created.count.is_some() && (created.at_least.is_some() || created.at_most.is_some()) {
        bail!("{path}.count cannot be combined with at_least or at_most");
    }
    if let (Some(minimum), Some(maximum)) = (created.at_least, created.at_most)
        && minimum > maximum
    {
        bail!("{path} has at_least greater than at_most");
    }
    if let Some(selector) = &created.selector {
        crate::backend::label_conditions::validate_condition(selector)?;
    }
    Ok(())
}

pub(crate) async fn diff_yaml(
    store: &Store,
    project_name: &str,
    yaml: &str,
) -> Result<AutomationBundleDiffView> {
    let bundle = validate_yaml(yaml)?;
    diff_validated(store, project_name, &bundle).await
}

async fn diff_validated(
    store: &Store,
    project_name: &str,
    bundle: &ValidatedBundle,
) -> Result<AutomationBundleDiffView> {
    let project_id = projects::project_id(store, project_name).await?;
    let current_hash =
        latest_bundle_hash(store.db().as_ref(), project_id, &bundle.manifest.bundle_key).await?;
    let personalities = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .filter(personality::Column::ManagedBundleKey.eq(&bundle.manifest.bundle_key))
        .all(store.db().as_ref())
        .await
        .context("failed to load managed personalities")?;
    let rules = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::ManagedBundleKey.eq(&bundle.manifest.bundle_key))
        .all(store.db().as_ref())
        .await
        .context("failed to load managed automations")?;
    let personality_by_key = personalities
        .iter()
        .filter_map(|model| {
            model
                .managed_object_key
                .as_ref()
                .map(|key| (key.as_str(), model))
        })
        .collect::<BTreeMap<_, _>>();
    let rule_by_key = rules
        .iter()
        .filter_map(|model| {
            model
                .managed_object_key
                .as_ref()
                .map(|key| (key.as_str(), model))
        })
        .collect::<BTreeMap<_, _>>();
    let mut objects = Vec::new();
    for input in &bundle.manifest.personalities {
        let operation = match personality_by_key.get(input.key.as_str()) {
            None => BundleDiffOperation::Create,
            Some(model)
                if model.name == input.name
                    && normalize_markdown(&prompt_text::rich_text_to_prompt_markdown(
                        &model.personality_description,
                    )?) == input.description =>
            {
                BundleDiffOperation::Unchanged
            }
            Some(_) => BundleDiffOperation::Update,
        };
        objects.push(BundleObjectDiffView {
            object_type: "personality".to_owned(),
            key: input.key.clone(),
            name: input.name.clone(),
            operation,
            changes: Vec::new(),
        });
    }
    for input in &bundle.manifest.automations {
        let key = input.key.as_deref().expect("validated bundle key");
        let operation = match rule_by_key.get(key) {
            None => BundleDiffOperation::Create,
            Some(model) if rule_semantically_matches(model, input, &personality_by_key)? => {
                BundleDiffOperation::Unchanged
            }
            Some(_) => BundleDiffOperation::Update,
        };
        objects.push(BundleObjectDiffView {
            object_type: "automation".to_owned(),
            key: key.to_owned(),
            name: input.name.clone(),
            operation,
            changes: Vec::new(),
        });
    }
    let wanted_personalities = bundle
        .manifest
        .personalities
        .iter()
        .map(|input| input.key.as_str())
        .collect::<BTreeSet<_>>();
    for model in &personalities {
        let key = model.managed_object_key.as_deref().unwrap_or_default();
        if !wanted_personalities.contains(key) {
            objects.push(BundleObjectDiffView {
                object_type: "personality".to_owned(),
                key: key.to_owned(),
                name: model.name.clone(),
                operation: BundleDiffOperation::Delete,
                changes: Vec::new(),
            });
        }
    }
    let wanted_rules = bundle
        .manifest
        .automations
        .iter()
        .filter_map(|input| input.key.as_deref())
        .collect::<BTreeSet<_>>();
    for model in &rules {
        let key = model.managed_object_key.as_deref().unwrap_or_default();
        if !wanted_rules.contains(key) {
            objects.push(BundleObjectDiffView {
                object_type: "automation".to_owned(),
                key: key.to_owned(),
                name: model.name.clone(),
                operation: BundleDiffOperation::Delete,
                changes: Vec::new(),
            });
        }
    }
    let has_deletions = objects
        .iter()
        .any(|object| object.operation == BundleDiffOperation::Delete);
    Ok(AutomationBundleDiffView {
        bundle_key: bundle.manifest.bundle_key.clone(),
        display_name: bundle.manifest.display_name.clone(),
        current_hash,
        manifest_hash: bundle.manifest_hash.clone(),
        objects,
        has_deletions,
    })
}

fn rule_semantically_matches(
    model: &automation_trigger::Model,
    input: &AutomationRuleInput,
    personalities: &BTreeMap<&str, &personality::Model>,
) -> Result<bool> {
    let personality_key = model.personality_id.and_then(|id| {
        personalities
            .iter()
            .find_map(|(key, personality)| (personality.id == id).then_some(*key))
    });
    Ok(model.name == input.name
        && model.enabled == input.enabled
        && model.activation == input.activation.as_storage()
        && model.effect == input.effect.as_storage()
        && model.schedule == input.schedule
        && model.tool_name == input.tool_name.as_storage()
        && model.mutability == input.mutability.as_storage()
        && personality_key == input.personality.as_deref()
        && normalize_markdown(&prompt_text::rich_text_to_prompt_markdown(&model.prompt)?)
            == input.prompt_markdown
        && automation_triggers::selector_from_storage(model.work_item_selector.as_deref())?
            == input.selector
        && model.priority == input.priority
        && model.exclusive == input.exclusive
        && model.produced_work_spec_json
            == input
                .produced_work
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?
        && model.postconditions_json
            == input
                .postconditions
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?
        && projects::normalize_optional(model.model_override.clone()) == input.execution.model
        && model.reasoning_effort_override.as_deref()
            == input
                .execution
                .reasoning_effort
                .map(|effort| effort.as_storage())
        && model.timeout_seconds == input.execution.timeout_seconds.map(|value| value as i64)
        && model.max_concurrent_runs
            == input
                .execution
                .max_concurrent_runs
                .map(|value| value as i64)
        && projects::normalize_optional(model.concurrency_group.clone())
            == input.execution.concurrency_group)
}

pub(crate) async fn apply_yaml(
    store: &Store,
    project_name: &str,
    yaml: &str,
    expected_current_hash: Option<&str>,
) -> Result<AutomationBundleApplyView> {
    let bundle = validate_yaml(yaml)?;
    let diff = diff_validated(store, project_name, &bundle).await?;
    if expected_current_hash != diff.current_hash.as_deref() {
        bail!(
            "bundle hash changed; expected {:?}, found {:?}",
            expected_current_hash,
            diff.current_hash
        );
    }
    let project_id = projects::project_id(store, project_name).await?;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start bundle apply")?;
    let current_hash = latest_bundle_hash(&txn, project_id, &bundle.manifest.bundle_key).await?;
    if expected_current_hash != current_hash.as_deref() {
        bail!("bundle changed while apply was starting");
    }
    let existing_personalities = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .all(&txn)
        .await
        .context("failed to load project personalities for bundle apply")?;
    let existing_rules = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .all(&txn)
        .await
        .context("failed to load project automations for bundle apply")?;
    reject_unmanaged_name_conflicts(&bundle, &existing_personalities, &existing_rules)?;
    let unchanged_personality_keys = diff
        .objects
        .iter()
        .filter(|object| {
            object.object_type == "personality"
                && object.operation == BundleDiffOperation::Unchanged
        })
        .map(|object| object.key.as_str())
        .collect::<BTreeSet<_>>();
    let unchanged_rule_keys = diff
        .objects
        .iter()
        .filter(|object| {
            object.object_type == "automation" && object.operation == BundleDiffOperation::Unchanged
        })
        .map(|object| object.key.as_str())
        .collect::<BTreeSet<_>>();

    let wanted_rule_keys = bundle
        .manifest
        .automations
        .iter()
        .filter_map(|input| input.key.as_deref())
        .collect::<BTreeSet<_>>();
    for model in existing_rules.iter().filter(|model| {
        model.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
            && !wanted_rule_keys.contains(model.managed_object_key.as_deref().unwrap_or_default())
    }) {
        AutomationTrigger::delete_by_id(model.id)
            .exec(&txn)
            .await
            .context("failed to delete removed managed automation")?;
    }

    let mut personality_ids = BTreeMap::new();
    for input in &bundle.manifest.personalities {
        let existing = existing_personalities.iter().find(|model| {
            model.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
                && model.managed_object_key.as_deref() == Some(input.key.as_str())
        });
        let description = markdown_to_html(&input.description);
        let model = match existing {
            Some(existing) if unchanged_personality_keys.contains(input.key.as_str()) => {
                existing.clone()
            }
            Some(existing) => {
                let mut active: PersonalityActiveModel = existing.clone().into();
                active.name = Set(input.name.clone());
                active.personality_description = Set(description);
                active.updated_at = Set(utc_now());
                active
                    .update(&txn)
                    .await
                    .context("failed to update managed personality")?
            }
            None => PersonalityActiveModel {
                project_id: Set(project_id),
                name: Set(input.name.clone()),
                personality_description: Set(description),
                current_revision_id: Set(None),
                managed_bundle_key: Set(Some(bundle.manifest.bundle_key.clone())),
                managed_object_key: Set(Some(input.key.clone())),
                created_at: Set(utc_now()),
                updated_at: Set(utc_now()),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .context("failed to create managed personality")?,
        };
        if !unchanged_personality_keys.contains(input.key.as_str()) {
            automation_revisions::record_personality_revision_in_conn(
                &txn,
                &model,
                RevisionChangeOperation::BundleApply,
                &RevisionActor::default(),
            )
            .await?;
        }
        personality_ids.insert(input.key.clone(), model.id);
    }

    for input in &bundle.manifest.automations {
        let key = input.key.as_deref().expect("validated automation key");
        let existing = existing_rules.iter().find(|model| {
            model.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
                && model.managed_object_key.as_deref() == Some(key)
        });
        if existing.is_some() && unchanged_rule_keys.contains(key) {
            continue;
        }
        let personality_id = input
            .personality
            .as_ref()
            .map(|key| {
                personality_ids
                    .get(key)
                    .copied()
                    .ok_or_else(|| report!("missing applied personality '{key}'"))
            })
            .transpose()?;
        let now = utc_now();
        let mut active = existing
            .cloned()
            .map(Into::<AutomationTriggerActiveModel>::into)
            .unwrap_or_default();
        if existing.is_none() {
            active.project_id = Set(project_id);
            active.evaluation_count = Set(0);
            active.pending_evaluation_count = Set(0);
            active.last_evaluation_queued_at = Set(None);
            active.last_evaluated_at = Set(None);
            active.last_event_id = Set(match input.activation {
                AutomationActivation::WorkItemCreated => {
                    automation_triggers::latest_item_created_event_id(store, project_id).await?
                }
                _ => None,
            });
            active.created_at = Set(now.clone());
            active.managed_bundle_key = Set(Some(bundle.manifest.bundle_key.clone()));
            active.managed_object_key = Set(Some(key.to_owned()));
        }
        active.name = Set(input.name.clone());
        active.enabled = Set(input.enabled);
        active.activation = Set(input.activation.as_storage().to_owned());
        active.effect = Set(input.effect.as_storage().to_owned());
        active.schedule = Set(input.schedule.clone());
        active.tool_name = Set(input.tool_name.as_storage().to_owned());
        active.mutability = Set(input.mutability.as_storage().to_owned());
        active.personality_id = Set(personality_id);
        active.prompt = Set(markdown_to_html(&input.prompt_markdown));
        active.work_item_selector = Set(automation_triggers::selector_to_storage(
            input.selector.as_ref(),
        )?);
        active.priority = Set(input.priority);
        active.exclusive = Set(input.exclusive);
        active.produced_work_spec_json = Set(input
            .produced_work
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?);
        active.postconditions_json = Set(input
            .postconditions
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?);
        active.model_override = Set(input.execution.model.clone());
        active.reasoning_effort_override = Set(input
            .execution
            .reasoning_effort
            .map(|value| value.as_storage().to_owned()));
        active.timeout_seconds = Set(input.execution.timeout_seconds.map(|value| value as i64));
        active.max_concurrent_runs = Set(input
            .execution
            .max_concurrent_runs
            .map(|value| value as i64));
        active.concurrency_group = Set(input.execution.concurrency_group.clone());
        active.next_evaluation_at = Set(match input.activation {
            AutomationActivation::Cron => {
                Some(automation_triggers::next_evaluation_at(&input.schedule)?)
            }
            _ => None,
        });
        active.updated_at = Set(now);
        let model = if existing.is_some() {
            active
                .update(&txn)
                .await
                .context("failed to update managed automation")?
        } else {
            active
                .insert(&txn)
                .await
                .context("failed to create managed automation")?
        };
        automation_revisions::record_trigger_revision_in_conn(
            &txn,
            &model,
            RevisionChangeOperation::BundleApply,
            &RevisionActor::default(),
        )
        .await?;
    }

    let wanted_personality_keys = bundle
        .manifest
        .personalities
        .iter()
        .map(|input| input.key.as_str())
        .collect::<BTreeSet<_>>();
    for model in existing_personalities.iter().filter(|model| {
        model.managed_bundle_key.as_deref() == Some(bundle.manifest.bundle_key.as_str())
            && !wanted_personality_keys
                .contains(model.managed_object_key.as_deref().unwrap_or_default())
    }) {
        let outside_reference = AutomationTrigger::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .filter(automation_trigger::Column::PersonalityId.eq(model.id))
            .one(&txn)
            .await
            .context("failed to validate managed personality deletion")?;
        if let Some(rule) = outside_reference {
            bail!(
                "cannot delete managed personality '{}' while automation '{}' references it",
                model.name,
                rule.name
            );
        }
        Personality::delete_by_id(model.id)
            .exec(&txn)
            .await
            .context("failed to delete removed managed personality")?;
    }

    let applied_at = utc_now();
    let apply = AutomationBundleApplyActiveModel {
        project_id: Set(project_id),
        bundle_key: Set(bundle.manifest.bundle_key.clone()),
        display_name: Set(bundle.manifest.display_name.clone()),
        manifest_hash: Set(bundle.manifest_hash.clone()),
        applied_diff_json: Set(serde_json::to_string(&diff)?),
        actor_type: Set(None),
        actor_id: Set(None),
        status: Set("applied".to_owned()),
        created_at: Set(applied_at.clone()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .context("failed to record bundle apply")?;
    txn.commit()
        .await
        .context("failed to commit bundle apply")?;
    events::publish_automation_changed(project_name);
    Ok(AutomationBundleApplyView {
        apply_id: apply.id,
        diff,
        status: apply.status,
        applied_at,
    })
}

fn reject_unmanaged_name_conflicts(
    bundle: &ValidatedBundle,
    personalities: &[personality::Model],
    rules: &[automation_trigger::Model],
) -> Result<()> {
    for input in &bundle.manifest.personalities {
        if let Some(existing) = personalities.iter().find(|model| model.name == input.name)
            && (existing.managed_bundle_key.as_deref() != Some(bundle.manifest.bundle_key.as_str())
                || existing.managed_object_key.as_deref() != Some(input.key.as_str()))
        {
            bail!(
                "personality name '{}' conflicts with an unmanaged or differently managed object",
                input.name
            );
        }
    }
    for input in &bundle.manifest.automations {
        let key = input.key.as_deref().expect("validated key");
        if let Some(existing) = rules.iter().find(|model| model.name == input.name)
            && (existing.managed_bundle_key.as_deref() != Some(bundle.manifest.bundle_key.as_str())
                || existing.managed_object_key.as_deref() != Some(key))
        {
            bail!(
                "automation name '{}' conflicts with an unmanaged or differently managed object",
                input.name
            );
        }
    }
    Ok(())
}

async fn latest_bundle_hash<C>(
    conn: &C,
    project_id: i64,
    bundle_key: &str,
) -> Result<Option<String>>
where
    C: ConnectionTrait,
{
    Ok(latest_bundle_apply(conn, project_id, bundle_key)
        .await?
        .filter(|apply| apply.status == "applied")
        .map(|apply| apply.manifest_hash))
}

async fn latest_bundle_apply<C>(
    conn: &C,
    project_id: i64,
    bundle_key: &str,
) -> Result<Option<automation_bundle_apply::Model>>
where
    C: ConnectionTrait,
{
    Ok(AutomationBundleApply::find()
        .filter(automation_bundle_apply::Column::ProjectId.eq(project_id))
        .filter(automation_bundle_apply::Column::BundleKey.eq(bundle_key))
        .order_by_desc(automation_bundle_apply::Column::Id)
        .one(conn)
        .await
        .context("failed to load latest bundle apply")?)
}

pub(crate) async fn list_installed(
    store: &Store,
    project_name: &str,
) -> Result<Vec<InstalledAutomationBundleView>> {
    let project_id = projects::project_id(store, project_name).await?;
    let applies = AutomationBundleApply::find()
        .filter(automation_bundle_apply::Column::ProjectId.eq(project_id))
        .order_by_asc(automation_bundle_apply::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load bundle apply history")?;
    let latest = applies.into_iter().fold(
        BTreeMap::<String, automation_bundle_apply::Model>::new(),
        |mut latest, apply| {
            latest.insert(apply.bundle_key.clone(), apply);
            latest
        },
    );
    let rules = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .all(store.db().as_ref())
        .await
        .context("failed to load installed bundle automations")?;
    let personalities = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .all(store.db().as_ref())
        .await
        .context("failed to load installed bundle personalities")?;
    let mut installed = latest
        .into_values()
        .filter(|apply| apply.status == "applied")
        .map(|apply| InstalledAutomationBundleView {
            apply_id: apply.id,
            automation_count: rules
                .iter()
                .filter(|rule| rule.managed_bundle_key.as_deref() == Some(&apply.bundle_key))
                .count() as u64,
            personality_count: personalities
                .iter()
                .filter(|personality| {
                    personality.managed_bundle_key.as_deref() == Some(&apply.bundle_key)
                })
                .count() as u64,
            bundle_key: apply.bundle_key,
            display_name: apply.display_name,
            manifest_hash: apply.manifest_hash,
            installed_at: apply.created_at,
        })
        .collect::<Vec<_>>();
    installed.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.bundle_key.cmp(&right.bundle_key))
    });
    Ok(installed)
}

pub(crate) async fn remove_bundle(
    store: &Store,
    project_name: &str,
    bundle_key: &str,
    expected_current_hash: Option<&str>,
) -> Result<AutomationBundleApplyView> {
    automation_triggers::validate_stable_key("bundle key", bundle_key)?;
    let project_id = projects::project_id(store, project_name).await?;
    let current = latest_bundle_apply(store.db().as_ref(), project_id, bundle_key)
        .await?
        .filter(|apply| apply.status == "applied")
        .ok_or_else(|| report!("bundle '{bundle_key}' is not installed in this project"))?;
    if expected_current_hash != Some(current.manifest_hash.as_str()) {
        bail!(
            "bundle hash changed; expected {:?}, found {:?}",
            expected_current_hash,
            Some(current.manifest_hash.as_str())
        );
    }

    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start bundle removal")?;
    let current = latest_bundle_apply(&txn, project_id, bundle_key)
        .await?
        .filter(|apply| apply.status == "applied")
        .ok_or_else(|| report!("bundle '{bundle_key}' is no longer installed"))?;
    if expected_current_hash != Some(current.manifest_hash.as_str()) {
        bail!("bundle changed while removal was starting");
    }
    let rules = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::ManagedBundleKey.eq(bundle_key))
        .all(&txn)
        .await
        .context("failed to load managed automations for bundle removal")?;
    let personalities = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .filter(personality::Column::ManagedBundleKey.eq(bundle_key))
        .all(&txn)
        .await
        .context("failed to load managed personalities for bundle removal")?;
    let mut objects = rules
        .iter()
        .map(|rule| BundleObjectDiffView {
            object_type: "automation".to_owned(),
            key: rule.managed_object_key.clone().unwrap_or_default(),
            name: rule.name.clone(),
            operation: BundleDiffOperation::Delete,
            changes: Vec::new(),
        })
        .chain(
            personalities
                .iter()
                .map(|personality| BundleObjectDiffView {
                    object_type: "personality".to_owned(),
                    key: personality.managed_object_key.clone().unwrap_or_default(),
                    name: personality.name.clone(),
                    operation: BundleDiffOperation::Delete,
                    changes: Vec::new(),
                }),
        )
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| {
        left.object_type
            .cmp(&right.object_type)
            .then_with(|| left.key.cmp(&right.key))
    });
    let diff = AutomationBundleDiffView {
        bundle_key: bundle_key.to_owned(),
        display_name: current.display_name.clone(),
        current_hash: Some(current.manifest_hash.clone()),
        manifest_hash: current.manifest_hash.clone(),
        has_deletions: !objects.is_empty(),
        objects,
    };

    for rule in &rules {
        AutomationTrigger::delete_by_id(rule.id)
            .exec(&txn)
            .await
            .context("failed to delete managed automation")?;
    }
    for personality in &personalities {
        if let Some(rule) = AutomationTrigger::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .filter(automation_trigger::Column::PersonalityId.eq(personality.id))
            .one(&txn)
            .await
            .context("failed to validate managed personality deletion")?
        {
            bail!(
                "cannot remove bundle while automation '{}' outside the bundle references personality '{}'",
                rule.name,
                personality.name
            );
        }
        Personality::delete_by_id(personality.id)
            .exec(&txn)
            .await
            .context("failed to delete managed personality")?;
    }

    let removed_at = utc_now();
    let removal = AutomationBundleApplyActiveModel {
        project_id: Set(project_id),
        bundle_key: Set(bundle_key.to_owned()),
        display_name: Set(current.display_name),
        manifest_hash: Set(current.manifest_hash),
        applied_diff_json: Set(serde_json::to_string(&diff)?),
        actor_type: Set(None),
        actor_id: Set(None),
        status: Set("removed".to_owned()),
        created_at: Set(removed_at.clone()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .context("failed to record bundle removal")?;
    txn.commit()
        .await
        .context("failed to commit bundle removal")?;
    events::publish_automation_changed(project_name);
    Ok(AutomationBundleApplyView {
        apply_id: removal.id,
        diff,
        status: removal.status,
        applied_at: removed_at,
    })
}

pub(crate) async fn export_yaml(
    store: &Store,
    project_name: &str,
    bundle_key: &str,
) -> Result<String> {
    automation_triggers::validate_stable_key("bundle key", bundle_key)?;
    let project_id = projects::project_id(store, project_name).await?;
    let apply = latest_bundle_apply(store.db().as_ref(), project_id, bundle_key)
        .await?
        .filter(|apply| apply.status == "applied")
        .ok_or_else(|| report!("bundle '{bundle_key}' has not been applied to this project"))?;
    let personalities = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .filter(personality::Column::ManagedBundleKey.eq(bundle_key))
        .order_by_asc(personality::Column::ManagedObjectKey)
        .all(store.db().as_ref())
        .await?;
    let rules = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::ManagedBundleKey.eq(bundle_key))
        .order_by_asc(automation_trigger::Column::ManagedObjectKey)
        .all(store.db().as_ref())
        .await?;
    let personality_inputs = personalities
        .iter()
        .map(|personality| {
            Ok(AutomationPersonalityInput {
                key: personality.managed_object_key.clone().unwrap_or_default(),
                name: personality.name.clone(),
                description: normalize_markdown(&prompt_text::rich_text_to_prompt_markdown(
                    &personality.personality_description,
                )?),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let personality_key_by_id = personalities
        .iter()
        .filter_map(|personality| {
            personality
                .managed_object_key
                .as_ref()
                .map(|key| (personality.id, key.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let automation_inputs = rules
        .into_iter()
        .map(|rule| {
            let view = automation_triggers::model_to_view(rule.clone())?;
            Ok(AutomationRuleInput {
                key: rule.managed_object_key,
                name: view.name,
                enabled: view.enabled,
                activation: view.activation,
                effect: view.effect,
                schedule: view.schedule,
                tool_name: view.tool_name,
                mutability: view.mutability,
                personality: view
                    .personality_id
                    .and_then(|id| personality_key_by_id.get(&id).cloned()),
                prompt_markdown: normalize_markdown(&prompt_text::rich_text_to_prompt_markdown(
                    &view.prompt,
                )?),
                selector: view.work_item_selector,
                priority: view.priority,
                exclusive: view.exclusive,
                produced_work: view.produced_work,
                execution: view.execution,
                postconditions: view.postconditions,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let manifest = AutomationBundleManifest {
        schema_version: 1,
        bundle_key: bundle_key.to_owned(),
        display_name: apply.display_name,
        personalities: personality_inputs,
        automations: automation_inputs,
    };
    Ok(yaml_serde::to_string(&manifest).context("failed to encode automation bundle YAML")?)
}

fn normalize_markdown(value: &str) -> String {
    value.replace("\r\n", "\n").trim().to_owned()
}

fn canonicalize_markdown(value: &str) -> Result<String> {
    let html = markdown_to_html(&normalize_markdown(value));
    Ok(normalize_markdown(
        &prompt_text::rich_text_to_prompt_markdown(&html)?,
    ))
}

pub(crate) fn markdown_to_html(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::projects::{CreateProject, create_project};

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        (temp, store)
    }

    #[test]
    fn reference_engineering_review_bundle_is_valid() {
        let bundle = validate_yaml(include_str!(
            "../../../examples/automation/engineering-review.yaml"
        ))
        .unwrap();
        assert_that!(&(bundle.manifest.bundle_key)).is_equal_to("engineering-review");
        assert_that!(&(bundle.manifest.personalities.len())).is_equal_to(2);
        assert_that!(&(bundle.manifest.automations.len())).is_equal_to(10);
    }

    #[test]
    fn bundle_rejects_unknown_fields() {
        let error = validate_yaml(
            "schema_version: 1\nbundle_key: demo\ndisplay_name: Demo\nunknown: true\n",
        )
        .unwrap_err();
        assert_that!(&(error.to_string().contains("unknown field"))).is_true();
    }

    #[tokio::test]
    async fn semantically_unchanged_apply_does_not_create_revisions() {
        let (_temp, store) = test_store().await;
        let yaml = include_str!("../../../examples/automation/engineering-review.yaml");
        let first = apply_yaml(&store, "demo", yaml, None).await.unwrap();
        let rules = automation_triggers::list_triggers(&store, "demo")
            .await
            .unwrap();
        let revision_ids = rules
            .iter()
            .map(|rule| rule.current_revision_id)
            .collect::<Vec<_>>();

        let second = apply_yaml(
            &store,
            "demo",
            yaml,
            Some(first.diff.manifest_hash.as_str()),
        )
        .await
        .unwrap();
        assert_that!(
            &(second
                .diff
                .objects
                .iter()
                .all(|object| object.operation == BundleDiffOperation::Unchanged))
        )
        .with_detail_message(format!("{:#?}", second.diff.objects))
        .is_true();
        let after = automation_triggers::list_triggers(&store, "demo")
            .await
            .unwrap();
        assert_that!(&(revision_ids)).is_equal_to(
            after
                .iter()
                .map(|rule| rule.current_revision_id)
                .collect::<Vec<_>>(),
        );

        let exported = export_yaml(&store, "demo", "engineering-review")
            .await
            .unwrap();
        let exported = validate_yaml(&exported).unwrap();
        assert_that!(&(exported.manifest_hash)).is_equal_to(first.diff.manifest_hash);
    }

    #[tokio::test]
    async fn installed_bundle_inventory_removal_and_reinstall_are_consistent() {
        let (_temp, store) = test_store().await;
        let yaml = include_str!("../../../examples/automation/engineering-review.yaml");
        let first = apply_yaml(&store, "demo", yaml, None).await.unwrap();

        let installed = list_installed(&store, "demo").await.unwrap();
        assert_that!(&(installed.len())).is_equal_to(1);
        assert_that!(&(installed[0].bundle_key)).is_equal_to("engineering-review");
        assert_that!(&(installed[0].automation_count)).is_equal_to(10);
        assert_that!(&(installed[0].personality_count)).is_equal_to(2);

        let stale = remove_bundle(&store, "demo", "engineering-review", Some("stale"))
            .await
            .unwrap_err();
        assert_that!(&(stale.to_string().contains("bundle hash changed"))).is_true();
        assert_that!(&(list_installed(&store, "demo").await.unwrap().len())).is_equal_to(1);

        let removed = remove_bundle(
            &store,
            "demo",
            "engineering-review",
            Some(&first.diff.manifest_hash),
        )
        .await
        .unwrap();
        assert_that!(&(removed.status)).is_equal_to("removed");
        assert_that!(&(removed.diff.has_deletions)).is_true();
        assert_that!(&(list_installed(&store, "demo").await.unwrap().is_empty())).is_true();
        assert_that!(
            &(export_yaml(&store, "demo", "engineering-review")
                .await
                .unwrap_err()
                .to_string()
                .contains("has not been applied"))
        )
        .is_true();

        let reapplied = apply_yaml(&store, "demo", yaml, None).await.unwrap();
        assert_that!(&(reapplied.status)).is_equal_to("applied");
        assert_that!(&(list_installed(&store, "demo").await.unwrap().len())).is_equal_to(1);
    }

    #[tokio::test]
    async fn bundle_removal_is_atomic_when_an_outside_rule_uses_a_managed_personality() {
        let (_temp, store) = test_store().await;
        let yaml = include_str!("../../../examples/automation/engineering-review.yaml");
        let applied = apply_yaml(&store, "demo", yaml, None).await.unwrap();
        let project_id = projects::project_id(&store, "demo").await.unwrap();
        let reviewer = Personality::find()
            .filter(personality::Column::ProjectId.eq(project_id))
            .filter(personality::Column::ManagedBundleKey.eq("engineering-review"))
            .filter(personality::Column::ManagedObjectKey.eq("reviewer"))
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap();
        automation_triggers::create_trigger(
            &store,
            "demo",
            automation_triggers::CreateAutomationTrigger {
                name: "Outside reviewer".to_owned(),
                enabled: false,
                activation: AutomationActivation::WorkItem,
                effect: AutomationEffect::ConsumeWork,
                schedule: "@every 15s".to_owned(),
                tool_name: None,
                mutability: crate::shared::view_models::AutomationRunMutability::ReadOnly,
                personality_id: Some(reviewer.id),
                prompt: "Outside rule".to_owned(),
                work_item_selector: Some(
                    crate::shared::view_models::default_automation_work_item_selector(),
                ),
                priority: 0,
            },
        )
        .await
        .unwrap();

        let error = remove_bundle(
            &store,
            "demo",
            "engineering-review",
            Some(&applied.diff.manifest_hash),
        )
        .await
        .unwrap_err();
        assert_that!(&(error.to_string().contains("outside the bundle references"))).is_true();
        assert_that!(&(list_installed(&store, "demo").await.unwrap().len())).is_equal_to(1);
        assert_that!(
            &(AutomationTrigger::find()
                .filter(automation_trigger::Column::ProjectId.eq(project_id))
                .filter(automation_trigger::Column::ManagedBundleKey.eq("engineering-review"))
                .all(store.db().as_ref())
                .await
                .unwrap()
                .len())
        )
        .is_equal_to(10);
    }
}
