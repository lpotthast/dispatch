use crate::{
    frontend::{
        components::{
            ActivePage, cached_query, encode_path, selected_project_signal, top_bar,
            trigger_runs_panel,
        },
        crudkit::{
            AutomationTableKind, PersonalitiesPanel, automation_triggers_crudkit_instance,
            selected_trigger_id_from_context,
        },
        services::{
            apply_bundle_yaml, automation_service, detach_automation_rule, diff_bundle_yaml,
            explain_automation_route, export_bundle_yaml, list_installed_bundles,
            load_automation_rule_inspector, project_cache, remove_installed_bundle,
            restore_automation_rule_revision, validate_bundle_yaml,
        },
    },
    shared::view_models::{
        AutomationEvaluationView, AutomationRevisionView, AutomationTriggerView,
        CodexAppServerStatusView, PersonalityView, ProjectView, RevisionAnalyticsView,
        WorkspaceEditorView,
    },
};
use crudkit_leptos::crud_instance::CrudInstanceContext;
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TriggersPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub personalities: Vec<PersonalityView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationRuleInspectorView {
    pub trigger: AutomationTriggerView,
    pub revisions: Vec<AutomationRevisionView>,
    pub evaluations: Vec<AutomationEvaluationView>,
    pub current_revision_analytics: Option<RevisionAnalyticsView>,
}

#[component]
pub fn PageTriggers() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = automation_service();
    let initial = service.cached_page_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |selected_project| service_for_cache.cached_page(selected_project),
        move |selected_project| {
            let service = service_for_load.clone();
            let selected_project = selected_project.clone();
            async move { service.load_page(selected_project).await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    let active_project_names = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.active_project_names)
            .unwrap_or_default()
    });
    let codex_status = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.codex_status)
            .unwrap_or_default()
    });
    let topbar = top_bar(
        active_project_names,
        selected_project.into(),
        ActivePage::Triggers,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Automation"/>
        <div>
            {topbar}
            <main class="page-shell triggers-page">
                <section class="page-heading">
                    <h1>"Automation"</h1>
                </section>
                {move || {
                    result
                        .value
                        .get()
                        .map(triggers_content)
                        .unwrap_or_else(triggers_shell)
                }}
            </main>
        </div>
    }
}

fn triggers_shell() -> AnyView {
    view! {
        <>
            <section class="personalities panel">
                <div class="panel-heading"><h2>"Personalities"</h2></div>
            </section>
            <section class="automation-triggers panel">
                <div class="panel-heading"><h2>"Work-consuming automations"</h2></div>
            </section>
            <section class="automation-triggers panel">
                <div class="panel-heading"><h2>"Work-producing automations"</h2></div>
            </section>
        </>
    }
    .into_any()
}

fn triggers_content(page: TriggersPage) -> AnyView {
    let TriggersPage {
        projects: _,
        active_project_names: _,
        selected_project,
        selected_project_view,
        personalities,
        workspace_editors,
        api_base_url,
        codex_status: _,
    } = page;

    if let (Some(project), Some(project_view)) = (selected_project, selected_project_view) {
        let (consumer_context, set_consumer_context) = signal(None::<CrudInstanceContext>);
        let (producer_context, set_producer_context) = signal(None::<CrudInstanceContext>);
        let selected_trigger_id = Memo::new(move |_| {
            consumer_context
                .get()
                .and_then(selected_trigger_id_from_context)
                .or_else(|| {
                    producer_context
                        .get()
                        .and_then(selected_trigger_id_from_context)
                })
        });
        let consuming_triggers = automation_triggers_crudkit_instance(
            api_base_url.clone(),
            project.clone(),
            project_view.id,
            personalities.clone(),
            AutomationTableKind::Consuming,
            Callback::new(move |context| set_consumer_context.set(Some(context))),
        );
        let producing_triggers = automation_triggers_crudkit_instance(
            api_base_url.clone(),
            project.clone(),
            project_view.id,
            Vec::new(),
            AutomationTableKind::Producing,
            Callback::new(move |context| set_producer_context.set(Some(context))),
        );
        let trigger_runs = trigger_runs_panel(
            project.clone(),
            selected_trigger_id,
            workspace_editors.clone(),
        );
        let inspector = automation_rule_inspector(project.clone(), selected_trigger_id);
        view! {
            <>
                    {bundle_administration(project.clone())}
                    <PersonalitiesPanel
                        api_base_url=api_base_url
                        project=project.clone()
                        project_id=project_view.id
                    />
                    <section class="automation-triggers panel">
                        <div class="panel-heading">
                            <h2>"Work-consuming automations"</h2>
                        </div>
                        <div class="crudkit-automation-triggers" data-crudkit-leptos="automation-triggers">
                            {consuming_triggers}
                        </div>
                    </section>
                    <section class="automation-triggers panel">
                        <div class="panel-heading">
                            <h2>"Work-producing automations"</h2>
                        </div>
                        <div class="crudkit-automation-triggers" data-crudkit-leptos="automation-triggers">
                            {producing_triggers}
                        </div>
                    </section>
                    {inspector}
                    {trigger_runs}
            </>
        }
        .into_any()
    } else {
        view! {
            <section class="empty-state">
                <h2>"Choose a project"</h2>
                <a class="button-link" href="/projects">"Projects"</a>
            </section>
        }
        .into_any()
    }
}

