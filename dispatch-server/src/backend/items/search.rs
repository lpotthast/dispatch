use std::collections::BTreeSet;

use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, Condition as SeaCondition, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    QueryTrait,
    sea_query::{Expr, LikeExpr},
};

use crate::{
    backend::{
        entities::{
            work_item::{self, WorkItem},
            work_item_origin::{self, WorkItemOrigin},
            work_item_relationship::{self, WorkItemRelationship},
        },
        label_conditions::ValidatedLabelCondition,
        projects,
        storage::Store,
        work_item_views, workflow_labels,
    },
    shared::view_models::{WorkItemPage, WorkItemSearchRequest, WorkItemView},
};

const DEFAULT_SEARCH_LIMIT: u64 = 50;
const MAX_SEARCH_LIMIT: u64 = 200;

#[cfg(not(test))]
const SEARCH_SCAN_BATCH_SIZE: u64 = 256;
#[cfg(test)]
const SEARCH_SCAN_BATCH_SIZE: u64 = 2;

pub async fn search_items(
    store: &Store,
    project_name: &str,
    request: WorkItemSearchRequest,
) -> Result<WorkItemPage> {
    let project_id = projects::project_id(store, project_name).await?;
    let plan = WorkItemSearchPlan::new(request)?;
    let mut query = WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .order_by_desc(work_item::Column::UpdatedAt)
        .order_by_desc(work_item::Column::Id);

    if let Some(finished) = plan.finished {
        query = if finished {
            query.filter(work_item::Column::FinishedAt.is_not_null())
        } else {
            query.filter(work_item::Column::FinishedAt.is_null())
        };
    }
    if let Some(pattern) = &plan.text_pattern {
        query = query.filter(
            SeaCondition::any()
                .add(
                    Expr::col(work_item::Column::Title)
                        .like(LikeExpr::new(pattern.clone()).escape('\\')),
                )
                .add(
                    Expr::col(work_item::Column::Description)
                        .like(LikeExpr::new(pattern.clone()).escape('\\')),
                ),
        );
    }
    if let Some(updated_since) = &plan.updated_since {
        query = query.filter(work_item::Column::UpdatedAt.gte(updated_since));
    }
    if plan.created_by_run.is_some() || plan.produced_by_trigger.is_some() {
        let mut matching_origins = WorkItemOrigin::find()
            .select_only()
            .column(work_item_origin::Column::WorkItemId)
            .filter(work_item_origin::Column::ProjectId.eq(project_id));
        if let Some(run_id) = plan.created_by_run {
            matching_origins =
                matching_origins.filter(work_item_origin::Column::AgentRunId.eq(run_id));
        }
        if let Some(trigger_id) = plan.produced_by_trigger {
            matching_origins =
                matching_origins.filter(work_item_origin::Column::TriggerId.eq(trigger_id));
        }
        query = query.filter(work_item::Column::Id.in_subquery(matching_origins.into_query()));
    }
    if let Some(kind) = &plan.relationship_kind {
        let source_ids = related_item_ids_subquery(
            project_id,
            kind,
            work_item_relationship::Column::SourceWorkItemId,
        );
        let target_ids = related_item_ids_subquery(
            project_id,
            kind,
            work_item_relationship::Column::TargetWorkItemId,
        );
        query = query.filter(
            SeaCondition::any()
                .add(work_item::Column::Id.in_subquery(source_ids))
                .add(work_item::Column::Id.in_subquery(target_ids)),
        );
    }

    let batch_size = plan.scan_batch_size();
    let mut scan_cursor = plan.cursor.clone();
    let mut views = Vec::with_capacity(plan.limit + 1);
    while views.len() <= plan.limit {
        let mut batch_query = query.clone().limit(batch_size);
        if let Some(cursor) = &scan_cursor {
            batch_query = batch_query.filter(cursor.after_condition());
        }
        let models = batch_query
            .all(store.db().as_ref())
            .await
            .context("failed to search work items")?;
        let batch_is_full = models.len() == batch_size as usize;
        let Some(last_model) = models.last() else {
            break;
        };
        let next_scan_cursor = SearchCursor::from(last_model);
        let mut batch = work_item_views::models_to_views(store, project_id, models).await?;
        batch.retain(|item| plan.matches(item));
        views.extend(batch);
        scan_cursor = Some(next_scan_cursor);
        if !batch_is_full {
            break;
        }
    }

    let has_more = views.len() > plan.limit;
    views.truncate(plan.limit);
    let next_cursor = if has_more {
        views
            .last()
            .map(SearchCursor::from)
            .map(|cursor| cursor.encode())
    } else {
        None
    };
    Ok(WorkItemPage {
        items: views,
        next_cursor,
    })
}

fn related_item_ids_subquery(
    project_id: i64,
    kind: &str,
    item_column: work_item_relationship::Column,
) -> sea_orm::sea_query::SelectStatement {
    WorkItemRelationship::find()
        .select_only()
        .column(item_column)
        .filter(work_item_relationship::Column::ProjectId.eq(project_id))
        .filter(work_item_relationship::Column::Kind.eq(kind))
        .into_query()
}

