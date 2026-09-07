use super::super::configuration::*;
use super::{encoding::selector_to_storage, revisions};
use crate::backend::{
    automation::revisions::model::RevisionActor,
    entities::automation_trigger::{self, AutomationTrigger, AutomationTriggerActiveModel},
    storage::utc_now,
};
use crudkit_core::condition::Condition;
use dispatch_types::{
    AutomationActivation, AutomationEffect, AutomationRunMutability, RevisionChangeOperation,
};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect,
};
struct DefaultProjectAutomation {
    name: &'static str,
    prompt: &'static str,
    selector: fn() -> Condition,
    priority: i64,
    mutability: AutomationRunMutability,
}

fn default_project_automations() -> [DefaultProjectAutomation; 3] {
    [
        DefaultProjectAutomation {
            name: DEFAULT_WORK_ITEM_AUTOMATION_NAME,
            prompt: "",
            selector: default_work_item_selector,
            priority: 0,
            mutability: AutomationRunMutability::Mutating,
        },
        DefaultProjectAutomation {
            name: DEFAULT_REFINEMENT_AUTOMATION_NAME,
            prompt: DEFAULT_REFINEMENT_AUTOMATION_PROMPT,
            selector: default_refinement_work_item_selector,
            priority: REFINEMENT_AUTOMATION_PRIORITY,
            mutability: AutomationRunMutability::ReadOnly,
        },
        DefaultProjectAutomation {
            name: DEFAULT_VERIFICATION_AUTOMATION_NAME,
            prompt: DEFAULT_VERIFICATION_AUTOMATION_PROMPT,
            selector: default_verification_work_item_selector,
            priority: VERIFICATION_AUTOMATION_PRIORITY,
            mutability: AutomationRunMutability::ReadOnly,
        },
    ]
}
pub(crate) async fn ensure_default_project_automations_in_conn<C>(
    conn: &C,
    project_id: i64,
    default_tool: &str,
) -> Result<()>
where
    C: ConnectionTrait,
{
    for default in default_project_automations() {
        ensure_default_project_automation_in_conn(conn, project_id, default_tool, default).await?;
    }
    Ok(())
}
async fn ensure_default_project_automation_in_conn<C>(
    conn: &C,
    project_id: i64,
    default_tool: &str,
    default: DefaultProjectAutomation,
) -> Result<()>
where
    C: ConnectionTrait,
{
    let existing = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::Name.eq(default.name))
        .limit(1)
        .one(conn)
        .await
        .context("failed to check project automation defaults")?;
    if existing.is_some() {
        return Ok(());
    }

    let selector = (default.selector)();
    let personality_id =
        crate::backend::automation::personalities::repository::default_personality_id_in_conn(
            conn, project_id,
        )
        .await?;
    let now = utc_now();
    let trigger = AutomationTriggerActiveModel {
        project_id: Set(project_id),
        name: Set(default.name.to_owned()),
        enabled: Set(true),
        activation: Set(AutomationActivation::WorkItem.as_storage().to_owned()),
        effect: Set(AutomationEffect::ConsumeWork.as_storage().to_owned()),
        schedule: Set(DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE.to_owned()),
        tool_name: Set(default_tool.to_owned()),
        mutability: Set(default.mutability.as_storage().to_owned()),
        personality_id: Set(Some(personality_id)),
        prompt: Set(default.prompt.to_owned()),
        work_item_selector: Set(selector_to_storage(Some(&selector))?),
        priority: Set(default.priority),
        evaluation_count: Set(0),
        pending_evaluation_count: Set(0),
        last_evaluation_queued_at: Set(None),
        last_evaluated_at: Set(None),
        next_evaluation_at: Set(None),
        last_event_id: Set(None),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to create default project automation")?;

    revisions::record_in_conn(
        conn,
        &trigger,
        RevisionChangeOperation::Create,
        &RevisionActor::default(),
    )
    .await?;

    Ok(())
}