fn automation_rule_inspector(project: String, selected_trigger_id: Memo<Option<i64>>) -> AnyView {
    let (inspector, set_inspector) = signal(None::<AutomationRuleInspectorView>);
    let (error, set_error) = signal(None::<String>);
    let refresh = RwSignal::new(0_u64);
    let project_for_load = project.clone();
    Effect::new(move |_| {
        refresh.get();
        let trigger_id = selected_trigger_id.get();
        set_inspector.set(None);
        set_error.set(None);
        if let Some(trigger_id) = trigger_id {
            let project = project_for_load.clone();
            leptos::task::spawn_local(async move {
                match load_automation_rule_inspector(project, trigger_id).await {
                    Ok(value) if selected_trigger_id.get_untracked() == Some(trigger_id) => {
                        set_inspector.set(Some(value));
                    }
                    Err(error) if selected_trigger_id.get_untracked() == Some(trigger_id) => {
                        set_error.set(Some(error.to_string()));
                    }
                    Ok(_) | Err(_) => {}
                }
            });
        }
    });

    let (route_item_id, set_route_item_id) = signal(String::new());
    let (route_result, set_route_result) = signal(String::new());
    let project_for_route = project.clone();

    view! {
        {move || {
            selected_trigger_id.get().map(|_| {
                let project_for_route = project_for_route.clone();
                let explain_route = move |_| {
                    match route_item_id.get_untracked().trim().parse::<i64>() {
                        Ok(item_id) => {
                            let project = project_for_route.clone();
                            leptos::task::spawn_local(async move {
                                let text = match explain_automation_route(project, item_id).await {
                                    Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                                    Err(error) => format!("Routing explanation failed: {error}"),
                                };
                                set_route_result.set(text);
                            });
                        }
                        Err(_) => set_route_result.set("Enter a numeric work-item ID.".to_owned()),
                    }
                };
                let content = match inspector.get() {
                    Some(value) => automation_rule_inspector_content(
                        value,
                        project.clone(),
                        refresh,
                        set_error,
                    ),
                    None => view! {
                        <p class="muted">
                            {error.get().unwrap_or_else(|| "Loading revision history…".to_owned())}
                        </p>
                    }
                    .into_any(),
                };
                view! {
                    <section class="automation-inspector panel" data-testid="automation-inspector">
                        <div class="panel-heading">
                            <h2>"Selected automation configuration"</h2>
                            <span class="muted">"Revisions, analytics, evaluations, and routing diagnostics"</span>
                        </div>
                        {content}
                        <div class="automation-route-explain">
                            <h3>"Explain routing"</h3>
                            <div class="automation-bundle-export">
                                <input
                                    placeholder="work-item ID"
                                    prop:value=move || route_item_id.get()
                                    on:input=move |event| set_route_item_id.set(event_target_value(&event))
                                />
                                <button type="button" on:click=explain_route>"Explain"</button>
                            </div>
                            <pre>{move || route_result.get()}</pre>
                        </div>
                    </section>
                }
            })
        }}
    }
    .into_any()
}

