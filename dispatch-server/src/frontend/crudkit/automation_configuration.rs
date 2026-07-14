use super::{swim_lane_filter::condition_editor_view, *};
use dispatch_types::{
    AgentReasoningEffort, AutomationOutcomeSet, AutomationPostconditions,
    CreateWorkItemLabelRequest, CreatedItemAssertion, ExpectedDisposition, LabelAssertion,
    LabelAssertionKind, ProduceDeduplication, ProducedWorkSpec, WorkItemEventType,
    WorkspaceAssertion,
};

#[derive(Clone, Copy)]
struct JsonEditorContext {
    disabled: bool,
    value_signal: RwSignal<Value>,
    value_changed: Callback<Result<Value, std::sync::Arc<dyn std::error::Error>>>,
}

#[derive(Debug)]
struct AutomationConfigurationEditorError(String);

impl std::fmt::Display for AutomationConfigurationEditorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AutomationConfigurationEditorError {}

pub(super) fn produced_work_field_renderer<F: TypeErasedField>() -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, _field: F, field_mode, field_options, value, value_changed| {
            let current = Signal::derive(move || json_string_from_value(&value.value.get()));
            match field_mode {
                FieldMode::Display => view! {
                    <span>{move || produced_work_summary(&current.get())}</span>
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let context = JsonEditorContext {
                        disabled: field_mode != FieldMode::Editable || field_options.disabled,
                        value_signal: value.value,
                        value_changed,
                    };
                    let raw_mode = RwSignal::new(false);
                    view! {
                        {render_label(field_options.label.clone())}
                        <div class="automation-config-editor produced-work-editor" data-produced-work-editor="structured">
                            {move || {
                                let raw = current.get();
                                match parse_produced_work(&raw) {
                                    Ok(spec) if !raw_mode.get() => {
                                        produced_work_structured_view(spec, raw_mode, context)
                                    }
                                    Ok(_) => json_raw_view(raw, None, raw_mode, context),
                                    Err(error) => json_raw_view(raw, Some(error), raw_mode, context),
                                }
                            }}
                        </div>
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(super) fn postconditions_field_renderer<F: TypeErasedField>() -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, _field: F, field_mode, field_options, value, value_changed| {
            let current = Signal::derive(move || json_string_from_value(&value.value.get()));
            match field_mode {
                FieldMode::Display => view! {
                    <span>{move || postconditions_summary(&current.get())}</span>
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let context = JsonEditorContext {
                        disabled: field_mode != FieldMode::Editable || field_options.disabled,
                        value_signal: value.value,
                        value_changed,
                    };
                    let raw_mode = RwSignal::new(false);
                    view! {
                        {render_label(field_options.label.clone())}
                        <div class="automation-config-editor postconditions-editor" data-postconditions-editor="structured">
                            {move || {
                                let raw = current.get();
                                match parse_postconditions(&raw) {
                                    Ok(postconditions) if !raw_mode.get() => {
                                        postconditions_structured_view(postconditions, raw_mode, context)
                                    }
                                    Ok(_) => json_raw_view(raw, None, raw_mode, context),
                                    Err(error) => json_raw_view(raw, Some(error), raw_mode, context),
                                }
                            }}
                        </div>
                    }
                    .into_any()
                }
            }
        },
    )
}

fn produced_work_structured_view(
    spec: ProducedWorkSpec,
    raw_mode: RwSignal<bool>,
    context: JsonEditorContext,
) -> AnyView {
    let title = spec.title.clone().unwrap_or_default();
    let state = spec.state.clone();
    let model = spec.agent_model_override.clone().unwrap_or_default();
    let effort = spec
        .agent_reasoning_effort_override
        .map(|effort| effort.as_storage().to_owned())
        .unwrap_or_default();
    let (deduplication, deduplication_key) = match &spec.deduplication {
        ProduceDeduplication::Always => ("always", String::new()),
        ProduceDeduplication::WhileUnfinishedForTrigger => {
            ("while_unfinished_for_trigger", String::new())
        }
        ProduceDeduplication::WhileUnfinishedForKey { key } => {
            ("while_unfinished_for_key", key.clone())
        }
    };
    let labels = spec
        .initial_labels
        .into_iter()
        .enumerate()
        .map(|(index, label)| produced_label_view(index, label, context))
        .collect::<Vec<_>>();

    view! {
        <div class="automation-config-structured produced-work-structured">
            <div class="automation-config-grid">
                <label>
                    <span>"Produced item title"</span>
                    <input
                        type="text"
                        class="crud-input-field"
                        data-produced-title="true"
                        prop:value=title
                        placeholder="Automation name"
                        disabled=context.disabled
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            update_produced_work(context, move |spec| {
                                spec.title = nonempty(value);
                            });
                        }
                    />
                </label>
                <label>
                    <span>"Initial state"</span>
                    <input
                        type="text"
                        class="crud-input-field"
                        data-produced-state="true"
                        prop:value=state
                        placeholder="open"
                        disabled=context.disabled
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            update_produced_work(context, move |spec| spec.state = value);
                        }
                    />
                </label>
                <label>
                    <span>"Work-item model override"</span>
                    <input
                        type="text"
                        class="crud-input-field"
                        data-produced-model="true"
                        prop:value=model
                        placeholder="Project default"
                        disabled=context.disabled
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            update_produced_work(context, move |spec| {
                                spec.agent_model_override = nonempty(value);
                            });
                        }
                    />
                </label>
                <label>
                    <span>"Work-item reasoning effort"</span>
                    <select
                        class="crud-input-field"
                        data-produced-reasoning-effort="true"
                        prop:value=effort
                        disabled=context.disabled
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            update_produced_work(context, move |spec| {
                                spec.agent_reasoning_effort_override = parse_effort(&value);
                            });
                        }
                    >
                        <option value="">"Project/rule default"</option>
                        {AgentReasoningEffort::all().into_iter().map(|effort| {
                            let value = effort.as_storage();
                            view! { <option value=value>{value}</option> }
                        }).collect::<Vec<_>>()}
                    </select>
                </label>
                <label>
                    <span>"Deduplication"</span>
                    <select
                        class="crud-input-field"
                        data-produced-deduplication="true"
                        prop:value=deduplication
                        disabled=context.disabled
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            update_produced_work(context, move |spec| {
                                spec.deduplication = match value.as_str() {
                                    "while_unfinished_for_trigger" => {
                                        ProduceDeduplication::WhileUnfinishedForTrigger
                                    }
                                    "while_unfinished_for_key" => {
                                        ProduceDeduplication::WhileUnfinishedForKey {
                                            key: String::new(),
                                        }
                                    }
                                    _ => ProduceDeduplication::Always,
                                };
                            });
                        }
                    >
                        <option value="always">"Always create"</option>
                        <option value="while_unfinished_for_trigger">"Reuse unfinished item for this rule"</option>
                        <option value="while_unfinished_for_key">"Reuse unfinished item for project key"</option>
                    </select>
                </label>
                {matches!(spec.deduplication, ProduceDeduplication::WhileUnfinishedForKey { .. }).then(|| view! {
                    <label>
                        <span>"Project deduplication key"</span>
                        <input
                            type="text"
                            class="crud-input-field"
                            data-produced-deduplication-key="true"
                            prop:value=deduplication_key
                            disabled=context.disabled
                            on:input=move |event| {
                                let value = event_target_value(&event);
                                update_produced_work(context, move |spec| {
                                    spec.deduplication = ProduceDeduplication::WhileUnfinishedForKey {
                                        key: value,
                                    };
                                });
                            }
                        />
                    </label>
                })}
            </div>
            <fieldset class="automation-config-list produced-labels">
                <legend>"Initial labels"</legend>
                {labels}
                <button
                    type="button"
                    class="secondary automation-config-add"
                    data-produced-add-label="true"
                    disabled=context.disabled
                    on:click=move |_| update_produced_work(context, |spec| {
                        spec.initial_labels.push(CreateWorkItemLabelRequest {
                            key: String::new(),
                            value: None,
                        });
                    })
                >
                    <Icon icon=icondata::BsPlusLg/>
                    <span>"Add initial label"</span>
                </button>
            </fieldset>
            {raw_toggle(raw_mode, context.disabled)}
        </div>
    }
    .into_any()
}

