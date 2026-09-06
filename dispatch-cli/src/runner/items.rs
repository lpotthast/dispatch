use std::time::Duration;

use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use dispatch_types::{
    ClaimWorkItemRequest, CreateWorkItemLabelRequest, CreateWorkItemRequest, FinishWorkItemRequest,
    ProgressWorkItemRequest, ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest,
    UpdateWorkItemRequest, WorkItemSearchRequest,
};
use rootcause::Result;

use crate::{
    commands::{ItemCommand, ItemCreateArgs},
    context::ResolvedContext,
    output, render,
};

pub(super) async fn run(
    command: ItemCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        ItemCommand::List(args) => {
            let items = client.list_items(args.state.as_deref()).await?;
            output::write(format, &items, |output| {
                render::write_item_rows(output, &items)
            })
        }
        ItemCommand::Search(args) => {
            let labels = (!args.labels.is_empty()).then(|| {
                Condition::All(
                    args.labels
                        .into_iter()
                        .map(label_condition_element)
                        .collect(),
                )
            });
            let selector = args
                .selector_json
                .as_deref()
                .map(serde_json::from_str::<Condition>)
                .transpose()?;
            let request = WorkItemSearchRequest {
                states: args.states,
                labels,
                selector,
                text: args.text,
                finished: if args.finished {
                    Some(true)
                } else if args.unfinished {
                    Some(false)
                } else {
                    None
                },
                created_by_run: args.created_by_run,
                produced_by_trigger: args.produced_by_trigger,
                relationship_kind: args.relationship_kind,
                updated_since: args.updated_since,
                limit: args.limit,
                cursor: args.cursor,
            };
            let page = client.search_items(&request).await?;
            output::write(format, &page, |output| {
                render::write_item_rows(output, &page.items)?;
                if let Some(cursor) = &page.next_cursor {
                    writeln!(output, "Next cursor: {cursor}")?;
                }
                Ok(())
            })
        }
        ItemCommand::Show(args) => {
            let item = client.get_item(context.item_id(args.item_id)?).await?;
            output::write(format, &item, |output| {
                render::write_item_detail(output, &item)
            })
        }
        ItemCommand::Create(args) => {
            let request = create_work_item_request(args);
            let item = client.create_item(&request).await?;
            output::write(format, &item, |output| {
                writeln!(output, "Created item #{}: {}", item.id, item.title)
            })
        }
        ItemCommand::Update(args) => {
            let item_id = context.item_id(args.item_id)?;
            let request = UpdateWorkItemRequest {
                title: args.title,
                description: args.description,
                state: args.state,
                agent_model_override: optional_override(args.agent_model, args.clear_agent_model),
                agent_reasoning_effort_override: optional_override(
                    args.agent_reasoning_effort,
                    args.clear_agent_reasoning_effort,
                ),
                expect_version: args.expect_version,
            };
            let item = client.update_item(item_id, &request).await?;
            output::write(format, &item, |output| {
                writeln!(output, "Updated item #{} v{}", item.id, item.version)
            })
        }
        ItemCommand::Claim(args) => {
            let agent_id = context.agent_id()?;
            let claimed = client
                .claim_item(&ClaimWorkItemRequest {
                    agent_id: agent_id.to_owned(),
                    state: args.state.clone(),
                })
                .await?;
            if let Some(item) = claimed.item {
                output::write(format, &item, |output| {
                    writeln!(output, "Claimed item #{} for {}", item.id, agent_id)
                })
            } else if format == output::Format::Json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "claimed": false,
                        "project": client.project_name(),
                        "state": args.state,
                    }))?
                );
                Ok(())
            } else {
                println!("No matching item available");
                Ok(())
            }
        }
        ItemCommand::Progress(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let comment = client
                .progress_item(
                    item_id,
                    &ProgressWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        body: args.body,
                    },
                )
                .await?;
            output::write(format, &comment, |output| {
                writeln!(output, "Recorded progress comment #{}", comment.id)
            })
        }
        ItemCommand::Finish(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .finish_item(
                    item_id,
                    &FinishWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        report: args.report,
                    },
                )
                .await?;
            output::write(format, &item, |output| {
                writeln!(output, "Finished item #{} v{}", item.id, item.version)
            })
        }
        ItemCommand::Release(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .release_item(
                    item_id,
                    &ReleaseWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        comment: args.comment,
                    },
                )
                .await?;
            output::write(format, &item, |output| {
                writeln!(
                    output,
                    "Released item #{} back to {}",
                    item.id,
                    render::item_state_label(&item)
                )
            })
        }
        ItemCommand::RequestFeedback(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .request_item_feedback(
                    item_id,
                    &RequestFeedbackWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        body: args.body,
                    },
                )
                .await?;
            output::write(format, &item, |output| {
                writeln!(
                    output,
                    "Requested feedback for item #{} and restored state to {}",
                    item.id,
                    render::item_state_label(&item)
                )
            })
        }
        ItemCommand::Watch(args) => {
            let item_id = context.item_id(args.item_id)?;
            let mut last_version = args.since_version.unwrap_or(0);
            loop {
                let item = client.get_item(item_id).await?;
                if item.version > last_version {
                    last_version = item.version;
                    output::write(format, &item, |output| {
                        render::write_item_row(output, &item)
                    })?;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn label_condition_element(raw: String) -> ConditionElement {
    let (key, value) = match raw.split_once('=') {
        Some((key, value)) => (
            key.trim().to_owned(),
            ConditionClauseValue::String(value.trim().to_owned()),
        ),
        None => (raw.trim().to_owned(), ConditionClauseValue::Bool(true)),
    };
    ConditionElement::Clause(ConditionClause {
        column_name: key,
        operator: Operator::Equal,
        value,
    })
}

pub(super) fn optional_override<T>(value: Option<T>, clear: bool) -> Option<Option<T>> {
    if clear { Some(None) } else { value.map(Some) }
}

fn create_work_item_request(args: ItemCreateArgs) -> CreateWorkItemRequest {
    CreateWorkItemRequest {
        title: args.title,
        description: args.description,
        state: args.state,
        agent_model_override: args.agent_model,
        agent_reasoning_effort_override: args.agent_reasoning_effort,
        initial_labels: args
            .labels
            .into_iter()
            .map(create_work_item_label_request)
            .collect(),
    }
}

fn create_work_item_label_request(raw: String) -> CreateWorkItemLabelRequest {
    let (key, value) = match raw.split_once('=') {
        Some((key, value)) => (key.trim().to_owned(), trimmed_label_value(value)),
        None => (raw.trim().to_owned(), None),
    };
    CreateWorkItemLabelRequest { key, value }
}

fn trimmed_label_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use dispatch_types::AgentReasoningEffort;

    use super::*;

    fn create_args(labels: &[&str]) -> ItemCreateArgs {
        ItemCreateArgs {
            title: "Title".to_owned(),
            description: "Description".to_owned(),
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
            state: None,
            agent_model: None,
            agent_reasoning_effort: None,
        }
    }

    #[test]
    fn create_request_without_labels_has_empty_initial_labels() {
        let request = create_work_item_request(create_args(&[]));

        assert_that!(&(request.title)).is_equal_to("Title");
        assert_that!(&(request.description)).is_equal_to("Description");
        assert_that!(&(request.initial_labels.is_empty())).is_true();
    }

    #[test]
    fn create_request_preserves_state_and_agent_overrides() {
        let request = create_work_item_request(ItemCreateArgs {
            title: "Title".to_owned(),
            description: "Description".to_owned(),
            labels: Vec::new(),
            state: Some("idea".to_owned()),
            agent_model: Some("gpt-5-codex".to_owned()),
            agent_reasoning_effort: Some(AgentReasoningEffort::High),
        });

        assert_that!(&(request.state.as_deref())).is_equal_to(Some("idea"));
        assert_that!(&(request.agent_model_override.as_deref())).is_equal_to(Some("gpt-5-codex"));
        assert_that!(&(request.agent_reasoning_effort_override))
            .is_equal_to(Some(AgentReasoningEffort::High));
    }

    #[test]
    fn create_request_parses_initial_labels() {
        let request = create_work_item_request(create_args(&[
            "type=feature",
            "needs-verification",
            "token=a=b",
        ]));

        assert_that!(&(request.initial_labels.len())).is_equal_to(3);
        assert_that!(&(request.initial_labels[0].key)).is_equal_to("type");
        assert_that!(&(request.initial_labels[0].value.as_deref())).is_equal_to(Some("feature"));
        assert_that!(&(request.initial_labels[1].key)).is_equal_to("needs-verification");
        assert_that!(&(request.initial_labels[1].value.is_none())).is_true();
        assert_that!(&(request.initial_labels[2].key)).is_equal_to("token");
        assert_that!(&(request.initial_labels[2].value.as_deref())).is_equal_to(Some("a=b"));
    }

    #[test]
    fn create_request_trims_keys_and_values() {
        let request = create_work_item_request(create_args(&[
            " type = feature ",
            " needs-verification ",
            " empty = ",
        ]));

        assert_that!(&(request.initial_labels[0].key)).is_equal_to("type");
        assert_that!(&(request.initial_labels[0].value.as_deref())).is_equal_to(Some("feature"));
        assert_that!(&(request.initial_labels[1].key)).is_equal_to("needs-verification");
        assert_that!(&(request.initial_labels[1].value.is_none())).is_true();
        assert_that!(&(request.initial_labels[2].key)).is_equal_to("empty");
        assert_that!(&(request.initial_labels[2].value.is_none())).is_true();
    }

    #[test]
    fn create_request_leaves_validation_to_server() {
        let request = create_work_item_request(create_args(&[
            "dup=one",
            "dup=two",
            "=missing-key",
            "state=blocked",
        ]));

        assert_that!(&(request.initial_labels[0].key)).is_equal_to("dup");
        assert_that!(&(request.initial_labels[0].value.as_deref())).is_equal_to(Some("one"));
        assert_that!(&(request.initial_labels[1].key)).is_equal_to("dup");
        assert_that!(&(request.initial_labels[1].value.as_deref())).is_equal_to(Some("two"));
        assert_that!(&(request.initial_labels[2].key)).is_equal_to("");
        assert_that!(&(request.initial_labels[2].value.as_deref()))
            .is_equal_to(Some("missing-key"));
        assert_that!(&(request.initial_labels[3].key)).is_equal_to("state");
        assert_that!(&(request.initial_labels[3].value.as_deref())).is_equal_to(Some("blocked"));
    }
}