struct WorkItemSearchPlan {
    limit: usize,
    states: BTreeSet<String>,
    labels: Option<ValidatedLabelCondition>,
    selector: Option<ValidatedLabelCondition>,
    text_pattern: Option<String>,
    finished: Option<bool>,
    created_by_run: Option<i64>,
    produced_by_trigger: Option<i64>,
    relationship_kind: Option<String>,
    updated_since: Option<String>,
    cursor: Option<SearchCursor>,
}

impl WorkItemSearchPlan {
    fn new(request: WorkItemSearchRequest) -> Result<Self> {
        let limit = request.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        if limit == 0 || limit > MAX_SEARCH_LIMIT {
            bail!("item search limit must be between 1 and {MAX_SEARCH_LIMIT}");
        }
        let states = request
            .states
            .into_iter()
            .map(workflow_labels::normalize_state_value)
            .collect::<Result<_>>()?;
        let labels = request
            .labels
            .as_ref()
            .map(ValidatedLabelCondition::new)
            .transpose()?;
        let selector = request
            .selector
            .as_ref()
            .map(ValidatedLabelCondition::new)
            .transpose()?;
        let relationship_kind = request
            .relationship_kind
            .map(|kind| -> Result<String> {
                let kind = kind.trim();
                if kind.is_empty() {
                    bail!("relationship kind cannot be empty");
                }
                Ok(kind.to_owned())
            })
            .transpose()?;
        let cursor = request
            .cursor
            .as_deref()
            .map(SearchCursor::parse)
            .transpose()?;

        Ok(Self {
            limit: limit as usize,
            states,
            labels,
            selector,
            text_pattern: request
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(text_search_pattern),
            finished: request.finished,
            created_by_run: request.created_by_run,
            produced_by_trigger: request.produced_by_trigger,
            relationship_kind,
            updated_since: request.updated_since,
            cursor,
        })
    }

    fn matches(&self, item: &WorkItemView) -> bool {
        (self.states.is_empty()
            || item
                .state
                .as_ref()
                .is_some_and(|state| self.states.contains(state)))
            && self
                .labels
                .as_ref()
                .is_none_or(|condition| condition.matches(&item.labels))
            && self
                .selector
                .as_ref()
                .is_none_or(|condition| condition.matches(&item.labels))
    }

