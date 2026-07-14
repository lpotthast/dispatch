//! Typed persistence boundary for Dispatch's workflow audit stream.
//!
//! The SeaORM entity mirrors SQLite and therefore stores event and actor kinds as text. Service
//! code uses the shared enums accepted here, keeping string conversion in this module instead of
//! distributing event-name literals across workflow implementations.

use rootcause::{Result, prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};

use crate::{
    backend::{
        entities::work_item_event::{self, WorkItemEventActiveModel},
        storage::utc_now,
    },
    shared::view_models::{AuthorType, WorkItemEventType},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EventAttribution<'a> {
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<&'a str>,
    pub agent_run_id: Option<i64>,
}

pub(crate) fn agent_event_attribution(agent_id: &str) -> EventAttribution<'_> {
    EventAttribution {
        actor_type: Some(AuthorType::Agent),
        actor_id: Some(agent_id),
        agent_run_id: crate::backend::agent_ids::parse_dispatch_run_agent_id(agent_id),
    }
}

pub(crate) async fn record_event_in_tx<C>(
    conn: &C,
    project_id: i64,
    work_item_id: Option<i64>,
    event_type: WorkItemEventType,
    body: &str,
) -> Result<work_item_event::Model>
where
    C: sea_orm::ConnectionTrait,
{
    record_event_with_attribution_in_tx(
        conn,
        project_id,
        work_item_id,
        event_type,
        body,
        EventAttribution::default(),
    )
    .await
}

pub(crate) async fn record_event_with_attribution_in_tx<C>(
    conn: &C,
    project_id: i64,
    work_item_id: Option<i64>,
    event_type: WorkItemEventType,
    body: &str,
    attribution: EventAttribution<'_>,
) -> Result<work_item_event::Model>
where
    C: sea_orm::ConnectionTrait,
{
    let active = WorkItemEventActiveModel {
        project_id: Set(project_id),
        work_item_id: Set(work_item_id),
        event_type: Set(event_type.as_storage().to_owned()),
        body: Set(body.to_owned()),
        actor_type: Set(attribution
            .actor_type
            .map(|actor_type| actor_type.as_storage().to_owned())),
        actor_id: Set(attribution.actor_id.map(ToOwned::to_owned)),
        agent_run_id: Set(attribution.agent_run_id),
        created_at: Set(utc_now()),
        ..Default::default()
    };
    let event = active
        .insert(conn)
        .await
        .context_with(|| format!("failed to record event {event_type}"))?;
    Ok(event)
}
