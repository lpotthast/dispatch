use crate::frontend::items::label_keys::form::*;
pub(crate) mod form;
use crate::frontend::crudkit::*;

#[component]
pub(crate) fn LabelKeysPanel(
    api_base_url: String,
    project: String,
    project_id: i64,
) -> impl IntoView + 'static {
    view! {
        <section id="label-keys" class="label-keys-admin panel">
            <div class="panel-heading">
                <div>
                    <h2>"Label keys"</h2>
                    <p class="muted">
                        "Used labels are discovered automatically. Persistent and built-in keys remain available with zero usage."
                    </p>
                </div>
            </div>
            <div class="crudkit-label-keys" data-crudkit-leptos="label-keys">
                <LabelKeysCrudkitInstance api_base_url project project_id/>
            </div>
        </section>
    }
}

#[component]
fn LabelKeysCrudkitInstance(
    api_base_url: String,
    project: String,
    project_id: i64,
) -> impl IntoView + 'static {
    let (context, set_context) = signal(None::<CrudInstanceContext>);
    reload_crudkit_on_live_event(context, move |event| {
        event_scopes_named_project(event, Some(project.as_str()))
            && matches!(
                event,
                UiEvent::LabelKeyChanged { .. } | UiEvent::WorkItemChanged { .. }
            )
    });
    let config = label_keys_crudkit_config(api_base_url, project_id);

    view! {
        <CrudInstance
            name="label-keys"
            config
            on_context_created=Callback::new(move |context| set_context.set(Some(context)))
        />
    }
}

