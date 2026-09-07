//! Typed persistence boundary for Dispatch's workflow audit stream.
//!
//! The SeaORM entity mirrors SQLite and therefore stores event and actor kinds as text. Service
//! code uses the shared enums accepted here, keeping string conversion in this module instead of
//! distributing event-name literals across workflow implementations.

use super::model::EventAttribution;
use rootcause::{Result, prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};

use crate::{
    backend::{
        entities::work_item_event::{self, WorkItemEventActiveModel},
        storage::utc_now,
    },
    shared::view_models::WorkItemEventType,
};

pub(crate) async fn record_event_in_tx<C>(
    conn: &C,
    project_id: i64,
    work_item_id: Option<i64>,
    event_type: WorkItemEventType,
    body: &str,
) -> Result<WorkItemEventView>
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
) -> Result<WorkItemEventView>
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
    Ok(WorkItemEventView {
        id: event.id,
        project_id: event.project_id,
        work_item_id: event.work_item_id,
        event_type,
        body: event.body,
        actor_type: attribution.actor_type,
        actor_id: event.actor_id,
        agent_run_id: event.agent_run_id,
        created_at: event.created_at,
    })
}

use crate::backend::storage::Transaction;
use dispatch_types::WorkItemEventView;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
pub(crate) struct EventRepository;
impl EventRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<WorkItemEventView>> {
        if let Some(item_id) = item_id {
            crate::backend::items::repository::records::get(
                transaction.connection(),
                project_id,
                item_id,
            )
            .await?;
        }

        let mut query = work_item_event::Entity::find()
            .filter(work_item_event::Column::ProjectId.eq(project_id))
            .order_by_asc(work_item_event::Column::Id);

        if let Some(item_id) = item_id {
            query = query.filter(work_item_event::Column::WorkItemId.eq(item_id));
        }

        if let Some(since_id) = since_id {
            query = query.filter(work_item_event::Column::Id.gt(since_id));
        }

        let events = query
            .all(transaction.connection())
            .await
            .context("failed to list work item events")?;

        events.into_iter().map(decode).collect()
    }
}

pub(crate) fn decode(event: work_item_event::Model) -> Result<WorkItemEventView> {
    let event_type = event
        .event_type
        .parse::<WorkItemEventType>()
        .context_with(|| {
            format!(
                "work item event {} has invalid type '{}'",
                event.id, event.event_type
            )
        })?;
    let actor_type = event
        .actor_type
        .as_deref()
        .map(str::parse)
        .transpose()
        .context_with(|| {
            format!(
                "work item event {} has invalid actor type {:?}",
                event.id, event.actor_type
            )
        })?;
    Ok(WorkItemEventView {
        id: event.id,
        project_id: event.project_id,
        work_item_id: event.work_item_id,
        event_type,
        body: event.body,
        actor_type,
        actor_id: event.actor_id,
        agent_run_id: event.agent_run_id,
        created_at: event.created_at,
    })
}