    fn scan_batch_size(&self) -> u64 {
        if self.states.is_empty() && self.labels.is_none() && self.selector.is_none() {
            (self.limit + 1) as u64
        } else {
            SEARCH_SCAN_BATCH_SIZE
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SearchCursor {
    updated_at: String,
    item_id: i64,
}

impl SearchCursor {
    fn parse(cursor: &str) -> Result<Self> {
        let (item_id, updated_at) = cursor
            .split_once(':')
            .ok_or_else(|| report!("invalid item search cursor"))?;
        let item_id = item_id
            .parse::<i64>()
            .context("invalid item search cursor id")?;
        if updated_at.trim().is_empty() {
            bail!("invalid item search cursor timestamp");
        }
        Ok(Self {
            updated_at: updated_at.to_owned(),
            item_id,
        })
    }

    fn encode(&self) -> String {
        format!("{}:{}", self.item_id, self.updated_at)
    }

    fn after_condition(&self) -> SeaCondition {
        SeaCondition::any()
            .add(work_item::Column::UpdatedAt.lt(self.updated_at.clone()))
            .add(
                SeaCondition::all()
                    .add(work_item::Column::UpdatedAt.eq(self.updated_at.clone()))
                    .add(work_item::Column::Id.lt(self.item_id)),
            )
    }
}

impl From<&work_item::Model> for SearchCursor {
    fn from(item: &work_item::Model) -> Self {
        Self {
            updated_at: item.updated_at.clone(),
            item_id: item.id,
        }
    }
}

impl From<&WorkItemView> for SearchCursor {
    fn from(item: &WorkItemView) -> Self {
        Self {
            updated_at: item.updated_at.clone(),
            item_id: item.id,
        }
    }
}

fn text_search_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        entities::agent_run::AgentRunActiveModel,
        item_claims,
        items::{CreateWorkItem, create_item, create_item_with_attribution},
        projects::{self, CreateProject, create_project},
        relationships,
        request_attribution::RequestAttribution,
        storage::utc_now,
    };
    use crate::shared::view_models::CreateWorkItemLabelRequest;

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

    async fn create_test_item(store: &Store, title: impl Into<String>) -> WorkItemView {
        create_item(
            store,
            "demo",
            CreateWorkItem {
                title: title.into(),
                description: "Search test item".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
    }

    async fn insert_test_run(store: &Store) -> i64 {
        let project_id = projects::project_id(store, "demo").await.unwrap();
        let now = utc_now();
        AgentRunActiveModel {
            project_id: Set(project_id),
            tool_name: Set("codex".to_owned()),
            mutability: Set("read_only".to_owned()),
            status: Set("running".to_owned()),
            command: Set(String::new()),
            working_dir: Set(String::new()),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(store.db().as_ref())
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn pages_stably_across_filtered_batches() {
        let (_temp, store) = test_store().await;
        for index in 0..6 {
            let item = create_item(
                &store,
                "demo",
                CreateWorkItem {
                    title: format!("Search candidate {index}"),
                    description: if index == 4 {
                        "contains unique needle".to_owned()
                    } else {
                        "ordinary text".to_owned()
                    },
                    state: if index == 0 { "done" } else { "open" }.to_owned(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: vec![CreateWorkItemLabelRequest {
                        key: "bucket".to_owned(),
                        value: Some(if index % 2 == 0 { "keep" } else { "drop" }.to_owned()),
                    }],
                },
            )
            .await
            .unwrap();
            if index == 0 {
                item_claims::claim_specific_item(&store, "demo", item.id, "agent-search")
                    .await
                    .unwrap()
                    .unwrap();
                item_claims::finish_item(&store, "demo", item.id, "agent-search", "complete")
                    .await
                    .unwrap();
            }
        }
        let keep_selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "bucket".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("keep".to_owned()),
        })]);
        let first = search_items(
            &store,
            "demo",
            WorkItemSearchRequest {
                labels: Some(keep_selector.clone()),
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_that!(&(first.items.len())).is_equal_to(2);
        assert_that!(&(first.next_cursor.is_some())).is_true();

        let second = search_items(
            &store,
            "demo",
            WorkItemSearchRequest {
                labels: Some(keep_selector),
                limit: Some(2),
                cursor: first.next_cursor.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_that!(&(second.items.len())).is_equal_to(1);
        assert_that!(&(second.next_cursor.is_none())).is_true();
        assert_that!(
            &(first.items.iter().all(|first_item| {
                second
                    .items
                    .iter()
                    .all(|second_item| second_item.id != first_item.id)
            }))
        )
        .is_true();

        let text = search_items(
            &store,
            "demo",
            WorkItemSearchRequest {
                text: Some("needle".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_that!(&(text.items.len())).is_equal_to(1);
        assert_that!(&(text.items[0].title)).is_equal_to("Search candidate 4");

        let unfinished = search_items(
            &store,
            "demo",
            WorkItemSearchRequest {
                finished: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_that!(&(unfinished.items.len())).is_equal_to(5);
        assert_that!(
            &(search_items(
                &store,
                "demo",
                WorkItemSearchRequest {
                    limit: Some(201),
                    ..Default::default()
                }
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("between 1 and 200"))
        )
        .is_true();
    }

    #[tokio::test]
    async fn text_filter_treats_like_metacharacters_as_literals() {
        let (_temp, store) = test_store().await;
        create_test_item(&store, "literal % percent").await;
        create_test_item(&store, "literal _ underscore").await;
        create_test_item(&store, r"literal \ slash").await;
        create_test_item(&store, "ordinary item").await;

        for (text, expected_title) in [
            ("%", "literal % percent"),
            ("_", "literal _ underscore"),
            (r"\", r"literal \ slash"),
        ] {
            let page = search_items(
                &store,
                "demo",
                WorkItemSearchRequest {
                    text: Some(text.to_owned()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert_that!(&(page.items.len())).is_equal_to(1);
            assert_that!(&(page.items[0].title.as_str())).is_equal_to(expected_title);
        }
    }

    #[tokio::test]
    async fn origin_and_relationship_filters_are_combined_by_sql() {
        let (_temp, store) = test_store().await;
        let run_id = insert_test_run(&store).await;
        let mut run_attribution = RequestAttribution::default();
        run_attribution.agent_id = Some(format!("dispatch-run-{run_id}"));
        run_attribution.agent_run_id = Some(run_id);
        let matching = create_item_with_attribution(
            &store,
            "demo",
            CreateWorkItem {
                title: "Run-created and related".to_owned(),
                description: "Matches both relational filters".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            &run_attribution,
        )
        .await
        .unwrap();
        let run_only = create_item_with_attribution(
            &store,
            "demo",
            CreateWorkItem {
                title: "Run-created only".to_owned(),
                description: "Matches only the origin filter".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            &run_attribution,
        )
        .await
        .unwrap();
        let related = create_test_item(&store, "Related endpoint").await;
        relationships::create_relationship(
            &store,
            "demo",
            matching.id,
            related.id,
            "blocks".to_owned(),
        )
        .await
        .unwrap();

        let page = search_items(
            &store,
            "demo",
            WorkItemSearchRequest {
                created_by_run: Some(run_id),
                relationship_kind: Some(" blocks ".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_that!(&(page.items.len())).is_equal_to(1);
        assert_that!(&(page.items[0].id)).is_equal_to(matching.id);
        assert_that!(&(page.items[0].id != run_only.id)).is_true();
        assert_that!(&(page.items[0].id != related.id)).is_true();
    }

    #[test]
    fn cursor_round_trips_as_one_typed_value() {
        let cursor = SearchCursor {
            updated_at: "2026-09-03T12:34:56.123Z".to_owned(),
            item_id: 42,
        };

        assert_that!(&(SearchCursor::parse(&cursor.encode()).unwrap())).is_equal_to(cursor);
    }
}