fn produced_label_view(
    index: usize,
    label: CreateWorkItemLabelRequest,
    context: JsonEditorContext,
) -> AnyView {
    let key = label.key;
    let value = label.value.unwrap_or_default();
    view! {
        <div class="automation-config-row" data-produced-label=index.to_string()>
            <input
                type="text"
                class="crud-input-field"
                aria-label="Initial label key"
                prop:value=key
                placeholder="label key"
                disabled=context.disabled
                on:input=move |event| {
                    let value = event_target_value(&event);
                    update_produced_work(context, move |spec| {
                        if let Some(label) = spec.initial_labels.get_mut(index) {
                            label.key = value;
                        }
                    });
                }
            />
            <input
                type="text"
                class="crud-input-field"
                aria-label="Initial label value"
                prop:value=value
                placeholder="optional value"
                disabled=context.disabled
                on:input=move |event| {
                    let value = event_target_value(&event);
                    update_produced_work(context, move |spec| {
                        if let Some(label) = spec.initial_labels.get_mut(index) {
                            label.value = nonempty(value);
                        }
                    });
                }
            />
            <button
                type="button"
                class="secondary icon-button"
                title="Remove initial label"
                aria-label="Remove initial label"
                disabled=context.disabled
                on:click=move |_| update_produced_work(context, |spec| {
                    if index < spec.initial_labels.len() {
                        spec.initial_labels.remove(index);
                    }
                })
            >
                <Icon icon=icondata::BsTrash/>
            </button>
        </div>
    }
    .into_any()
}

