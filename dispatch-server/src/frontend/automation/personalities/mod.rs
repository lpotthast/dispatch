use crate::frontend::automation::personalities::form::*;
pub(crate) mod form;
use crate::frontend::automation::service::automation_service;
use crate::frontend::crudkit::*;

#[component]
pub(crate) fn PersonalitiesPanel(
    api_base_url: String,
    project: String,
    project_id: i64,
) -> impl IntoView + 'static {
    let (context, set_context) = signal(None::<CrudInstanceContext>);
    let project_for_events = project.clone();
    reload_crudkit_on_live_event(context, move |event| {
        event_scopes_named_project(event, Some(project_for_events.as_str()))
            && matches!(event, UiEvent::AutomationChanged { .. })
    });
    let (selected_personality_id, set_selected_personality_id) = signal(None::<i64>);
    Effect::new(move |_| {
        if let Some(personality_id) = context.get().and_then(selected_personality_id_from_context) {
            set_selected_personality_id.set(Some(personality_id));
        }
    });
    let inspector = view! { <PersonalityInspector project selected_personality_id/> };
    let config = personalities_crudkit_config(api_base_url, project_id);

    view! {
        <section id="personalities" class="personalities-admin panel">
            <div class="panel-heading">
                <h2>"Personalities"</h2>
            </div>
            <div class="crudkit-personalities" data-crudkit-leptos="personalities">
                <CrudInstance
                    name="personalities"
                    config
                    on_context_created=Callback::new(move |context| set_context.set(Some(context)))
                />
            </div>
            {inspector}
        </section>
    }
}

#[component]
fn PersonalityInspector(
    project: String,
    selected_personality_id: ReadSignal<Option<i64>>,
) -> impl IntoView {
    let store = crate::frontend::automation::store::automation_store();
    let (error, set_error) = signal(None::<String>);
    let refresh = RwSignal::new(0_u64);
    let project_for_load = project.clone();
    let loaded = LocalResource::new(move || {
        refresh.track();
        let id = selected_personality_id.get();
        let project = project_for_load.clone();
        let store = store.clone();
        async move {
            let result = match id {
                Some(id) => store
                    .load_personality_inspector(project, id)
                    .await
                    .map(Some),
                None => Ok(None),
            };
            (id, result)
        }
    });
    let inspector = Signal::derive(move || {
        loaded.get().and_then(|(id, result)| {
            (id == selected_personality_id.get())
                .then(|| result.ok().flatten())
                .flatten()
        })
    });
    Effect::new(move |_| {
        set_error.set(loaded.get().and_then(|(id, result)| {
            (id == selected_personality_id.get_untracked())
                .then(|| result.err().map(|error| error.to_string()))
                .flatten()
        }));
    });

    view! {
        {move || selected_personality_id.get().map(|personality_id| {
            let content = match inspector.get() {
                Some(inspector) => view! {
                    <PersonalityInspectorContent
                        inspector
                        project=project.clone()
                        refresh
                        set_error
                    />
                }.into_any(),
                None => view! {
                    <p class="muted">
                        {error.get().unwrap_or_else(|| "Loading personality revision history…".to_owned())}
                    </p>
                }
                .into_any(),
            };
            view! {
                <section
                    class="automation-personality-inspector"
                    data-testid="automation-personality-inspector"
                    data-personality-id=personality_id.to_string()
                >
                    <h3>"Selected personality history"</h3>
                    {content}
                </section>
            }
        })}
    }
    .into_any()
}