fn automation_rule_inspector_content(
    inspector: AutomationRuleInspectorView,
    project: String,
    refresh: RwSignal<u64>,
    set_error: WriteSignal<Option<String>>,
) -> AnyView {
    let trigger = inspector.trigger;
    let trigger_id = trigger.id;
    let managed = trigger.managed_bundle_key.is_some();
    let managed_badge = trigger.managed_bundle_key.clone().map(|bundle| {
        view! { <span class="automation-managed-badge">{"Managed by "}{bundle}</span> }
    });
    let detach = managed.then(|| {
        let project = project.clone();
        move |_| {
            let project = project.clone();
            leptos::task::spawn_local(async move {
                match detach_automation_rule(project, trigger_id).await {
                    Ok(_) => refresh.update(|value| *value += 1),
                    Err(error) => set_error.set(Some(format!("Detach failed: {error}"))),
                }
            });
        }
    });
    let analytics = inspector.current_revision_analytics.map(|analytics| {
        view! {
            <dl class="automation-analytics">
                <dt>"Runs"</dt><dd>{analytics.run_count}</dd>
                <dt>"Completed / failed"</dt><dd>{format!("{} / {}", analytics.completed_count, analytics.failed_count)}</dd>
                <dt>"Semantic pass / fail"</dt><dd>{format!("{} / {}", analytics.semantic_passed_count, analytics.semantic_failed_count)}</dd>
                <dt>"Created items"</dt><dd>{analytics.created_item_count}</dd>
                <dt>"Input / output tokens"</dt><dd>{format!("{} / {}", analytics.input_tokens, analytics.output_tokens)}</dd>
                <dt>"Total duration"</dt><dd>{format!("{}s", analytics.total_duration_seconds)}</dd>
            </dl>
        }
    });
    let current_revision_id = trigger.current_revision_id;
    let revisions = inspector
        .revisions
        .into_iter()
        .map(|revision| {
            let revision_id = revision.id;
            let current = current_revision_id == Some(revision_id);
            let project = project.clone();
            let restore = move |_| {
                let project = project.clone();
                leptos::task::spawn_local(async move {
                    match restore_automation_rule_revision(project, trigger_id, revision_id).await {
                        Ok(_) => refresh.update(|value| *value += 1),
                        Err(error) => set_error.set(Some(format!("Restore failed: {error}"))),
                    }
                });
            };
            let configuration = serde_json::to_string_pretty(&revision.configuration)
                .unwrap_or_else(|_| revision.configuration.to_string());
            view! {
                <li>
                    <div class="automation-revision-heading">
                        <span>{format!("Revision {} · {:?}", revision.revision_number, revision.operation)}</span>
                        <code>{revision.sha256}</code>
                        <button type="button" disabled=current || managed on:click=restore>
                            {if current { "Current" } else { "Restore" }}
                        </button>
                    </div>
                    <details>
                        <summary>"Configuration and prompt snapshot"</summary>
                        <pre>{configuration}</pre>
                    </details>
                </li>
            }
        })
        .collect::<Vec<_>>();
    let project_for_evaluations = project.clone();
    let evaluations = inspector
        .evaluations
        .into_iter()
        .map(|evaluation| {
            let item_link = evaluation.work_item_id.map(|item_id| {
                let href = format!(
                    "/projects/{}/items/{item_id}",
                    encode_path(&project_for_evaluations)
                );
                view! { <a href=href>{format!("item #{item_id}")}</a> }
            });
            let run_link = evaluation.run_id.map(|run_id| {
                let href = format!(
                    "/projects/{}/automation/runs/{run_id}/log",
                    encode_path(&project_for_evaluations)
                );
                view! { <a href=href>{format!("run #{run_id}")}</a> }
            });
            view! {
                <li>
                    <span>{format!("{} · {:?}", evaluation.activation_cause, evaluation.outcome)}</span>
                    {item_link}
                    {run_link}
                    {evaluation.error.map(|error| view! { <span class="error-message">{error}</span> })}
                </li>
            }
        })
        .collect::<Vec<_>>();

    view! {
        <div class="automation-inspector-summary">
            <div>
                <strong>{trigger.name}</strong>
                {managed_badge}
                {detach.map(|detach| view! {
                    <button type="button" class="secondary" on:click=detach>"Detach from bundle"</button>
                })}
            </div>
            {analytics}
        </div>
        <div class="automation-inspector-grid">
            <div>
                <h3>"Revision history"</h3>
                <ul class="automation-revision-list">{revisions}</ul>
            </div>
            <div>
                <h3>"Evaluation history"</h3>
                <ul class="automation-evaluation-list">{evaluations}</ul>
            </div>
        </div>
    }
    .into_any()
}

