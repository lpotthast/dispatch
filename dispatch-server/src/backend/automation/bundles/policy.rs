use super::model::ValidatedBundle;
use crate::backend::execution::prompt_text::markdown_to_html;
use crate::backend::{automation::rules as automation_triggers, execution::prompt_text, projects};
use dispatch_types::{
    AutomationBundleDiffView, AutomationBundleManifest, AutomationEffect,
    AutomationPersonalityInput, AutomationRuleInput, AutomationTriggerView, BundleDiffOperation,
    BundleObjectDiffView, PersonalityView,
};
use rootcause::{Result, prelude::*};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn validate_yaml(yaml: &str) -> Result<ValidatedBundle> {
    let mut manifest = yaml_serde::from_str::<AutomationBundleManifest>(yaml)
        .context("invalid automation bundle YAML")?;
    if manifest.schema_version != 1 {
        bail!(
            "unsupported automation bundle schema_version {}; expected 1",
            manifest.schema_version
        );
    }
    automation_triggers::policy::validate_stable_key("bundle_key", &manifest.bundle_key)?;
    if manifest.display_name.trim().is_empty() {
        bail!("bundle display_name cannot be empty");
    }
    manifest.display_name = manifest.display_name.trim().to_owned();
    let mut personality_keys = BTreeSet::new();
    let mut personality_names = BTreeSet::new();
    for (index, personality) in manifest.personalities.iter_mut().enumerate() {
        automation_triggers::policy::validate_stable_key(
            &format!("personalities[{index}].key"),
            &personality.key,
        )?;
        if !personality_keys.insert(personality.key.clone()) {
            bail!("duplicate personality key '{}'", personality.key);
        }
        personality.name = crate::backend::automation::personalities::policy::normalize_name(
            std::mem::take(&mut personality.name),
        )?;
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
        automation_triggers::policy::validate_stable_key(
            &format!("automations[{index}].key"),
            key,
        )?;
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
        automation_triggers::policy::validate_rule_input(rule)
            .context_with(|| format!("invalid automations[{index}] configuration"))?;
        if rule.effect == AutomationEffect::ConsumeWork {
            let personality = rule.personality.as_deref().ok_or_else(|| {
                report!("automations[{index}].personality is required for consume_work")
            })?;
            if !personality_keys.contains(personality) {
                bail!("automations[{index}].personality references unknown key '{personality}'");
            }
        } else if rule.personality.is_some() || rule.selector.is_some() {
            bail!("automations[{index}] has consume-work fields on a produce_work automation");
        }
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
pub(crate) fn normalize_markdown(value: &str) -> String {
    value.replace("\r\n", "\n").trim().to_owned()
}
fn canonicalize_markdown(value: &str) -> Result<String> {
    let html = markdown_to_html(&normalize_markdown(value));
    Ok(normalize_markdown(
        &prompt_text::rich_text_to_prompt_markdown(&html)?,
    ))
}
pub(crate) fn diff(
    bundle: &ValidatedBundle,
    current_hash: Option<String>,
    personalities: &[PersonalityView],
    rules: &[AutomationTriggerView],
) -> Result<AutomationBundleDiffView> {
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
    for model in personalities {
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
    for model in rules {
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
pub(crate) fn reject_unmanaged_name_conflicts(
    bundle: &ValidatedBundle,
    personalities: &[PersonalityView],
    rules: &[AutomationTriggerView],
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
fn rule_semantically_matches(
    model: &AutomationTriggerView,
    input: &AutomationRuleInput,
    personalities: &BTreeMap<&str, &PersonalityView>,
) -> Result<bool> {
    let personality_key = model.personality_id.and_then(|id| {
        personalities
            .iter()
            .find_map(|(key, personality)| (personality.id == id).then_some(*key))
    });
    Ok(model.name == input.name
        && model.enabled == input.enabled
        && model.activation == input.activation
        && model.effect == input.effect
        && model.schedule == input.schedule
        && model.tool_name == input.tool_name
        && model.mutability == input.mutability
        && personality_key == input.personality.as_deref()
        && normalize_markdown(&prompt_text::rich_text_to_prompt_markdown(&model.prompt)?)
            == input.prompt_markdown
        && model.work_item_selector == input.selector
        && model.priority == input.priority
        && model.exclusive == input.exclusive
        && serde_json::to_value(&model.produced_work)?
            == serde_json::to_value(&input.produced_work)?
        && serde_json::to_value(&model.postconditions)?
            == serde_json::to_value(&input.postconditions)?
        && projects::normalize_optional(model.execution.model.clone()) == input.execution.model
        && model.execution.reasoning_effort == input.execution.reasoning_effort
        && model.execution.timeout_seconds == input.execution.timeout_seconds
        && model.execution.max_concurrent_runs == input.execution.max_concurrent_runs
        && projects::normalize_optional(model.execution.concurrency_group.clone())
            == input.execution.concurrency_group)
}
pub(crate) fn export(
    bundle_key: &str,
    display_name: String,
    personalities: &[PersonalityView],
    rules: Vec<AutomationTriggerView>,
) -> Result<String> {
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
            let view = rule.clone();
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
        display_name,
        personalities: personality_inputs,
        automations: automation_inputs,
    };
    Ok(yaml_serde::to_string(&manifest).context("failed to encode automation bundle YAML")?)
}