fn postconditions_structured_view(
    postconditions: AutomationPostconditions,
    raw_mode: RwSignal<bool>,
    context: JsonEditorContext,
) -> AnyView {
    let empty = postconditions.any_of.is_empty().then(|| {
        view! {
            <p class="automation-config-empty">
                "No semantic postconditions are configured. Add an alternative outcome to enforce them."
            </p>
        }
    });
    let outcomes = postconditions
        .any_of
        .into_iter()
        .enumerate()
        .map(|(index, outcome)| postcondition_outcome_view(index, outcome, context))
        .collect::<Vec<_>>();

    view! {
        <div class="automation-config-structured postconditions-structured">
            <p class="automation-config-help">
                "Every assertion inside an outcome must pass. The run passes when any alternative outcome passes."
            </p>
            {empty}
            <div class="postcondition-outcomes">{outcomes}</div>
            <div class="automation-config-footer">
                <button
                    type="button"
                    class="secondary automation-config-add"
                    data-postconditions-add-outcome="true"
                    disabled=context.disabled
                    on:click=move |_| update_postconditions(context, |postconditions| {
                        postconditions.any_of.push(AutomationOutcomeSet::default());
                    })
                >
                    <Icon icon=icondata::BsPlusLg/>
                    <span>"Add alternative outcome"</span>
                </button>
                {raw_toggle(raw_mode, context.disabled)}
            </div>
        </div>
    }
    .into_any()
}