fn bundle_administration(project: String) -> AnyView {
    let (yaml, set_yaml) = signal(String::new());
    let (loaded_file_name, set_loaded_file_name) = signal(Option::<String>::None);
    let (allow_deletions, set_allow_deletions) = signal(false);
    let (result, set_result) = signal(String::new());
    let (installed, set_installed) = signal(Vec::new());
    let (installed_refresh, set_installed_refresh) = signal(0_u64);
    let (pending_removal, set_pending_removal) = signal(Option::<String>::None);

    let project_for_inventory = project.clone();
    Effect::new(move |_| {
        installed_refresh.get();
        let project = project_for_inventory.clone();
        leptos::task::spawn_local(async move {
            match list_installed_bundles(project).await {
                Ok(bundles) => set_installed.set(bundles),
                Err(error) => set_result.set(format!("Could not load installed bundles: {error}")),
            }
        });
    });

    let validate = move |_| {
        let yaml = yaml.get_untracked();
        leptos::task::spawn_local(async move {
            let text = match validate_bundle_yaml(yaml).await {
                Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                Err(error) => format!("Validation failed: {error}"),
            };
            set_result.set(text);
        });
    };
    let project_for_diff = project.clone();
    let diff = move |_| {
        let yaml = yaml.get_untracked();
        let project = project_for_diff.clone();
        leptos::task::spawn_local(async move {
            let text = match diff_bundle_yaml(project, yaml).await {
                Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                Err(error) => format!("Diff failed: {error}"),
            };
            set_result.set(text);
        });
    };
    let project_for_apply = project.clone();
    let apply = move |_| {
        let yaml = yaml.get_untracked();
        let project = project_for_apply.clone();
        let allow_deletions = allow_deletions.get_untracked();
        leptos::task::spawn_local(async move {
            match apply_bundle_yaml(project, yaml, allow_deletions).await {
                Ok(value) => {
                    set_result.set(serde_json::to_string_pretty(&value).unwrap_or_default());
                    set_pending_removal.set(None);
                    set_installed_refresh.update(|value| *value += 1);
                }
                Err(error) => set_result.set(format!("Apply failed: {error}")),
            }
        });
    };

    let on_file_change = move |event| {
        load_bundle_file(event, set_yaml, set_loaded_file_name, set_result);
    };
    let project_for_rows = project.clone();
    let installed_rows = move || {
        let project = project_for_rows.clone();
        installed
            .get()
            .into_iter()
            .map(|bundle| {
                let export_project = project.clone();
                let export_key = bundle.bundle_key.clone();
                let export = move |_| {
                    let project = export_project.clone();
                    let bundle_key = export_key.clone();
                    leptos::task::spawn_local(async move {
                        match export_bundle_yaml(project, bundle_key).await {
                            Ok(value) => {
                                set_yaml.set(value.yaml);
                                set_loaded_file_name.set(None);
                                set_result.set("Exported bundle into the YAML editor.".to_owned());
                            }
                            Err(error) => set_result.set(format!("Export failed: {error}")),
                        }
                    });
                };
                let remove_project = project.clone();
                let remove_key = bundle.bundle_key.clone();
                let remove_hash = bundle.manifest_hash.clone();
                let remove = move |_| {
                    if pending_removal.get_untracked().as_deref() != Some(remove_key.as_str()) {
                        set_pending_removal.set(Some(remove_key.clone()));
                        set_result.set(format!(
                            "Click Remove again to delete every object managed by '{}'.",
                            remove_key
                        ));
                        return;
                    }
                    let project = remove_project.clone();
                    let bundle_key = remove_key.clone();
                    let expected_hash = remove_hash.clone();
                    leptos::task::spawn_local(async move {
                        match remove_installed_bundle(project, bundle_key, expected_hash).await {
                            Ok(value) => {
                                set_result.set(
                                    serde_json::to_string_pretty(&value).unwrap_or_default(),
                                );
                                set_pending_removal.set(None);
                                set_installed_refresh.update(|value| *value += 1);
                            }
                            Err(error) => set_result.set(format!("Removal failed: {error}")),
                        }
                    });
                };
                let key_for_confirmation = bundle.bundle_key.clone();
                view! {
                    <li class="automation-installed-bundle" data-bundle-key=bundle.bundle_key.clone()>
                        <div>
                            <strong>{bundle.display_name}</strong>
                            <code>{bundle.bundle_key.clone()}</code>
                            <span class="muted">
                                {format!(
                                    "{} automations · {} personalities · installed {}",
                                    bundle.automation_count,
                                    bundle.personality_count,
                                    bundle.installed_at
                                )}
                            </span>
                        </div>
                        <div class="automation-installed-bundle-actions">
                            <button type="button" class="secondary" on:click=export>
                                "Export into editor"
                            </button>
                            <button type="button" class="danger" on:click=remove>
                                {move || {
                                    if pending_removal.get().as_deref()
                                        == Some(key_for_confirmation.as_str())
                                    {
                                        "Confirm removal"
                                    } else {
                                        "Remove"
                                    }
                                }}
                            </button>
                        </div>
                    </li>
                }
            })
            .collect::<Vec<_>>()
    };

    view! {
        <section class="automation-bundles panel" data-testid="automation-bundles">
            <div class="panel-heading">
                <h2>"Automation bundles"</h2>
                <span class="muted">"Schema v1 · managed objects require detach-before-edit"</span>
            </div>
            <div class="automation-installed-bundles">
                <h3>"Installed bundles"</h3>
                {move || {
                    if installed.get().is_empty() {
                        view! { <p class="muted">"No bundles installed in this project."</p> }
                            .into_any()
                    } else {
                        view! { <ul>{installed_rows()}</ul> }.into_any()
                    }
                }}
            </div>
            <label>
                "Bundle YAML"
                <span class="automation-bundle-file-picker">
                    <input
                        type="file"
                        accept=".yaml,.yml,application/yaml,text/yaml,text/x-yaml"
                        data-testid="automation-bundle-file"
                        on:change=on_file_change
                    />
                    {move || loaded_file_name.get().map(|name| format!("Loaded {name}"))}
                </span>
                <textarea
                    class="automation-bundle-yaml"
                    rows="18"
                    prop:value=move || yaml.get()
                    on:input=move |event| set_yaml.set(event_target_value(&event))
                ></textarea>
            </label>
            <div class="automation-bundle-actions">
                <button type="button" on:click=validate>"Validate"</button>
                <button type="button" on:click=diff>"Diff"</button>
                <label>
                    <input
                        type="checkbox"
                        prop:checked=move || allow_deletions.get()
                        on:change=move |event| {
                            set_allow_deletions.set(event_target_checked(&event));
                        }
                    />
                    "Confirm managed deletions"
                </label>
                <button type="button" on:click=apply>"Apply"</button>
            </div>
            <pre class="automation-bundle-result">{move || result.get()}</pre>
        </section>
    }
    .into_any()
}

fn load_bundle_file(
    event: leptos::ev::Event,
    set_yaml: WriteSignal<String>,
    set_loaded_file_name: WriteSignal<Option<String>>,
    set_result: WriteSignal<String>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let Some(input) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            set_result.set("Could not read the selected bundle file.".to_owned());
            return;
        };
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        let name = file.name();
        leptos::task::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(file.text()).await {
                Ok(value) => match value.as_string() {
                    Some(text) => {
                        set_yaml.set(text);
                        set_loaded_file_name.set(Some(name.clone()));
                        set_result.set(format!("Loaded {name}. Validate or diff before applying."));
                    }
                    None => set_result.set(format!("Could not decode {name} as text.")),
                },
                Err(_) => set_result.set(format!("Could not read {name}.")),
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (event, set_yaml, set_loaded_file_name, set_result);
    }
}