#[component]
fn PersonalityInspectorContent(
    inspector: AutomationPersonalityInspectorView,
    project: String,
    refresh: RwSignal<u64>,
    set_error: WriteSignal<Option<String>>,
) -> impl IntoView {
    let service = automation_service();
    let detach_service = service.clone();
    let restore_service = service;
    let personality = inspector.personality;
    let personality_id = personality.id;
    let current_revision_id = personality.current_revision_id;
    let managed = personality.managed_bundle_key.is_some();
    let managed_badge = personality.managed_bundle_key.clone().map(|bundle| {
        view! { <span class="automation-managed-badge">{"Managed by "}{bundle}</span> }
    });
    let project_for_detach = project.clone();
    let detach_action = Action::new(move |_: &()| {
        let project = project_for_detach.clone();
        let service = detach_service.clone();
        async move {
            match service.detach_personality(project, personality_id).await {
                Ok(_) => refresh.update(|value| *value += 1),
                Err(error) => set_error.set(Some(format!("Detach failed: {error}"))),
            }
        }
    });
    let detach = managed.then_some({
        move |_| {
            if !detach_action.pending().get_untracked() {
                detach_action.dispatch(());
            }
        }
    });

    let revisions = inspector
        .revisions
        .into_iter()
        .map(|revision| {
            let revision_id = revision.id;
            let current = current_revision_id == Some(revision_id);
            let project = project.clone();
            let service = restore_service.clone();
            let restore_action = Action::new(move |_: &()| {
                let project = project.clone();
                let service = service.clone();
                async move {
                    match service
                        .restore_personality_revision(project, personality_id, revision_id)
                        .await
                    {
                        Ok(_) => refresh.update(|value| *value += 1),
                        Err(error) => set_error.set(Some(format!("Restore failed: {error}"))),
                    }
                }

    });
    let restore_pending = restore_action.pending();
    let restore = move |_| {
        restore_action.dispatch(());
    };
            view! {
                <li>
                    <div class="automation-revision-heading">
                        <span>{format!("Revision {} · {:?}", revision.revision_number, revision.operation)}</span>
                        <code>{revision.sha256}</code>
                        <button type="button" disabled=move || current || managed || restore_pending.get() on:click=restore>
                            {if current { "Current" } else { "Restore" }}
                        </button>
                    </div>
                    <details>
                        <summary>"Personality text snapshot"</summary>
                        <strong>{revision.name}</strong>
                        <pre>{revision.personality_description}</pre>
                    </details>
                </li>
            }
        })
        .collect::<Vec<_>>();

    view! {
        <div class="automation-personality-heading">
            <strong>{personality.name}</strong>
            {managed_badge}
            {detach.map(|detach| view! {
                <button type="button" on:click=detach>"Detach from bundle"</button>
            })}
        </div>
        <ul class="automation-revision-list">{revisions}</ul>
    }
    .into_any()
}

fn personalities_crudkit_config(api_base_url: String, project_id: i64) -> CrudInstanceConfig {
    CrudInstanceConfig {
        api_base_url,
        initial_view: CrudView::table(),
        list_columns: vec![
            Header::showing(
                ReadPersonalityField::Name,
                HeaderOptions {
                    display_name: "Name".into(),
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadPersonalityField::ManagedBundleKey,
                HeaderOptions {
                    display_name: "Managed bundle".into(),
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadPersonalityField::CurrentRevisionId,
                HeaderOptions {
                    display_name: "Revision".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadPersonalityField::UpdatedAt,
                HeaderOptions {
                    display_name: "Updated".into(),
                    ..Default::default()
                },
            ),
        ],
        create_elements: CreateElements::Custom(vec![Elem::Enclosing(Enclosing::None(Group {
            layout: Layout::default(),
            children: vec![
                Elem::create_field(
                    CreatePersonalityField::Name,
                    FieldOptions {
                        label: Some(Label::new("Name")),
                        ..Default::default()
                    },
                ),
                Elem::create_field(
                    CreatePersonalityField::PersonalityDescription,
                    FieldOptions {
                        label: Some(Label::new("Personality description")),
                        ..Default::default()
                    },
                ),
            ],
        }))]),
        elements: vec![Elem::Enclosing(Enclosing::None(Group {
            layout: Layout::default(),
            children: vec![
                Elem::field(
                    Personality::Id,
                    FieldOptions {
                        disabled: true,
                        label: Some(Label::new("ID")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    PersonalityField::Name,
                    FieldOptions {
                        label: Some(Label::new("Name")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    PersonalityField::PersonalityDescription,
                    FieldOptions {
                        label: Some(Label::new("Personality description")),
                        ..Default::default()
                    },
                ),
            ],
        }))],
        order_by: indexmap! {
            ReadPersonality::Name.into() => Order::Asc,
        },
        items_per_page: ItemsPerPage::default(),
        page_nr: PageNr::first(),
        base_condition: Some(project_id_condition(project_id)),
        resource_name: CrudPersonalityResource::resource_name().to_owned(),
        reqwest_executor: Arc::new(NewClientPerRequestExecutor),
        model_handler: personality_model_handler(project_id),
        actions: vec![],
        entity_actions: vec![],
        builtin_view_controls: CrudBuiltinViewControls::default(),
        view_registry: CrudViewRegistry::default(),
        read_field_renderer: FieldRendererRegistry::builder().build(),
        create_field_renderer: FieldRendererRegistry::builder()
            .register(
                CreatePersonalityField::PersonalityDescription,
                multiline_text_field_renderer::<DynCreateField>("Personality description"),
            )
            .build(),
        update_field_renderer: FieldRendererRegistry::builder()
            .register(
                PersonalityField::PersonalityDescription,
                multiline_text_field_renderer::<DynUpdateField>("Personality description"),
            )
            .build(),
    }
}

fn personality_model_handler(project_id: i64) -> ModelHandler {
    let mut handler = ModelHandler::new::<CreatePersonality, ReadPersonality, Personality>();
    handler.get_default_create_model = Callback::new(move |()| {
        DynCreateModel::from(CreatePersonality {
            project_id,
            ..Default::default()
        })
    });
    handler
}

pub(crate) fn selected_personality_id_from_context(context: CrudInstanceContext) -> Option<i64> {
    selected_entity_id_from_context(context)
}