fn postcondition_outcome_view(
    outcome_index: usize,
    outcome: AutomationOutcomeSet,
    context: JsonEditorContext,
) -> AnyView {
    let disposition = outcome
        .disposition
        .map(disposition_storage)
        .unwrap_or_default();
    let workspace = outcome
        .workspace_changes
        .map(workspace_storage)
        .unwrap_or_default();
    let events = work_item_event_types()
        .into_iter()
        .map(|event_type| {
            let checked = outcome.attributed_events.contains(&event_type);
            view! {
                <label class="automation-config-checkbox">
                    <input
                        type="checkbox"
                        value=event_type.as_storage()
                        prop:checked=checked
                        disabled=context.disabled
                        on:change=move |event| {
                            let checked = event_target_checked(&event);
                            update_postconditions(context, move |postconditions| {
                                let Some(outcome) = postconditions.any_of.get_mut(outcome_index) else {
                                    return;
                                };
                                outcome.attributed_events.retain(|candidate| *candidate != event_type);
                                if checked {
                                    outcome.attributed_events.push(event_type);
                                }
                            });
                        }
                    />
                    <span>{event_type.as_storage()}</span>
                </label>
            }
        })
        .collect::<Vec<_>>();
    let labels = outcome
        .labels
        .into_iter()
        .enumerate()
        .map(|(label_index, label)| {
            postcondition_label_view(outcome_index, label_index, label, context)
        })
        .collect::<Vec<_>>();
    let created_items = outcome
        .created_items
        .map(|created| created_items_view(outcome_index, created, context));

    view! {
        <fieldset class="postcondition-outcome" data-postcondition-outcome=outcome_index.to_string()>
            <legend>{format!("Alternative {}", outcome_index + 1)}</legend>
            <div class="automation-config-grid">
                <label>
                    <span>"Expected disposition"</span>
                    <select
                        class="crud-input-field"
                        data-postcondition-disposition="true"
                        prop:value=disposition
                        disabled=context.disabled
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            update_postconditions(context, move |postconditions| {
                                if let Some(outcome) = postconditions.any_of.get_mut(outcome_index) {
                                    outcome.disposition = parse_disposition(&value);
                                }
                            });
                        }
                    >
                        <option value="">"No disposition requirement"</option>
                        <option value="finished">"Finished"</option>
                        <option value="released">"Released"</option>
                        <option value="feedback_requested">"Feedback requested"</option>
                        <option value="successful_nonterminal">"Successful nonterminal"</option>
                    </select>
                </label>
                <label>
                    <span>"Workspace changes"</span>
                    <select
                        class="crud-input-field"
                        data-postcondition-workspace="true"
                        prop:value=workspace
                        disabled=context.disabled
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            update_postconditions(context, move |postconditions| {
                                if let Some(outcome) = postconditions.any_of.get_mut(outcome_index) {
                                    outcome.workspace_changes = parse_workspace(&value);
                                }
                            });
                        }
                    >
                        <option value="">"No workspace requirement"</option>
                        <option value="any">"Any"</option>
                        <option value="none">"None"</option>
                        <option value="required">"Required"</option>
                    </select>
                </label>
            </div>
            <fieldset class="postcondition-events">
                <legend>"Required run-attributed events"</legend>
                <div class="automation-config-checkboxes">{events}</div>
            </fieldset>
            <fieldset class="automation-config-list postcondition-labels">
                <legend>"Label assertions"</legend>
                {labels}
                <button
                    type="button"
                    class="secondary automation-config-add"
                    data-postcondition-add-label="true"
                    disabled=context.disabled
                    on:click=move |_| update_postconditions(context, |postconditions| {
                        if let Some(outcome) = postconditions.any_of.get_mut(outcome_index) {
                            outcome.labels.push(LabelAssertion {
                                assertion: LabelAssertionKind::Present,
                                key: String::new(),
                                value: None,
                            });
                        }
                    })
                >
                    <Icon icon=icondata::BsPlusLg/>
                    <span>"Add label assertion"</span>
                </button>
            </fieldset>
            {created_items.unwrap_or_else(|| view! {
                <button
                    type="button"
                    class="secondary automation-config-add"
                    data-postcondition-add-created-items="true"
                    disabled=context.disabled
                    on:click=move |_| update_postconditions(context, |postconditions| {
                        if let Some(outcome) = postconditions.any_of.get_mut(outcome_index) {
                            outcome.created_items = Some(CreatedItemAssertion::default());
                        }
                    })
                >
                    <Icon icon=icondata::BsPlusLg/>
                    <span>"Require created items"</span>
                </button>
            }.into_any())}
            <button
                type="button"
                class="secondary postcondition-remove-outcome"
                data-postcondition-remove-outcome="true"
                disabled=context.disabled
                on:click=move |_| update_postconditions(context, |postconditions| {
                    if outcome_index < postconditions.any_of.len() {
                        postconditions.any_of.remove(outcome_index);
                    }
                })
            >
                <Icon icon=icondata::BsTrash/>
                <span>"Remove alternative"</span>
            </button>
        </fieldset>
    }
    .into_any()
}

