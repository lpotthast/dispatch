use super::*;
use crate::frontend::services::codex_service;
use crudkit_leptos::crud_action::ResourceActionInput;
use leptonic::components::prelude::ButtonColor;

#[component]
pub(crate) fn AgentToolsPanel(
    api_base_url: String,
    on_refreshed: Callback<()>,
) -> impl IntoView + 'static {
    view! {
        <section class="app-tools panel">
            <div class="panel-heading">
                <h2>"Codex app-server"</h2>
                <p class="muted">"Dispatch requires Codex app-server for automation."</p>
            </div>
            <div class="crudkit-agent-tools" data-crudkit-leptos="agent-tools">
                <AgentToolsCrudkitInstance api_base_url on_refreshed/>
            </div>
        </section>
    }
}

#[component]
fn AgentToolsCrudkitInstance(
    api_base_url: String,
    on_refreshed: Callback<()>,
) -> impl IntoView + 'static {
    let (context, set_context) = signal(None::<CrudInstanceContext>);
    reload_crudkit_on_live_event(context, |event| {
        matches!(
            event,
            UiEvent::AgentToolChanged { .. } | UiEvent::CodexStatusChanged { .. }
        )
    });
    let config = agent_tools_crudkit_config(api_base_url, on_refreshed);

    view! {
        <CrudInstance
            name="agent-tools"
            config
            on_context_created=Callback::new(move |context| set_context.set(Some(context)))
        />
    }
}

fn agent_tools_crudkit_config(
    api_base_url: String,
    on_refreshed: Callback<()>,
) -> CrudInstanceConfig {
    let service = codex_service();
    let check_codex = CrudAction {
        id: "check-codex",
        name: "Check Codex".to_owned(),
        icon: Some(icondata::BsArrowRepeat),
        button_color: ButtonColor::Secondary,
        action: Callback::new(move |input: ResourceActionInput| {
            let service = service.clone();
            leptos::task::spawn_local(async move {
                let outcome = match service.discover_agent_tools().await {
                    Ok(()) => {
                        on_refreshed.run(());
                        Ok(CrudActionAftermath {
                            show_toast: None,
                            reload_data: true,
                        })
                    }
                    Err(_) => Err(CrudActionAftermath {
                        show_toast: None,
                        reload_data: false,
                    }),
                };
                input.and_then.run(outcome);
            });
        }),
        view: None,
    };

    CrudInstanceConfig {
        api_base_url,
        initial_view: CrudView::table(),
        list_columns: vec![
            Header::showing(
                ReadAgentToolField::Id,
                HeaderOptions {
                    display_name: "#".into(),
                    min_width: true,
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadAgentToolField::ToolName,
                HeaderOptions {
                    display_name: "Tool".into(),
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadAgentToolField::ExecutablePath,
                HeaderOptions {
                    display_name: "Configured binary".into(),
                    ..Default::default()
                },
            ),
            Header::showing(
                ReadAgentToolField::DiscoveredPath,
                HeaderOptions {
                    display_name: "Discovered binary".into(),
                    ..Default::default()
                },
            ),
        ],
        create_elements: CreateElements::Custom(vec![Elem::Enclosing(Enclosing::Card(Group {
            layout: Layout::default(),
            children: vec![
                Elem::create_field(
                    CreateAgentToolField::ToolName,
                    FieldOptions {
                        label: Some(Label::new("Tool")),
                        ..Default::default()
                    },
                ),
                Elem::create_field(
                    CreateAgentToolField::ExecutablePath,
                    FieldOptions {
                        label: Some(Label::new("Codex binary path")),
                        ..Default::default()
                    },
                ),
            ],
        }))]),
        elements: vec![Elem::Enclosing(Enclosing::Card(Group {
            layout: Layout::default(),
            children: vec![
                Elem::field(
                    AgentTool::Id,
                    FieldOptions {
                        disabled: true,
                        label: Some(Label::new("ID")),
                        ..Default::default()
                    },
                ),
                Elem::field(
                    AgentToolField::ExecutablePath,
                    FieldOptions {
                        label: Some(Label::new("Executable path")),
                        ..Default::default()
                    },
                ),
            ],
        }))],
        order_by: indexmap! {
            ReadAgentTool::Id.into() => Order::Asc,
        },
        items_per_page: ItemsPerPage::default(),
        page_nr: PageNr::first(),
        base_condition: None,
        resource_name: CrudAgentToolResource::resource_name().to_owned(),
        reqwest_executor: Arc::new(NewClientPerRequestExecutor),
        model_handler: ModelHandler::new::<CreateAgentTool, ReadAgentTool, AgentTool>(),
        actions: vec![check_codex],
        entity_actions: vec![],
        builtin_view_controls: CrudBuiltinViewControls::default(),
        view_registry: CrudViewRegistry::default(),
        read_field_renderer: FieldRendererRegistry::builder().build(),
        create_field_renderer: FieldRendererRegistry::builder().build(),
        update_field_renderer: FieldRendererRegistry::builder().build(),
    }
}
