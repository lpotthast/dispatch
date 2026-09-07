pub(crate) use dispatch_types::AutomationPersonalityInspectorView;
pub(crate) use std::{collections::HashMap, sync::Arc};

pub(crate) use crate::{
    frontend::{
        live_events::{event_scopes_named_project, reload_crudkit_on_live_event},
        rich_text::{normalize_tiptap_storage_value, rich_text_editor_html, rich_text_plain_text},
    },
    shared::view_models::{
        AgentReasoningEffort, CodexAgentModel, DEFAULT_STATE_LABEL, PersonalityView,
        ProjectLabelView, STATE_LABEL_KEY, SwimLaneItemOrder, UiEvent,
    },
};
pub(crate) use crudkit_leptos::crud_instance::CrudInstanceContext;
pub(crate) use crudkit_leptos::crudkit_core::{
    Value,
    condition::{Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator},
    id::{IdValue, SerializableId, SerializableIdEntry},
};
pub(crate) use crudkit_leptos::fields::{FieldRenderer, render_label};
pub(crate) use crudkit_leptos::{
    ReactiveField,
    crud_instance_config::{
        CrudBuiltinViewControls, CrudInstanceConfig, FieldRendererRegistry, Header, ItemsPerPage,
        ModelHandler, PageNr,
    },
    crud_view_registry::CrudViewRegistry,
    crudkit_web::{
        HeaderOptions, Label, reqwest_executor::NewClientPerRequestExecutor, view::CrudView,
    },
    prelude::*,
};
pub(crate) use indexmap::indexmap;
pub(crate) use leptonic::components::prelude::{Icon, TiptapToolbarEditor};
pub(crate) use leptonic::prelude::{TiptapContent, TiptapEditorHandle, icondata};
pub(crate) use leptos::prelude::*;

pub(crate) fn selected_entity_id_from_context(context: CrudInstanceContext) -> Option<i64> {
    context
        .navigation
        .current()
        .get()
        .subject
        .as_ref()
        .and_then(serializable_i64_id)
}

pub(crate) fn serializable_i64_id(id: &SerializableId) -> Option<i64> {
    id.entries().find_map(|entry| match &entry.value {
        IdValue::I64(value) => Some(*value),
        IdValue::I32(value) => Some(i64::from(*value)),
        IdValue::I16(value) => Some(i64::from(*value)),
        IdValue::I8(value) => Some(i64::from(*value)),
        _ => None,
    })
}