fn postcondition_label_view(
    outcome_index: usize,
    label_index: usize,
    label: LabelAssertion,
    context: JsonEditorContext,
) -> AnyView {
    let assertion = label_assertion_storage(label.assertion);
    let key = label.key;
    let value = label.value.unwrap_or_default();
    view! {
        <div class="automation-config-row" data-postcondition-label=label_index.to_string()>
            <select
                class="crud-input-field"
                aria-label="Label assertion"
                prop:value=assertion
                disabled=context.disabled
                on:change=move |event| {
                    let value = event_target_value(&event);
                    update_postconditions(context, move |postconditions| {
                        if let Some(label) = postconditions.any_of
                            .get_mut(outcome_index)
                            .and_then(|outcome| outcome.labels.get_mut(label_index))
                        {
                            label.assertion = parse_label_assertion(&value);
                        }
                    });
                }
            >
                <option value="added">"Added by this run"</option>
                <option value="removed">"Removed by this run"</option>
                <option value="present">"Present after run"</option>
                <option value="absent">"Absent after run"</option>
            </select>
            <input
                type="text"
                class="crud-input-field"
                aria-label="Postcondition label key"
                prop:value=key
                placeholder="label key"
                disabled=context.disabled
                on:input=move |event| {
                    let value = event_target_value(&event);
                    update_postconditions(context, move |postconditions| {
                        if let Some(label) = postconditions.any_of
                            .get_mut(outcome_index)
                            .and_then(|outcome| outcome.labels.get_mut(label_index))
                        {
                            label.key = value;
                        }
                    });
                }
            />
            <input
                type="text"
                class="crud-input-field"
                aria-label="Postcondition label value"
                prop:value=value
                placeholder="optional value"
                disabled=context.disabled
                on:input=move |event| {
                    let value = event_target_value(&event);
                    update_postconditions(context, move |postconditions| {
                        if let Some(label) = postconditions.any_of
                            .get_mut(outcome_index)
                            .and_then(|outcome| outcome.labels.get_mut(label_index))
                        {
                            label.value = nonempty(value);
                        }
                    });
                }
            />
            <button
                type="button"
                class="secondary icon-button"
                title="Remove label assertion"
                aria-label="Remove label assertion"
                disabled=context.disabled
                on:click=move |_| update_postconditions(context, |postconditions| {
                    if let Some(outcome) = postconditions.any_of.get_mut(outcome_index)
                        && label_index < outcome.labels.len()
                    {
                        outcome.labels.remove(label_index);
                    }
                })
            >
                <Icon icon=icondata::BsTrash/>
            </button>
        </div>
    }
    .into_any()
}

fn created_items_view(
    outcome_index: usize,
    created: CreatedItemAssertion,
    context: JsonEditorContext,
) -> AnyView {
    let count = optional_number(created.count);
    let at_least = optional_number(created.at_least);
    let at_most = optional_number(created.at_most);
    let selector = created
        .selector
        .as_ref()
        .and_then(|selector| serde_json::to_string(selector).ok())
        .unwrap_or_default();
    let selector_changed = Callback::new(move |raw: String| {
        let selector = if raw.trim().is_empty() {
            None
        } else {
            serde_json::from_str(&raw).ok()
        };
        update_postconditions(context, move |postconditions| {
            if let Some(created) = postconditions
                .any_of
                .get_mut(outcome_index)
                .and_then(|outcome| outcome.created_items.as_mut())
            {
                created.selector = selector;
            }
        });
    });

    view! {
        <fieldset class="postcondition-created-items" data-postcondition-created-items="true">
            <legend>"Created items attributed to this run"</legend>
            <div class="automation-config-grid three-columns">
                {created_number_field("Exact count", "count", count, outcome_index, context)}
                {created_number_field("At least", "at_least", at_least, outcome_index, context)}
                {created_number_field("At most", "at_most", at_most, outcome_index, context)}
            </div>
            <div class="postcondition-created-selector">
                <span>"Created-item selector"</span>
                {condition_editor_view(selector, context.disabled, selector_changed)}
            </div>
            <button
                type="button"
                class="secondary"
                data-postcondition-remove-created-items="true"
                disabled=context.disabled
                on:click=move |_| update_postconditions(context, |postconditions| {
                    if let Some(outcome) = postconditions.any_of.get_mut(outcome_index) {
                        outcome.created_items = None;
                    }
                })
            >
                <Icon icon=icondata::BsTrash/>
                <span>"Remove created-item requirement"</span>
            </button>
        </fieldset>
    }
    .into_any()
}

