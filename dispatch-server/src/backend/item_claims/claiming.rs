use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{ConnectionTrait, DatabaseTransaction, Statement, TransactionTrait};

use crate::{
    backend::{
        agent_ids,
        entities::work_item::WorkItemModel,
        events, projects,
        storage::{Store, utc_now},
        work_item_comments, work_item_events, work_item_labels, work_item_views, work_items,
        workflow_labels,
    },
    shared::view_models::{WorkItemEventType, WorkItemView},
};

use super::claim_candidates::{
    ClaimCandidate, ClaimCandidateScanner, ClaimSelector, has_matching_candidate,
};

pub(crate) async fn has_claimable_item_matching_condition(
    store: &Store,
    project_name: &str,
    condition: &Condition,
) -> Result<bool> {
    let selector = ClaimSelector::automation_condition(condition)?;
    let project_id = projects::project_id(store, project_name).await?;
    has_matching_candidate(store.db().as_ref(), project_id, &selector).await
}

pub(crate) async fn claim_item(
    store: &Store,
    project_name: &str,
    agent_id: &str,
    state_filter: &str,
) -> Result<Option<WorkItemView>> {
    agent_ids::validate_agent_id(agent_id)?;
    let selector = ClaimSelector::state(state_filter)?;

    let project_id = projects::project_id(store, project_name).await?;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start item claim")?;
    let item = claim_first_matching_candidate_in_tx(&txn, project_id, agent_id, &selector).await?;

    commit_claim_transaction(
        store,
        project_name,
        txn,
        item,
        "failed to commit item claim",
    )
    .await
}

pub(crate) async fn claim_item_matching_condition(
    store: &Store,
    project_name: &str,
    agent_id: &str,
    condition: &Condition,
) -> Result<Option<WorkItemView>> {
    agent_ids::validate_agent_id(agent_id)?;
    let selector = ClaimSelector::automation_condition(condition)?;

    let project_id = projects::project_id(store, project_name).await?;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start item claim")?;
    let item = claim_first_matching_candidate_in_tx(&txn, project_id, agent_id, &selector).await?;

    commit_claim_transaction(
        store,
        project_name,
        txn,
        item,
        "failed to commit item claim",
    )
    .await
}

pub(crate) async fn has_claimable_specific_item_matching_condition(
    store: &Store,
    project_name: &str,
    item_id: i64,
    condition: &Condition,
) -> Result<bool> {
    let selector = ClaimSelector::automation_condition(condition)?;
    let project_id = projects::project_id(store, project_name).await?;
    Ok(
        specific_claim_candidate_in_tx(store.db().as_ref(), project_id, item_id, Some(&selector))
            .await?
            .is_some(),
    )
}

pub(crate) async fn claim_specific_item_matching_condition(
    store: &Store,
    project_name: &str,
    item_id: i64,
    agent_id: &str,
    condition: &Condition,
) -> Result<Option<WorkItemView>> {
    agent_ids::validate_agent_id(agent_id)?;
    let selector = ClaimSelector::automation_condition(condition)?;

    let project_id = projects::project_id(store, project_name).await?;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start specific item claim")?;
    let candidate =
        specific_claim_candidate_in_tx(&txn, project_id, item_id, Some(&selector)).await?;
    let claimed = match candidate {
        Some(candidate) => {
            claim_candidate_in_tx(
                &txn,
                project_id,
                candidate.item_id,
                agent_id,
                &candidate.source_state,
            )
            .await?
        }
        None => None,
    };

    commit_claim_transaction(
        store,
        project_name,
        txn,
        claimed,
        "failed to commit specific item claim",
    )
    .await
}

