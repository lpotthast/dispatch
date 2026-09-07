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
        items::labels::conditions::ValidatedLabelCondition,
        items::labels::repository::conditions::{items_in_states, matching_items},
        items::labels::workflow as workflow_labels,
    },
    shared::view_models::{WorkItemPage, WorkItemSearchRequest},
};

const DEFAULT_SEARCH_LIMIT: u64 = 50;
const MAX_SEARCH_LIMIT: u64 = 200;

impl super::ItemRepository {
    pub(crate) async fn search_in(
        &self,
        transaction: &crate::backend::storage::Transaction,
        project_id: i64,
        request: WorkItemSearchRequest,
    ) -> Result<WorkItemPage> {
        let plan = WorkItemSearchPlan::new(request)?;
        let mut query = WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .order_by_desc(work_item::Column::UpdatedAt)
            .order_by_desc(work_item::Column::Id);

        if !plan.states.is_empty() {
            query = query.filter(items_in_states(
                project_id,
                plan.states.iter().map(String::as_str),
            ));
        }
        for condition in plan.labels.iter().chain(plan.selector.iter()) {
            query = query.filter(matching_items(project_id, condition));
        }
        if let Some(cursor) = &plan.cursor {
            query = query.filter(cursor.after_condition());
        }
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

        let mut models = query
            .limit((plan.limit + 1) as u64)
            .all(transaction.connection())
            .await
            .context("failed to search work items")?;
        let has_more = models.len() > plan.limit;
        models.truncate(plan.limit);
        let next_cursor = if has_more {
            models
                .last()
                .map(SearchCursor::from)
                .map(|cursor| cursor.encode())
        } else {
            None
        };
        let items =
            super::views::models_to_views(transaction.connection(), project_id, models).await?;
        Ok(WorkItemPage { items, next_cursor })
    }
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
        attribution::model::RequestAttribution, entities::agent_run::AgentRunActiveModel,
        items::CreateWorkItem, projects::CreateProject, storage::utc_now,
    };
    use crate::backend::{projects::repository::ProjectRepository, storage::Store};
    use crate::shared::view_models::{CreateWorkItemLabelRequest, STATE_LABEL_KEY, WorkItemView};

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open_with_max_connections(temp.path().join("dispatch.sqlite3"), 1)
            .await
            .unwrap();
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
        (temp, store)
    }

    async fn create_test_item(store: &Store, title: impl Into<String>) -> WorkItemView {
        crate::backend::items::creation::tests::service(
            store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: title.into(),
                description: "Search test item".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap()
    }

    async fn insert_test_run(store: &Store) -> i64 {
        let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
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

    fn label_condition(operator: Operator, value: ConditionClauseValue) -> Condition {
        Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: " priority ".to_owned(),
            operator,
            value,
        })])
    }

    #[tokio::test]
    async fn sql_label_filters_match_in_memory_semantics_across_scoped_pages() {
        let (temp, store) = test_store().await;
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "other".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();

        let mut fixtures = Vec::new();
        for project in ["demo", "other"] {
            for (index, value) in [
                None,
                Some(None),
                Some(Some("high")),
                Some(Some("low")),
                Some(Some("HIGH")),
                Some(Some("%_\\'日本語")),
            ]
            .into_iter()
            .enumerate()
            {
                let item = crate::backend::items::creation::tests::service(
                    &store,
                    crate::backend::events::UiEventBus::new(),
                )
                .create(
                    crate::backend::projects::ProjectReference::Name(project),
                    CreateWorkItem {
                        title: format!("Candidate {index}"),
                        description: "Label search fixture".to_owned(),
                        state: if index % 2 == 0 { "open" } else { "done" }.to_owned(),
                        agent_model_override: None,
                        agent_reasoning_effort_override: None,
                        initial_labels: value
                            .into_iter()
                            .map(|value| CreateWorkItemLabelRequest {
                                key: "priority".to_owned(),
                                value: value.map(ToOwned::to_owned),
                            })
                            .collect(),
                    },
                    Default::default(),
                )
                .await
                .unwrap();
                if project == "demo" {
                    fixtures.push(item);
                }
            }
        }
        // Equal timestamps force every page to use the ID tie-breaker.
        WorkItem::update_many()
            .col_expr(
                work_item::Column::UpdatedAt,
                Expr::value("2026-09-07T12:00:00Z"),
            )
            .exec(store.db().as_ref())
            .await
            .unwrap();
        fixtures.sort_by_key(|item| std::cmp::Reverse(item.id));

        let mut conditions = vec![Condition::All(vec![]), Condition::Any(vec![])];
        for operator in [Operator::Equal, Operator::NotEqual] {
            for value in [
                ConditionClauseValue::Bool(true),
                ConditionClauseValue::Bool(false),
                ConditionClauseValue::Json(serde_json::Value::Null),
                ConditionClauseValue::String("high".to_owned()),
                ConditionClauseValue::String("%_\\'日本語".to_owned()),
            ] {
                conditions.push(label_condition(operator, value));
            }
        }
        for values in [serde_json::json!([]), serde_json::json!(["high", "low"])] {
            conditions.push(label_condition(
                Operator::IsIn,
                ConditionClauseValue::Json(values),
            ));
        }
        conditions.push(Condition::All(vec![ConditionElement::Clause(
            ConditionClause {
                column_name: STATE_LABEL_KEY.to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::String("open".to_owned()),
            },
        )]));
        let leaves = conditions.clone();
        for left in &leaves {
            for right in &leaves {
                let elements = vec![
                    ConditionElement::Condition(Box::new(left.clone())),
                    ConditionElement::Condition(Box::new(right.clone())),
                ];
                conditions.push(Condition::All(elements.clone()));
                conditions.push(Condition::Any(elements));
            }
        }

        let service = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        );
        for condition in conditions {
            let validated = ValidatedLabelCondition::new(&condition).unwrap();
            let expected = fixtures
                .iter()
                .filter(|item| validated.matches(&item.labels))
                .map(|item| item.id)
                .collect::<Vec<_>>();
            let mut actual = Vec::new();
            let mut cursor = None;
            loop {
                let page = service
                    .search(
                        "demo",
                        WorkItemSearchRequest {
                            labels: Some(condition.clone()),
                            limit: Some(2),
                            cursor,
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap();
                actual.extend(page.items.iter().map(|item| item.id));
                assert_that!(&(actual.len() <= expected.len())).is_true();
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
                assert_that!(&page.items.len()).is_equal_to(2);
            }
            assert_that!(&actual).is_equal_to(expected);
        }

        let page = service
            .search(
                "demo",
                WorkItemSearchRequest {
                    states: vec![" open ".to_owned()],
                    labels: Some(label_condition(
                        Operator::NotEqual,
                        ConditionClauseValue::Json(serde_json::Value::Null),
                    )),
                    selector: Some(label_condition(
                        Operator::IsIn,
                        ConditionClauseValue::Json(serde_json::json!(["high", "low"])),
                    )),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_that!(
            &page
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>()
        )
        .is_equal_to(vec!["Candidate 2"]);
    }

    #[tokio::test]
    async fn search_enriches_only_the_returned_page() {
        let (_temp, store) = test_store().await;
        let lookahead = create_test_item(&store, "matching lookahead").await;
        let visible = create_test_item(&store, "matching visible").await;
        let excluded = create_test_item(&store, "excluded").await;
        crate::backend::items::labels::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .add(
            "demo",
            excluded.id,
            CreateWorkItemLabelRequest {
                key: "priority".to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();
        WorkItem::update_many()
            .col_expr(
                work_item::Column::AgentReasoningEffortOverride,
                Expr::value("invalid"),
            )
            .filter(work_item::Column::Id.is_in([lookahead.id, excluded.id]))
            .exec(store.db().as_ref())
            .await
            .unwrap();
        let service = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        );
        let request = WorkItemSearchRequest {
            labels: Some(label_condition(
                Operator::Equal,
                ConditionClauseValue::Bool(false),
            )),
            limit: Some(1),
            ..Default::default()
        };
        let page = service.search("demo", request.clone()).await.unwrap();
        assert_that!(&page.items.iter().map(|item| item.id).collect::<Vec<_>>())
            .is_equal_to(vec![visible.id]);
        assert_that!(&page.next_cursor).is_some();
        // Returned records still cross the validated persistence boundary.
        assert_that!(
            &service
                .search(
                    "demo",
                    WorkItemSearchRequest {
                        cursor: page.next_cursor,
                        ..request
                    }
                )
                .await
                .is_err()
        )
        .is_true();
    }

    #[tokio::test]
    async fn pages_stably_across_filtered_items() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store) = test_store().await;
        for index in 0..6 {
            let item = crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            )
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
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
                Default::default(),
            )
            .await
            .unwrap();
            if index == 0 {
                crate::backend::items::claims::tests::service(&store, event_bus.clone())
                    .claim_specific_item("demo", item.id, "agent-search")
                    .await
                    .unwrap()
                    .unwrap();
                crate::backend::items::claims::tests::service(&store, event_bus.clone())
                    .finish_item(
                        "demo",
                        item.id,
                        "agent-search",
                        "complete",
                        Default::default(),
                    )
                    .await
                    .unwrap();
            }
        }
        let keep_selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "bucket".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("keep".to_owned()),
        })]);
        let first = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .search(
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

        let second = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .search(
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

        let text = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .search(
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

        let unfinished = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .search(
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
            &(crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new()
            )
            .search(
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
            let page = crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            )
            .search(
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
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store) = test_store().await;
        let run_id = insert_test_run(&store).await;
        let mut run_attribution = RequestAttribution::default();
        run_attribution.agent_id = Some(format!("dispatch-run-{run_id}"));
        run_attribution.agent_run_id = Some(run_id);
        let matching = crate::backend::items::creation::tests::service(&store, event_bus.clone())
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
                CreateWorkItem {
                    title: "Run-created and related".to_owned(),
                    description: "Matches both relational filters".to_owned(),
                    state: "open".to_owned(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: Vec::new(),
                },
                crate::backend::attribution::model::AttributionInput {
                    agent_id: run_attribution.agent_id.clone(),
                    agent_run_id: run_attribution.agent_run_id,
                },
            )
            .await
            .unwrap();
        let run_only = crate::backend::items::creation::tests::service(&store, event_bus.clone())
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
                CreateWorkItem {
                    title: "Run-created only".to_owned(),
                    description: "Matches only the origin filter".to_owned(),
                    state: "open".to_owned(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: Vec::new(),
                },
                crate::backend::attribution::model::AttributionInput {
                    agent_id: run_attribution.agent_id.clone(),
                    agent_run_id: run_attribution.agent_run_id,
                },
            )
            .await
            .unwrap();
        let related = create_test_item(&store, "Related endpoint").await;
        crate::backend::relationships::tests::service(&store, event_bus.clone())
            .create(
                "demo",
                matching.id,
                related.id,
                "blocks".to_owned(),
                Default::default(),
            )
            .await
            .unwrap();

        let page = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .search(
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