fn label_keys_crudkit_config(api_base_url: String, project_id: i64) -> CrudInstanceConfig {
    CrudInstanceConfig {
        api_base_url,
        initial_view: CrudView::table(),
        list_columns: vec![
            Header::showing(
                ReadLabelKeyField::Key,
                HeaderOptions {
                    display_name: "Key".into(),
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadLabelKeyField::UsageCount,
                HeaderOptions {
                    display_name: "Usage".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadLabelKeyField::AccentColor,
                HeaderOptions {
                    display_name: "Accent color".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadLabelKeyField::Persistent,
                HeaderOptions {
                    display_name: "Persistent".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadLabelKeyField::BuiltIn,
                HeaderOptions {
                    display_name: "Built in".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadLabelKeyField::LastUsedAt,
                HeaderOptions {
                    display_name: "Last used".into(),
                    ..Default::default()
                },
            ),
        ],
        create_elements: CreateElements::Custom(vec![Elem::Enclosing(Enclosing::None(Group {
            layout: Layout::default(),
            children: vec![
                Elem::create_field(
                    CreateLabelKeyField::Key,
                    FieldOptions {
                        label: Some(Label::new("Key")),
                        ..Default::default()
                    },
                ),
                Elem::create_field(
                    CreateLabelKeyField::AccentColor,
                    FieldOptions {
                        label: Some(Label::new("Accent color")),
                        ..Default::default()
                    },
                ),
            ],
        }))]),
        elements: vec![Elem::Enclosing(Enclosing::None(Group {
            layout: Layout::default(),
            children: vec![
                Elem::field(
                    LabelKey::Id,
                    FieldOptions {
                        disabled: true,
                        label: Some(Label::new("ID")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    LabelKeyField::Key,
                    FieldOptions {
                        disabled: true,
                        label: Some(Label::new("Key")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    LabelKeyField::AccentColor,
                    FieldOptions {
                        label: Some(Label::new("Accent color")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    LabelKeyField::Persistent,
                    FieldOptions {
                        label: Some(Label::new("Persistent")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    LabelKeyField::BuiltIn,
                    FieldOptions {
                        disabled: true,
                        label: Some(Label::new("Built in")),
                        ..Default::default()
                    },
                ),
            ],
        }))],
        order_by: indexmap! {
            ReadLabelKey::Key.into() => Order::Asc,
            ReadLabelKey::Id.into() => Order::Asc,
        },
        items_per_page: ItemsPerPage::default(),
        page_nr: PageNr::first(),
        base_condition: Some(project_id_condition(project_id)),
        resource_name: CrudLabelKeyResource::resource_name().to_owned(),
        reqwest_executor: Arc::new(NewClientPerRequestExecutor),
        model_handler: label_key_model_handler(project_id),
        actions: vec![],
        entity_actions: vec![],
        builtin_view_controls: CrudBuiltinViewControls {
            show_save: false,
            show_save_and_back: true,
            show_save_and_new: false,
            show_delete: false,
            show_return: true,
            create_save_target: CrudCreateSaveTarget::Return,
            create_actions_placement: CrudActionsPlacement::Inline,
        },
        view_registry: CrudViewRegistry::default(),
        read_field_renderer: FieldRendererRegistry::builder()
            .register(ReadLabelKeyField::AccentColor, accent_color_renderer())
            .build(),
        create_field_renderer: FieldRendererRegistry::builder()
            .register(CreateLabelKeyField::AccentColor, accent_color_renderer())
            .build(),
        update_field_renderer: FieldRendererRegistry::builder()
            .register(LabelKeyField::AccentColor, accent_color_renderer())
            .register(LabelKeyField::Persistent, persistent_renderer())
            .build(),
    }
}

fn label_key_model_handler(project_id: i64) -> ModelHandler {
    let mut handler = ModelHandler::new::<CreateLabelKey, ReadLabelKey, LabelKey>();
    handler.get_default_create_model = Callback::new(move |()| {
        DynCreateModel::from(CreateLabelKey {
            project_id,
            persistent: true,
            ..Default::default()
        })
    });
    handler
}

fn accent_color_renderer<F: TypeErasedField>() -> FieldRenderer<F> {
    FieldRenderer::new(
        move |_signals, _field: F, field_mode, field_options, value, value_changed| {
            let current =
                Signal::derive(move || value.value.get().as_string().cloned().unwrap_or_default());
            let picker_value = Signal::derive(move || {
                let current = current.get();
                if current.is_empty() {
                    "#5b5bd6".to_owned()
                } else {
                    current
                }
            });

            match field_mode {
                FieldMode::Display => view! {
                    <span class="label-accent-value">
                        <span
                            class="label-accent-swatch"
                            style=move || {
                                let current = current.get();
                                (!current.is_empty()).then(|| format!("--label-accent: {current}"))
                            }
                        ></span>
                        {move || {
                            let current = current.get();
                            if current.is_empty() { "—".to_owned() } else { current }
                        }}
                    </span>
                }
                .into_any(),
                FieldMode::Readable | FieldMode::Editable => {
                    let disabled = field_mode != FieldMode::Editable || field_options.disabled;
                    view! {
                        {render_label(field_options.label.clone())}
                        <div class="label-accent-editor">
                            <input
                                type="color"
                                class="label-accent-picker"
                                prop:value=move || picker_value.get()
                                disabled=disabled
                                on:input=move |event| {
                                    value_changed.run(Ok(Value::String(event_target_value(&event))));
                                }
                            />
                            <input
                                type="text"
                                class="crud-input-field label-accent-text"
                                prop:value=move || current.get()
                                disabled=disabled
                                placeholder="#rrggbb"
                                on:input=move |event| {
                                    let next = event_target_value(&event);
                                    if next.trim().is_empty() {
                                        value_changed.run(Ok(Value::Null));
                                    } else {
                                        value_changed.run(Ok(Value::String(next)));
                                    }
                                }
                            />
                            <button
                                type="button"
                                class="label-accent-clear"
                                disabled=disabled
                                on:click=move |_| value_changed.run(Ok(Value::Null))
                            >
                                "Clear"
                            </button>
                        </div>
                    }
                    .into_any()
                }
            }
        },
    )
}

fn persistent_renderer<F: TypeErasedField>() -> FieldRenderer<F> {
    FieldRenderer::new(
        move |signals, _field: F, field_mode, field_options, value, value_changed| {
            let current = Signal::derive(move || value.value.get().as_bool().unwrap_or_default());
            let built_in = Signal::derive(move || {
                signals.with_value(|map| {
                    map.iter()
                        .find(|(field, _)| field.name().as_ref() == "built_in")
                        .and_then(|(_, field)| field.value.get().as_bool())
                        .unwrap_or(false)
                })
            });
            let disabled = Signal::derive(move || {
                field_mode != FieldMode::Editable || field_options.disabled || built_in.get()
            });

            match field_mode {
                FieldMode::Display => view! { {move || current.get()} }.into_any(),
                FieldMode::Readable | FieldMode::Editable => view! {
                    {render_label(field_options.label.clone())}
                    <label class="label-persistent-toggle">
                        <input
                            type="checkbox"
                            prop:checked=move || current.get()
                            disabled=move || disabled.get()
                            on:change=move |event| {
                                value_changed.run(Ok(Value::Bool(event_target_checked(&event))));
                            }
                        />
                        <span>
                            {move || {
                                if built_in.get() {
                                    "Built-in keys are always persistent"
                                } else {
                                    "Keep this key when usage reaches zero"
                                }
                            }}
                        </span>
                    </label>
                }
                .into_any(),
            }
        },
    )
}
