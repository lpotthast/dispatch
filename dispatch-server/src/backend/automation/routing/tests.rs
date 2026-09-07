pub(crate) fn service(
    store: &crate::backend::storage::Store,
) -> std::sync::Arc<super::service::RoutingService> {
    use crate::backend::{
        automation::rules::repository::RuleRepository, items::repository::ItemRepository,
        projects::repository::ProjectRepository, storage::TransactionManager,
    };
    use std::sync::Arc;
    Arc::new(super::service::RoutingService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(RuleRepository),
        Arc::new(ItemRepository),
        Arc::new(super::repository::RoutingRepository),
        crate::backend::runs::admission::tests::service(store),
    ))
}

#[tokio::test]
async fn unsaved_rule_preview_counts_scoped_matches_and_loads_only_compact_examples() {
    use crate::backend::{
        comments::tests::application,
        entities::work_item::{self, WorkItem, WorkItemActiveModel},
        items::CreateWorkItem,
        projects::{CreateProject, ProjectReference},
    };
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use dispatch_types::{
        AUTOMATION_BLOCKED_LABEL_KEY, AutomationRuleInput, CreateWorkItemLabelRequest,
        FEEDBACK_REQUESTED_LABEL_KEY, RoutingExplainRequest,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, sea_query::Expr};

    let (temp, app, _, _) = application().await;
    app.state
        .projects
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
    let mut matching_ids = Vec::new();
    for index in 0..16 {
        let project = if index == 14 { "other" } else { "demo" };
        let mut labels = vec![CreateWorkItemLabelRequest {
            key: "route".to_owned(),
            value: Some(if index == 15 { "other" } else { "selected" }.to_owned()),
        }];
        if let Some(key) = match index {
            12 => Some(AUTOMATION_BLOCKED_LABEL_KEY),
            13 => Some(FEEDBACK_REQUESTED_LABEL_KEY),
            _ => None,
        } {
            labels.push(CreateWorkItemLabelRequest {
                key: key.to_owned(),
                value: None,
            });
        }
        let item = crate::backend::items::creation::tests::service(
            &app.state.store,
            app.state.events.clone(),
        )
        .create(
            ProjectReference::Name(project),
            CreateWorkItem {
                title: format!("Preview {index}"),
                description: "Routing preview fixture".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: labels,
            },
            Default::default(),
        )
        .await
        .unwrap();
        WorkItemActiveModel {
            id: Set(item.id),
            updated_at: Set("2026-09-07T12:00:00Z".to_owned()),
            claimed_by: Set((index == 11).then(|| "existing-agent".to_owned())),
            finished_at: Set((index == 10).then(|| "2026-09-07T12:00:00Z".to_owned())),
            ..Default::default()
        }
        .update(app.state.store.db().as_ref())
        .await
        .unwrap();
        if index < 12 {
            matching_ids.push(item.id);
        }
    }
    // These fields are irrelevant to a selector preview, including for its examples.
    WorkItem::update_many()
        .col_expr(
            work_item::Column::AgentReasoningEffortOverride,
            Expr::value("invalid"),
        )
        .exec(app.state.store.db().as_ref())
        .await
        .unwrap();
    let selector = Condition::Any(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: "route".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("selected".to_owned()),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: "missing".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(true),
        }),
    ]);
    let mut rule: AutomationRuleInput = serde_json::from_value(serde_json::json!({
        "name": "Preview", "enabled": true, "activation": "work_item",
        "effect": "consume_work", "schedule": "@every 15s"
    }))
    .unwrap();
    rule.selector = Some(selector);
    let preview = app
        .state
        .routing
        .explain(
            "demo",
            RoutingExplainRequest {
                rule: Some(rule.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_that!(&preview.matching_item_count).is_equal_to(Some(12));
    assert_that!(
        &preview
            .example_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()
    )
    .is_equal_to(matching_ids.into_iter().rev().take(10).collect::<Vec<_>>());
    assert_that!(
        &preview
            .example_items
            .iter()
            .all(|item| item.state.as_deref() == Some("open"))
    )
    .is_true();
    assert_that!(&preview.rules[0].selector_matches).is_true();
    assert_that!(
        &preview.rules[0]
            .clause_results
            .iter()
            .map(|clause| clause.matched)
            .collect::<Vec<_>>()
    )
    .is_equal_to(vec![true, false]);
    assert_that!(
        &app.state
            .routing
            .explain(
                "other",
                RoutingExplainRequest {
                    rule: Some(rule.clone()),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .matching_item_count
    )
    .is_equal_to(Some(1));

    rule.selector = Some(Condition::Any(vec![]));
    let empty = app
        .state
        .routing
        .explain(
            "demo",
            RoutingExplainRequest {
                rule: Some(rule),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_that!(&empty.matching_item_count).is_equal_to(Some(0));
    assert_that!(&empty.example_items).is_empty();
    assert_that!(&empty.rules[0].selector_matches).is_false();
    assert_that!(&empty.rules[0].clause_results).is_empty();
}