pub(crate) async fn claim_specific_item(
    store: &Store,
    project_name: &str,
    item_id: i64,
    agent_id: &str,
) -> Result<Option<WorkItemView>> {
    agent_ids::validate_agent_id(agent_id)?;

    let project_id = projects::project_id(store, project_name).await?;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start specific item claim")?;
    let candidate = specific_claim_candidate_in_tx(&txn, project_id, item_id, None).await?;
    let claimed = match candidate {
        Some(candidate) => {
            claim_candidate_in_tx(
                &txn,
                project_id,
                candidate.item_id,
                agent_id,
                &candidate.source_state,
            )
            .await?
        }
        None => None,
    };

    commit_claim_transaction(
        store,
        project_name,
        txn,
        claimed,
        "failed to commit specific item claim",
    )
    .await
}

async fn claim_first_matching_candidate_in_tx<C>(
    conn: &C,
    project_id: i64,
    agent_id: &str,
    selector: &ClaimSelector,
) -> Result<Option<WorkItemModel>>
where
    C: ConnectionTrait,
{
    let mut scanner = ClaimCandidateScanner::new(conn, project_id, selector);
    while let Some(candidate) = scanner.next_matching_candidate().await? {
        let claimed = claim_candidate_in_tx(
            conn,
            project_id,
            candidate.item_id,
            agent_id,
            &candidate.source_state,
        )
        .await?;

        if claimed.is_some() {
            return Ok(claimed);
        }
    }

    Ok(None)
}

async fn specific_claim_candidate_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    selector: Option<&ClaimSelector>,
) -> Result<Option<ClaimCandidate>>
where
    C: ConnectionTrait,
{
    let existing = work_items::get(conn, project_id, item_id).await?;
    if existing.claimed_by.is_some() || existing.finished_at.is_some() {
        return Ok(None);
    }
    let labels = work_item_labels::for_item(conn, project_id, item_id).await?;
    if selector.is_some_and(|selector| !selector.matches(&labels)) {
        return Ok(None);
    }

    Ok(Some(ClaimCandidate {
        item_id,
        source_state: workflow_labels::source_state_for_new_claim(&labels),
    }))
}

async fn claim_candidate_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    agent_id: &str,
    source_state: &str,
) -> Result<Option<WorkItemModel>>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    let sql = r#"
        UPDATE work_items
        SET claimed_by = ?3,
            claimed_at = ?4,
            claim_expires_at = NULL,
            version = version + 1,
            updated_at = ?4
        WHERE id = ?2
          AND project_id = ?1
          AND claimed_by IS NULL
          AND finished_at IS NULL
        RETURNING id
        "#;

    let claimed_id = conn
        .query_one(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            sql,
            vec![
                project_id.into(),
                item_id.into(),
                agent_id.to_owned().into(),
                now.into(),
            ],
        ))
        .await
        .context("failed to claim work item")?
        .map(|row| row.try_get::<i64>("", "id"))
        .transpose()
        .context("failed to read claimed item id")?;

    let Some(item_id) = claimed_id else {
        return Ok(None);
    };

    Ok(Some(
        record_claim_in_tx(conn, project_id, item_id, agent_id, source_state).await?,
    ))
}

async fn commit_claim_transaction(
    store: &Store,
    project_name: &str,
    txn: DatabaseTransaction,
    item: Option<WorkItemModel>,
    commit_context: &'static str,
) -> Result<Option<WorkItemView>> {
    txn.commit().await.context(commit_context)?;

    let Some(item) = item else {
        return Ok(None);
    };

    events::publish_work_item_changed(project_name, item.id);
    Ok(Some(work_item_views::model_to_view(store, item).await?))
}

async fn record_claim_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    agent_id: &str,
    source_state: &str,
) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    workflow_labels::apply_plan_in_tx(
        conn,
        project_id,
        item_id,
        workflow_labels::new_claim_workflow_label_plan(source_state),
    )
    .await?;
    let comment_body = format!("Claimed by {agent_id}");
    work_item_comments::insert_system_in_tx(conn, item_id, comment_body.as_str()).await?;
    work_item_events::record_event_with_attribution_in_tx(
        conn,
        project_id,
        Some(item_id),
        WorkItemEventType::ItemClaimed,
        comment_body.as_str(),
        work_item_events::agent_event_attribution(agent_id),
    )
    .await?;
    work_items::get(conn, project_id, item_id).await
}