fn created_number_field(
    label: &'static str,
    field: &'static str,
    value: String,
    outcome_index: usize,
    context: JsonEditorContext,
) -> AnyView {
    view! {
        <label>
            <span>{label}</span>
            <input
                type="number"
                min="0"
                class="crud-input-field"
                data-created-count=field
                prop:value=value
                disabled=context.disabled
                on:input=move |event| {
                    let value = event_target_value(&event).trim().parse::<u64>().ok();
                    update_postconditions(context, move |postconditions| {
                        let Some(created) = postconditions.any_of
                            .get_mut(outcome_index)
                            .and_then(|outcome| outcome.created_items.as_mut())
                        else {
                            return;
                        };
                        match field {
                            "count" => created.count = value,
                            "at_least" => created.at_least = value,
                            "at_most" => created.at_most = value,
                            _ => {}
                        }
                    });
                }
            />
        </label>
    }
    .into_any()
}

fn raw_toggle(raw_mode: RwSignal<bool>, disabled: bool) -> AnyView {
    view! {
        <button
            type="button"
            class="secondary automation-config-raw-toggle"
            disabled=disabled
            on:click=move |_| raw_mode.set(true)
        >
            "Edit raw JSON"
        </button>
    }
    .into_any()
}

fn json_raw_view(
    raw: String,
    error: Option<String>,
    raw_mode: RwSignal<bool>,
    context: JsonEditorContext,
) -> AnyView {
    let can_use_structured = error.is_none();
    let error_view = error.map(|error| {
        view! {
            <p class="automation-config-error" role="alert">
                {"Cannot show this configuration as structured controls: "}{error}
            </p>
        }
    });
    view! {
        <div class="automation-config-raw-panel">
            {error_view}
            <textarea
                class="crud-input-field automation-config-raw"
                data-automation-config-raw="true"
                prop:value=raw
                disabled=context.disabled
                on:input=move |event| {
                    raw_mode.set(true);
                    let raw = event_target_value(&event);
                    context.value_changed.run(Ok(if raw.trim().is_empty() {
                        Value::Null
                    } else {
                        Value::String(raw)
                    }));
                }
            />
            <button
                type="button"
                class="secondary automation-config-structured-toggle"
                disabled=move || context.disabled || !can_use_structured
                on:click=move |_| raw_mode.set(false)
            >
                "Use structured editor"
            </button>
        </div>
    }
    .into_any()
}

fn update_produced_work(context: JsonEditorContext, update: impl FnOnce(&mut ProducedWorkSpec)) {
    let raw = json_string_from_value(&context.value_signal.get_untracked());
    let mut spec = parse_produced_work(&raw).unwrap_or_else(|_| default_produced_work());
    update(&mut spec);
    emit_json(context, &spec);
}

fn update_postconditions(
    context: JsonEditorContext,
    update: impl FnOnce(&mut AutomationPostconditions),
) {
    let raw = json_string_from_value(&context.value_signal.get_untracked());
    let mut postconditions = parse_postconditions(&raw).unwrap_or_default();
    update(&mut postconditions);
    if postconditions.any_of.is_empty() {
        context.value_changed.run(Ok(Value::Null));
    } else {
        emit_json(context, &postconditions);
    }
}

fn emit_json(context: JsonEditorContext, value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(json) => context.value_changed.run(Ok(Value::String(json))),
        Err(error) => context.value_changed.run(Err(std::sync::Arc::new(
            AutomationConfigurationEditorError(error.to_string()),
        ))),
    }
}

fn parse_produced_work(raw: &str) -> Result<ProducedWorkSpec, String> {
    if raw.trim().is_empty() {
        Ok(default_produced_work())
    } else {
        serde_json::from_str(raw).map_err(|error| error.to_string())
    }
}

fn parse_postconditions(raw: &str) -> Result<AutomationPostconditions, String> {
    if raw.trim().is_empty() {
        Ok(AutomationPostconditions::default())
    } else {
        serde_json::from_str(raw).map_err(|error| error.to_string())
    }
}

fn default_produced_work() -> ProducedWorkSpec {
    ProducedWorkSpec {
        title: None,
        state: "open".to_owned(),
        initial_labels: Vec::new(),
        agent_model_override: None,
        agent_reasoning_effort_override: None,
        deduplication: ProduceDeduplication::Always,
    }
}

