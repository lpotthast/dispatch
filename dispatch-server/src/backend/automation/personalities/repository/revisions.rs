use crate::backend::{
    automation::revisions::model::RevisionActor,
    entities::{
        personality::{PersonalityActiveModel, PersonalityModel},
        personality_revision::{self, PersonalityRevision, PersonalityRevisionActiveModel},
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::{PersonalityRevisionView, RevisionChangeOperation};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};
use sha2::{Digest, Sha256};
pub(crate) async fn record_in_conn<C>(
    conn: &C,
    personality: &PersonalityModel,
    operation: RevisionChangeOperation,
    actor: &RevisionActor,
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let revision_number = PersonalityRevision::find()
        .filter(personality_revision::Column::PersonalityId.eq(personality.id))
        .order_by_desc(personality_revision::Column::RevisionNumber)
        .one(conn)
        .await
        .context("failed to load current personality revision")?
        .map(|revision| revision.revision_number.saturating_add(1))
        .unwrap_or(1);
    let canonical = serde_json::to_string(&(
        personality.name.as_str(),
        personality.personality_description.as_str(),
        personality.managed_bundle_key.as_deref(),
        personality.managed_object_key.as_deref(),
    ))
    .context("failed to encode personality revision")?;
    let sha256 = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    let revision = PersonalityRevisionActiveModel {
        personality_id: Set(Some(personality.id)),
        project_id: Set(personality.project_id),
        personality_name: Set(personality.name.clone()),
        revision_number: Set(revision_number),
        personality_description: Set(personality.personality_description.clone()),
        sha256: Set(sha256),
        change_operation: Set(operation.as_storage().to_owned()),
        actor_type: Set(actor.actor_type.map(|kind| kind.as_storage().to_owned())),
        actor_id: Set(actor.actor_id.clone()),
        created_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to create personality revision")?;
    let mut active: PersonalityActiveModel = personality.clone().into();
    active.current_revision_id = Set(Some(revision.id));
    active
        .update(conn)
        .await
        .context("failed to set current personality revision")?;
    Ok(revision.id)
}

pub(crate) async fn list_in(
    transaction: &Transaction,
    project_id: i64,
    personality_id: i64,
) -> Result<Vec<PersonalityRevisionView>> {
    PersonalityRevision::find()
        .filter(personality_revision::Column::ProjectId.eq(project_id))
        .filter(personality_revision::Column::PersonalityId.eq(personality_id))
        .order_by_desc(personality_revision::Column::RevisionNumber)
        .all(transaction.connection())
        .await
        .context("failed to list personality revisions")?
        .into_iter()
        .map(decode)
        .collect()
}
pub(crate) async fn get_in(
    transaction: &Transaction,
    project_id: i64,
    personality_id: i64,
    revision_id: i64,
) -> Result<PersonalityRevisionView> {
    decode(
        PersonalityRevision::find_by_id(revision_id)
            .filter(personality_revision::Column::ProjectId.eq(project_id))
            .filter(personality_revision::Column::PersonalityId.eq(personality_id))
            .one(transaction.connection())
            .await
            .context("failed to load personality revision")?
            .ok_or_else(|| {
                report!("revision {revision_id} does not belong to personality {personality_id}")
            })?,
    )
}
fn decode(revision: personality_revision::Model) -> Result<PersonalityRevisionView> {
    Ok(PersonalityRevisionView {
        id: revision.id,
        personality_id: revision.personality_id,
        project_id: revision.project_id,
        revision_number: revision.revision_number,
        name: revision.personality_name,
        personality_description: revision.personality_description,
        sha256: revision.sha256,
        operation: revision.change_operation.parse()?,
        actor_type: revision.actor_type.as_deref().map(str::parse).transpose()?,
        actor_id: revision.actor_id,
        created_at: revision.created_at,
    })
}
