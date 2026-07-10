use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::{
    backend::{
        agent_ids,
        entities::{project::ProjectModel, work_item_event},
        work_item_events,
    },
    shared::view_models::{
        AuthorType, ProjectMemoryEventView, ProjectSystemPromptEventView, WorkItemEventType,
    },
};

pub(super) const SYSTEM_PROMPT_CHANGED_EVENT_TYPE: WorkItemEventType =
    WorkItemEventType::SystemPromptChanged;
pub(super) const MEMORY_CHANGED_EVENT_TYPE: WorkItemEventType = WorkItemEventType::MemoryChanged;

#[derive(Clone, Debug)]
pub enum ProjectChangeSource {
    Agent {
        agent_id: String,
        agent_run_id: Option<i64>,
    },
    User,
    System,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct MemoryChangedBody {
    pub(super) operation: String,
    pub(super) memory: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SystemPromptChangedBody {
    pub(super) operation: String,
    pub(super) system_prompt: String,
}

impl ProjectChangeSource {
    fn actor_type(&self) -> AuthorType {
        match self {
            Self::Agent { .. } => AuthorType::Agent,
            Self::User => AuthorType::User,
            Self::System => AuthorType::System,
        }
    }

    fn actor_id(&self) -> Option<&str> {
        match self {
            Self::Agent { agent_id, .. } => Some(agent_id.as_str()),
            Self::User | Self::System => None,
        }
    }

    fn agent_run_id(&self) -> Option<i64> {
        match self {
            Self::Agent {
                agent_id,
                agent_run_id,
            } => agent_run_id.or_else(|| agent_ids::parse_dispatch_run_agent_id(agent_id)),
            Self::User | Self::System => None,
        }
    }
}

pub(super) async fn record_system_prompt_changed_in_tx<C>(
    conn: &C,
    project: &ProjectModel,
    operation: &str,
    source: &ProjectChangeSource,
) -> Result<work_item_event::Model>
where
    C: ConnectionTrait,
{
    let body = serde_json::to_string(&SystemPromptChangedBody {
        operation: operation.to_owned(),
        system_prompt: project.system_prompt.clone(),
    })
    .context("failed to encode project system prompt event")?;
    record_project_text_event(
        conn,
        project.id,
        SYSTEM_PROMPT_CHANGED_EVENT_TYPE,
        &body,
        source,
    )
    .await
}

pub(super) async fn record_memory_changed_in_tx<C>(
    conn: &C,
    project: &ProjectModel,
    operation: &str,
    source: &ProjectChangeSource,
) -> Result<work_item_event::Model>
where
    C: ConnectionTrait,
{
    let body = serde_json::to_string(&MemoryChangedBody {
        operation: operation.to_owned(),
        memory: project.memory.clone(),
    })
    .context("failed to encode project memory event")?;
    record_project_text_event(conn, project.id, MEMORY_CHANGED_EVENT_TYPE, &body, source).await
}

pub(super) async fn list_system_prompt_events<C>(
    conn: &C,
    project_id: i64,
    project_name: &str,
) -> Result<Vec<ProjectSystemPromptEventView>>
where
    C: ConnectionTrait,
{
    let events = list_project_text_events(conn, project_id, SYSTEM_PROMPT_CHANGED_EVENT_TYPE)
        .await
        .context("failed to list project system prompt events")?;
    Ok(events
        .into_iter()
        .map(|event| system_prompt_event_to_view(project_name, event))
        .collect())
}

pub(super) async fn list_memory_events<C>(
    conn: &C,
    project_id: i64,
    project_name: &str,
) -> Result<Vec<ProjectMemoryEventView>>
where
    C: ConnectionTrait,
{
    let events = list_project_text_events(conn, project_id, MEMORY_CHANGED_EVENT_TYPE)
        .await
        .context("failed to list project memory events")?;
    Ok(events
        .into_iter()
        .map(|event| memory_event_to_view(project_name, event))
        .collect())
}

pub(super) async fn compact_system_prompt_events<C>(conn: &C, project_id: i64) -> Result<u64>
where
    C: ConnectionTrait,
{
    Ok(
        compact_project_text_events(conn, project_id, SYSTEM_PROMPT_CHANGED_EVENT_TYPE)
            .await
            .context("failed to compact project system prompt events")?,
    )
}

pub(super) async fn compact_memory_events<C>(conn: &C, project_id: i64) -> Result<u64>
where
    C: ConnectionTrait,
{
    Ok(
        compact_project_text_events(conn, project_id, MEMORY_CHANGED_EVENT_TYPE)
            .await
            .context("failed to compact project memory events")?,
    )
}

pub(super) async fn latest_memory_event<C>(
    conn: &C,
    project_id: i64,
) -> Result<Option<work_item_event::Model>>
where
    C: ConnectionTrait,
{
    Ok(
        latest_project_text_event(conn, project_id, MEMORY_CHANGED_EVENT_TYPE)
            .await
            .context("failed to load latest project memory event")?,
    )
}

pub(super) async fn memory_event_exists<C>(
    conn: &C,
    project_id: i64,
    event_id: i64,
) -> Result<Option<String>>
where
    C: ConnectionTrait,
{
    Ok(work_item_event::Entity::find_by_id(event_id)
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq(MEMORY_CHANGED_EVENT_TYPE.as_storage()))
        .one(conn)
        .await
        .context("failed to load project memory event")?
        .map(|event| event.created_at))
}

pub(super) fn system_prompt_event_to_view(
    project_name: &str,
    event: work_item_event::Model,
) -> ProjectSystemPromptEventView {
    let parsed = serde_json::from_str::<SystemPromptChangedBody>(&event.body).ok();
    ProjectSystemPromptEventView {
        id: event.id,
        project_id: event.project_id,
        project_name: project_name.to_owned(),
        operation: parsed
            .as_ref()
            .map(|body| body.operation.clone())
            .unwrap_or_else(|| "unknown".to_owned()),
        system_prompt: parsed
            .map(|body| body.system_prompt)
            .unwrap_or_else(|| event.body.clone()),
        actor_type: event.actor_type,
        actor_id: event.actor_id,
        agent_run_id: event.agent_run_id,
        created_at: event.created_at,
    }
}

pub(super) fn memory_event_to_view(
    project_name: &str,
    event: work_item_event::Model,
) -> ProjectMemoryEventView {
    let parsed = serde_json::from_str::<MemoryChangedBody>(&event.body).ok();
    ProjectMemoryEventView {
        id: event.id,
        project_id: event.project_id,
        project_name: project_name.to_owned(),
        operation: parsed
            .as_ref()
            .map(|body| body.operation.clone())
            .unwrap_or_else(|| "unknown".to_owned()),
        memory: parsed
            .map(|body| body.memory)
            .unwrap_or_else(|| event.body.clone()),
        actor_type: event.actor_type,
        actor_id: event.actor_id,
        agent_run_id: event.agent_run_id,
        created_at: event.created_at,
    }
}

async fn record_project_text_event<C>(
    conn: &C,
    project_id: i64,
    event_type: WorkItemEventType,
    body: &str,
    source: &ProjectChangeSource,
) -> Result<work_item_event::Model>
where
    C: ConnectionTrait,
{
    work_item_events::record_event_with_attribution_in_tx(
        conn,
        project_id,
        None,
        event_type,
        body,
        work_item_events::EventAttribution {
            actor_type: Some(source.actor_type()),
            actor_id: source.actor_id(),
            agent_run_id: source.agent_run_id(),
        },
    )
    .await
}

async fn list_project_text_events<C>(
    conn: &C,
    project_id: i64,
    event_type: WorkItemEventType,
) -> Result<Vec<work_item_event::Model>>
where
    C: ConnectionTrait,
{
    Ok(work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq(event_type.as_storage()))
        .order_by_desc(work_item_event::Column::Id)
        .all(conn)
        .await?)
}

async fn compact_project_text_events<C>(
    conn: &C,
    project_id: i64,
    event_type: WorkItemEventType,
) -> Result<u64>
where
    C: ConnectionTrait,
{
    Ok(work_item_event::Entity::delete_many()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq(event_type.as_storage()))
        .exec(conn)
        .await?
        .rows_affected)
}

async fn latest_project_text_event<C>(
    conn: &C,
    project_id: i64,
    event_type: WorkItemEventType,
) -> Result<Option<work_item_event::Model>>
where
    C: ConnectionTrait,
{
    Ok(work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq(event_type.as_storage()))
        .order_by_desc(work_item_event::Column::Id)
        .one(conn)
        .await?)
}