fn produced_work_summary(raw: &str) -> String {
    parse_produced_work(raw)
        .map(|spec| {
            format!(
                "state {}, {} initial label(s), {}",
                spec.state,
                spec.initial_labels.len(),
                produced_deduplication_summary(&spec.deduplication)
            )
        })
        .unwrap_or_else(|_| raw.to_owned())
}

fn postconditions_summary(raw: &str) -> String {
    parse_postconditions(raw)
        .map(|postconditions| {
            if postconditions.any_of.is_empty() {
                "Not configured".to_owned()
            } else {
                format!("{} alternative outcome(s)", postconditions.any_of.len())
            }
        })
        .unwrap_or_else(|_| raw.to_owned())
}

fn produced_deduplication_summary(deduplication: &ProduceDeduplication) -> String {
    match deduplication {
        ProduceDeduplication::Always => "always create".to_owned(),
        ProduceDeduplication::WhileUnfinishedForTrigger => {
            "reuse unfinished item for rule".to_owned()
        }
        ProduceDeduplication::WhileUnfinishedForKey { key } => {
            format!("reuse unfinished item for key {key}")
        }
    }
}

fn json_string_from_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Json(value) => value.to_string(),
        Value::Null | Value::Void(()) => String::new(),
        other => format!("{other:?}"),
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn optional_number(value: Option<u64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn parse_effort(value: &str) -> Option<AgentReasoningEffort> {
    value.parse().ok()
}

fn parse_disposition(value: &str) -> Option<ExpectedDisposition> {
    match value {
        "finished" => Some(ExpectedDisposition::Finished),
        "released" => Some(ExpectedDisposition::Released),
        "feedback_requested" => Some(ExpectedDisposition::FeedbackRequested),
        "successful_nonterminal" => Some(ExpectedDisposition::SuccessfulNonterminal),
        _ => None,
    }
}

fn disposition_storage(disposition: ExpectedDisposition) -> String {
    match disposition {
        ExpectedDisposition::Finished => "finished",
        ExpectedDisposition::Released => "released",
        ExpectedDisposition::FeedbackRequested => "feedback_requested",
        ExpectedDisposition::SuccessfulNonterminal => "successful_nonterminal",
    }
    .to_owned()
}

fn parse_workspace(value: &str) -> Option<WorkspaceAssertion> {
    match value {
        "any" => Some(WorkspaceAssertion::Any),
        "none" => Some(WorkspaceAssertion::None),
        "required" => Some(WorkspaceAssertion::Required),
        _ => None,
    }
}

fn workspace_storage(assertion: WorkspaceAssertion) -> String {
    match assertion {
        WorkspaceAssertion::Any => "any",
        WorkspaceAssertion::None => "none",
        WorkspaceAssertion::Required => "required",
    }
    .to_owned()
}

fn parse_label_assertion(value: &str) -> LabelAssertionKind {
    match value {
        "added" => LabelAssertionKind::Added,
        "removed" => LabelAssertionKind::Removed,
        "absent" => LabelAssertionKind::Absent,
        _ => LabelAssertionKind::Present,
    }
}

fn label_assertion_storage(assertion: LabelAssertionKind) -> String {
    match assertion {
        LabelAssertionKind::Added => "added",
        LabelAssertionKind::Removed => "removed",
        LabelAssertionKind::Present => "present",
        LabelAssertionKind::Absent => "absent",
    }
    .to_owned()
}

fn work_item_event_types() -> [WorkItemEventType; 18] {
    [
        WorkItemEventType::SystemPromptChanged,
        WorkItemEventType::MemoryChanged,
        WorkItemEventType::ItemCreated,
        WorkItemEventType::ItemUpdated,
        WorkItemEventType::ItemMoved,
        WorkItemEventType::ItemDeleted,
        WorkItemEventType::ItemClaimed,
        WorkItemEventType::ProgressAdded,
        WorkItemEventType::ItemFinished,
        WorkItemEventType::ItemReleased,
        WorkItemEventType::FeedbackRequested,
        WorkItemEventType::CommentAdded,
        WorkItemEventType::LabelAdded,
        WorkItemEventType::LabelUpdated,
        WorkItemEventType::LabelDeleted,
        WorkItemEventType::RelationshipCreated,
        WorkItemEventType::RelationshipUpdated,
        WorkItemEventType::RelationshipDeleted,
    ]
}