pub(crate) fn multiline_text_field_renderer<F: TypeErasedField>(
    placeholder: &'static str,
) -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, _field: F, field_mode, field_options, value, value_changed| {
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());

            match field_mode {
                FieldMode::Display => view! { {move || current.get()} }.into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    view! {
                        {render_label(field_options.label.clone())}
                        <textarea
                            class="crud-input-field"
                            prop:value=move || current.get()
                            disabled=disabled
                            placeholder=placeholder
                            on:input=move |event| {
                                value_changed.run(Ok(Value::String(event_target_value(&event))));
                            }
                        />
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(crate) fn rich_text_field_renderer<F: TypeErasedField>(
    label: &'static str,
) -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, field: F, field_mode, field_options, value, value_changed| {
            let field_name = field.name().into_owned();
            let field_name_attr = field_name.clone();
            let field_name_input = field_name.clone();
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());

            match field_mode {
                FieldMode::Display => {
                    view! { {move || rich_text_plain_text(&current.get())} }.into_any()
                }
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    let handle = TiptapEditorHandle::new();
                    let editor_id = format!("dispatch-tiptap-{}", uuid::Uuid::new_v4());
                    let initial_content =
                        TiptapContent::html(rich_text_editor_html(&current.get_untracked()));
                    view! {
                        {render_label(field_options.label.clone().or_else(|| Some(Label::new(label))))}
                        <div
                            class="rich-text-field crud-rich-text-field"
                            data-rich-text-field=field_name_attr
                            on:click=|event| {
                                // TipTap may render anchors/buttons; editor clicks should not activate page-level defaults.
                                event.prevent_default();
                            }
                        >
                            <input
                                type="hidden"
                                class="rich-text-input crud-input-field"
                                name=field_name_input
                                value=move || current.get()
                                on:input=move |event| {
                                    value_changed.run(Ok(Value::String(event_target_value(&event))));
                                }
                            />
                            <TiptapToolbarEditor
                                attr:class="crud-input-field"
                                id=editor_id
                                handle=handle
                                initial_content=initial_content
                                disabled=Signal::derive(move || disabled)
                                on_change=move || {
                                    let current_value = current.get_untracked();
                                    match handle.get_html() {
                                        Ok(content) => {
                                            let next_value = normalize_tiptap_storage_value(content);
                                            if next_value != current_value {
                                                value_changed.run(Ok(Value::String(next_value)));
                                            }
                                        }
                                        Err(report) => {
                                            tracing::error!(error = %report, "Could not read Tiptap editor content");
                                        }
                                    }
                                }
                                on_error=move |report| {
                                    tracing::error!(error = %report, "Tiptap editor operation failed");
                                }
                            />
                        </div>
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(crate) fn agent_model_field_renderer<F: TypeErasedField>(
    empty_label: Option<&'static str>,
    reasoning_field_name: Option<&'static str>,
) -> FieldRenderer<F> {
    FieldRenderer::new(
        move |signals, _field: F, field_mode, field_options, value, value_changed| {
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());
            let selected_effort = sibling_field_string_signal(signals, reasoning_field_name);

            match field_mode {
                FieldMode::Display => view! {
                    <span class=move || agent_model_class(&current.get())>
                        {move || agent_model_label(&current.get(), empty_label.unwrap_or("default"))}
                    </span>
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    let options = move || {
                        let selected_effort = selected_effort
                            .get()
                            .parse::<AgentReasoningEffort>()
                            .ok();
                        CodexAgentModel::all()
                            .iter()
                            .map(|model| {
                                let value = model.as_storage();
                                let incompatible = selected_effort
                                    .is_some_and(|effort| !model.supports_reasoning_effort(effort));
                                let label = if incompatible {
                                    format!("{value} (incompatible)")
                                } else {
                                    value.to_owned()
                                };
                                view! {
                                    <option value=value disabled=incompatible>{label}</option>
                                }
                            })
                            .collect::<Vec<_>>()
                    };
                    let stale_option = move || {
                        let current = current.get();
                        (!current.is_empty() && !CodexAgentModel::is_available_model(&current))
                            .then(|| {
                                let label = format!("{current} (unavailable)");
                                view! { <option value=current>{label}</option> }
                            })
                    };
                    let stale_warning = move || {
                        let current = current.get();
                        (!current.is_empty() && !CodexAgentModel::is_available_model(&current))
                            .then(|| {
                                view! {
                                    <p class="agent-model-warning">
                                        "Saved model is not available in this Codex install."
                                    </p>
                                }
                            })
                    };
                    let empty_option = empty_label.map(|empty_label| {
                        view! { <option value="">{empty_label}</option> }
                    });
                    view! {
                        {render_label(field_options.label.clone())}
                        <select
                            class="crud-input-field agent-model-select"
                            prop:value=move || current.get()
                            disabled=disabled
                            on:change=move |event| {
                                let selected = event_target_value(&event);
                                if selected.trim().is_empty() {
                                    value_changed.run(Ok(Value::Null));
                                } else {
                                    value_changed.run(Ok(Value::String(selected)));
                                }
                            }
                        >
                            {empty_option}
                            {stale_option}
                            {options}
                        </select>
                        {stale_warning}
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(crate) fn agent_model_label(value: &str, empty_label: &str) -> String {
    if value.is_empty() {
        empty_label.to_owned()
    } else if CodexAgentModel::is_available_model(value) {
        value.to_owned()
    } else {
        format!("{value} (unavailable)")
    }
}

pub(crate) fn agent_model_class(value: &str) -> &'static str {
    if value.is_empty() {
        "agent-model-value agent-model-default"
    } else if CodexAgentModel::is_available_model(value) {
        "agent-model-value"
    } else {
        "agent-model-value agent-model-stale"
    }
}

pub(crate) fn agent_reasoning_field_renderer<F: TypeErasedField>(
    empty_label: Option<&'static str>,
    model_field_name: Option<&'static str>,
) -> FieldRenderer<F> {
    FieldRenderer::new(
        move |signals, _field: F, field_mode, field_options, value, value_changed| {
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());
            let selected_model = sibling_field_string_signal(signals, model_field_name);

            match field_mode {
                FieldMode::Display => view! {
                    {move || {
                        let current = current.get();
                        if current.is_empty() {
                            empty_label.unwrap_or("default").to_owned()
                        } else {
                            current
                        }
                    }}
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    let options = move || {
                        let selected_model = selected_model.get().parse::<CodexAgentModel>().ok();
                        AgentReasoningEffort::all()
                            .into_iter()
                            .map(|effort| {
                                let value = effort.as_storage();
                                let incompatible = selected_model
                                    .is_some_and(|model| !model.supports_reasoning_effort(effort));
                                let label = if incompatible {
                                    format!("{value} (incompatible)")
                                } else {
                                    value.to_owned()
                                };
                                view! {
                                    <option value=value disabled=incompatible>{label}</option>
                                }
                            })
                            .collect::<Vec<_>>()
                    };
                    let empty_option = empty_label.map(|empty_label| {
                        view! { <option value="">{empty_label}</option> }
                    });
                    view! {
                        {render_label(field_options.label.clone())}
                        <select
                            class="crud-input-field agent-reasoning-select"
                            prop:value=move || current.get()
                            disabled=disabled
                            on:change=move |event| {
                                let selected = event_target_value(&event);
                                if selected.trim().is_empty() {
                                    value_changed.run(Ok(Value::Null));
                                } else {
                                    value_changed.run(Ok(Value::String(selected)));
                                }
                            }
                        >
                            {empty_option}
                            {options}
                        </select>
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(crate) fn sibling_field_string_signal<F: TypeErasedField>(
    signals: StoredValue<HashMap<F, ReactiveField>>,
    field_name: Option<&'static str>,
) -> Signal<String> {
    Signal::derive(move || {
        field_name
            .and_then(|field_name| {
                signals.with_value(|map| {
                    map.iter()
                        .find(|(field, _)| field.name().as_ref() == field_name)
                        .and_then(|(_, field)| field.value.get().as_string().cloned())
                })
            })
            .unwrap_or_default()
    })
}

pub(crate) fn select_field_renderer<F: TypeErasedField>(
    choices: &'static [(&'static str, &'static str)],
    nullable: bool,
) -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, _field: F, field_mode, field_options, value, value_changed| {
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());

            match field_mode {
                FieldMode::Display => view! {
                    {move || {
                        let current = current.get();
                        choices
                            .iter()
                            .find(|(value, _)| *value == current)
                            .map(|(_, label)| (*label).to_owned())
                            .unwrap_or(current)
                    }}
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    let options = choices
                        .iter()
                        .map(|(value, label)| {
                            view! {
                                <option value=*value>{*label}</option>
                            }
                        })
                        .collect::<Vec<_>>();
                    let empty_option = nullable.then(|| {
                        view! { <option value="">"default"</option> }
                    });
                    view! {
                        {render_label(field_options.label.clone())}
                        <select
                            class="crud-input-field"
                            prop:value=move || current.get()
                            disabled=disabled
                            on:change=move |event| {
                                let selected = event_target_value(&event);
                                if nullable && selected.trim().is_empty() {
                                    value_changed.run(Ok(Value::Null));
                                } else {
                                    value_changed.run(Ok(Value::String(selected)));
                                }
                            }
                        >
                            {empty_option}
                            {options}
                        </select>
                    }
                    .into_any()
                }
            }
        },
    )
}

pub(crate) fn value_to_optional_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I64(value) => Some(*value),
        Value::I32(value) => Some(i64::from(*value)),
        Value::I16(value) => Some(i64::from(*value)),
        Value::I8(value) => Some(i64::from(*value)),
        Value::U64(value) => i64::try_from(*value).ok(),
        Value::U32(value) => Some(i64::from(*value)),
        Value::U16(value) => Some(i64::from(*value)),
        Value::U8(value) => Some(i64::from(*value)),
        Value::String(value) => value.parse::<i64>().ok(),
        Value::Null | Value::Void(()) => None,
        _ => None,
    }
}

pub(crate) fn crudkit_i64_id(id: i64) -> SerializableId {
    SerializableId(vec![SerializableIdEntry {
        field_name: "id".to_owned(),
        value: IdValue::I64(id),
    }])
}

pub(crate) fn project_id_condition(project_id: i64) -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "project_id".to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(project_id),
    })])
}
