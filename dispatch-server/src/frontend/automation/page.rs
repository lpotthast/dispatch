use crate::frontend::automation::types::AutomationConfiguration;
use crate::frontend::queries::query_resource;
use crudkit_leptos::crud_instance::CrudInstanceContext;
use leptonic::components::prelude::Toggle;
use leptos::prelude::*;
use leptos_meta::Title;
use {
    crate::{
        frontend::{
            automation::rules::selected_trigger_id_from_context,
            automation::{
                personalities::PersonalitiesPanel,
                rules::{AutomationTableKind, AutomationTriggersCrudkitInstance},
                service::automation_service,
            },
            components::selected_project_signal,
            items::components::encode_path,
            projects::service::project_service,
            runs::panels::TriggerRunsPanel,
        },
        shared::view_models::{AgentGitCommandPolicy, ProjectSettingsView},
    },
    dispatch_types::CommitPolicyUpdate,
};

use dispatch_types::AutomationRuleInspectorView;

#[component]
pub fn PageTriggers() -> impl IntoView {
    let selected_project = selected_project_signal();
    let store = crate::frontend::automation::store::automation_store();
    let initial = store.cached_page_untracked(&selected_project.get_untracked());
    let store_for_cache = store.clone();
    let store_for_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let result = query_resource(
        initial,
        move || selected_project.get(),
        move |selected_project| store_for_cache.cached_page(selected_project),
        move |selected_project| {
            let store = store_for_load.clone();
            let selected_project = selected_project.clone();
            async move { store.load_page(selected_project).await }
        },
        move |key, value| store_for_seed.seed_page(key, value),
        move |key| store_for_refresh.invalidate_page(key),
    );
    let initial_auto_commit = result
        .value
        .with_untracked(|page| {
            page.as_ref()
                .and_then(|page| page.settings.as_ref())
                .map(|settings| settings.auto_commit)
        })
        .unwrap_or(false);
    let (auto_commit, set_auto_commit) = signal(initial_auto_commit);
    Effect::new(move |_| {
        if let Some(value) = result.value.with(|page| {
            page.as_ref()
                .and_then(|page| page.settings.as_ref())
                .map(|settings| settings.auto_commit)
        }) {
            set_auto_commit.set(value);
        }
    });

    view! {
            <Title text="Automation"/>
            <div>
                <main class="page-shell triggers-page">
                    <section class="page-heading">
                        <h1>"Automation"</h1>
                    </section>
                    <div class="automation-sections">
                        <Transition fallback=|| view! { <TriggersShell/> }>
                            <For each=move || result.value.get() key=|page| page.selected_project_view.as_ref().map(|project| project.id) children=move |initial| {
                                let identity = initial.selected_project_view.as_ref().map(|project| project.id);
                                let page = Signal::derive(move || result.value.get().filter(|page| page.selected_project_view.as_ref().map(|project| project.id) == identity).unwrap_or_else(|| initial.clone()));
                                view! { <TriggersContent page auto_commit set_auto_commit refresh=result.refresh/> }
                            }/>
                        </Transition>
    <crate::frontend::components::QueryFeedback pending=result.pending error=result.error refresh=result.refresh/>
                    </div>
                </main>
            </div>
        }
}

#[component]
fn TriggersShell() -> impl IntoView {
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
            <section class="project-settings automation-policy-shell">
                <div class="commit-policy"><h2>"Automation policy"</h2></div>
            </section>
        </>
    }
}

#[component]
fn TriggersContent(
    page: Signal<AutomationConfiguration>,
    auto_commit: ReadSignal<bool>,
    set_auto_commit: WriteSignal<bool>,
    refresh: Callback<()>,
) -> impl IntoView {
    let api_base_url = crate::frontend::http::HttpService::get().api_base_url();
    let AutomationConfiguration {
        selected_project,
        selected_project_view,
        settings,
        personalities: _,
    } = page.get_untracked();

    if let (Some(project), Some(project_view), Some(settings)) =
        (selected_project, selected_project_view, settings)
    {
        let personalities = Signal::derive(move || page.get().personalities);
        let settings =
            Signal::derive(move || page.get().settings.unwrap_or_else(|| settings.clone()));
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
        let consuming_triggers = view! {
            <AutomationTriggersCrudkitInstance
                api_base_url=api_base_url.clone()
                project=project.clone()
                project_id=project_view.id
                personalities
                kind=AutomationTableKind::Consuming
                on_context_created=Callback::new(move |context| {
                    set_consumer_context.set(Some(context));
                })
            />
        };
        let producing_triggers = view! {
            <AutomationTriggersCrudkitInstance
                api_base_url=api_base_url.clone()
                project=project.clone()
                project_id=project_view.id
                personalities=Signal::derive(Vec::new)
                kind=AutomationTableKind::Producing
                on_context_created=Callback::new(move |context| {
                    set_producer_context.set(Some(context));
                })
            />
        };
        let trigger_runs = view! {
            <TriggerRunsPanel project=project.clone() selected_trigger_id/>
        };
        let inspector = view! {
            <AutomationRuleInspector project=project.clone() selected_trigger_id/>
        };
        view! {
            <>
                    <BundleAdministration project=project.clone()/>
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
                    <AutomationPolicy
                        project=project.clone()
                        settings
                        auto_commit
                        set_auto_commit
                        refresh
                    />
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

#[component]
fn AutomationPolicy(
    project: String,
    settings: Signal<ProjectSettingsView>,
    auto_commit: ReadSignal<bool>,
    set_auto_commit: WriteSignal<bool>,
    refresh: Callback<()>,
) -> impl IntoView + 'static {
    let initial = settings.get_untracked();
    let (commit_standard, set_commit_standard) = signal(initial.commit_standard);
    let (max_read_only_agents, set_max_read_only_agents) = signal(initial.max_read_only_agents);
    let (revert_strategy, set_revert_strategy) = signal(initial.revert_strategy);
    let git_policy = initial.agent_git_command_policy;
    let (git_add, set_git_add) = signal(git_policy.add);
    let (git_commit, set_git_commit) = signal(git_policy.commit);
    let (git_push, set_git_push) = signal(git_policy.push);
    let (git_reset, set_git_reset) = signal(git_policy.reset);
    let (git_hard_reset, set_git_hard_reset) = signal(git_policy.hard_reset);
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().commit_standard),
        commit_standard,
        set_commit_standard,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().max_read_only_agents),
        max_read_only_agents,
        set_max_read_only_agents,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().revert_strategy),
        revert_strategy,
        set_revert_strategy,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().agent_git_command_policy.add),
        git_add,
        set_git_add,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().agent_git_command_policy.commit),
        git_commit,
        set_git_commit,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().agent_git_command_policy.push),
        git_push,
        set_git_push,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().agent_git_command_policy.reset),
        git_reset,
        set_git_reset,
    );
    crate::frontend::components::sync_saved_value(
        Signal::derive(move || settings.get().agent_git_command_policy.hard_reset),
        git_hard_reset,
        set_git_hard_reset,
    );
    let service = project_service();
    let save_action = Action::new(move |_: &()| {
        let service = service.clone();
        let project = project.clone();
        let update = CommitPolicyUpdate {
            max_read_only_agents: max_read_only_agents.get_untracked(),
            auto_commit: auto_commit.get_untracked(),
            commit_standard: commit_standard.get_untracked(),
            revert_strategy: revert_strategy.get_untracked(),
            agent_git_command_policy: AgentGitCommandPolicy {
                add: git_add.get_untracked(),
                commit: git_commit.get_untracked(),
                push: git_push.get_untracked(),
                reset: git_reset.get_untracked(),
                hard_reset: git_hard_reset.get_untracked(),
            },
        };
        async move {
            if service.update_commit_policy(project, update).await.is_ok() {
                refresh.run(());
            }
        }
    });
    let pending = save_action.pending();
    let save = move |_| {
        if pending.get_untracked() {
            return;
        }
        save_action.dispatch(());
    };

    view! {
        <section class="project-settings automation-policy-settings">
            <div class="commit-policy">
                <h2>"Automation policy"</h2>
                <div class="commit-policy-controls">
                    <label for="project-max-read-only-agents">"Read-only agents"</label>
                    <input
                        id="project-max-read-only-agents"
                        type="number"
                        min="0"
                        step="1"
                        prop:value=move || max_read_only_agents.get().to_string()
                        on:input=move |event| {
                            if let Ok(value) = event_target_value(&event).parse() {
                                set_max_read_only_agents.set(value);
                            }
                        }
                    />
                    <label class="checkbox-row auto-commit-policy-control">
                        <span>"Auto-Commit"</span>
                        <Toggle
                            state=auto_commit
                            set_state=set_auto_commit
                            attr:id="project-auto-commit"
                            attr:aria-label="Auto-Commit"
                        />
                    </label>
                    <label for="project-commit-standard">"Commit standard"</label>
                    <textarea
                        id="project-commit-standard"
                        placeholder="Commit message standard"
                        prop:value=move || commit_standard.get()
                        on:input=move |event| set_commit_standard.set(event_target_value(&event))
                    ></textarea>
                    <label for="project-revert-strategy">"Failure revert"</label>
                    <select
                        id="project-revert-strategy"
                        prop:value=move || revert_strategy.get().as_storage()
                        on:change=move |event| {
                            if let Ok(value) = event_target_value(&event).parse() {
                                set_revert_strategy.set(value);
                            }
                        }
                    >
                        <option value="manual">"revert manually"</option>
                        <option value="git_reset">"git reset"</option>
                    </select>
                    <div class="git-command-policy">
                        <label class="checkbox-row" for="project-git-add">
                            <input
                                id="project-git-add"
                                type="checkbox"
                                prop:checked=move || git_add.get()
                                on:change=move |event| set_git_add.set(event_target_checked(&event))
                            />
                            <span>"git add"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-commit">
                            <input
                                id="project-git-commit"
                                type="checkbox"
                                prop:checked=move || git_commit.get()
                                on:change=move |event| set_git_commit.set(event_target_checked(&event))
                            />
                            <span>"git commit"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-push">
                            <input
                                id="project-git-push"
                                type="checkbox"
                                prop:checked=move || git_push.get()
                                on:change=move |event| set_git_push.set(event_target_checked(&event))
                            />
                            <span>"git push"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-reset">
                            <input
                                id="project-git-reset"
                                type="checkbox"
                                prop:checked=move || git_reset.get()
                                on:change=move |event| set_git_reset.set(event_target_checked(&event))
                            />
                            <span>"git reset"</span>
                        </label>
                    </div>
                    <label for="project-git-hard-reset">"Hard reset"</label>
                    <select
                        id="project-git-hard-reset"
                        prop:value=move || git_hard_reset.get().as_storage()
                        on:change=move |event| {
                            if let Ok(value) = event_target_value(&event).parse() {
                                set_git_hard_reset.set(value);
                            }
                        }
                    >
                        <option value="isolated_workspaces">
                            "isolated branches/worktrees only"
                        </option>
                        <option value="never">"never"</option>
                    </select>
                    <button type="button" disabled=move || pending.get() on:click=save>
                        "Save policy"
                    </button>
                </div>
            </div>
        </section>
    }
}

#[component]
fn AutomationRuleInspector(
    project: String,
    selected_trigger_id: Memo<Option<i64>>,
) -> impl IntoView {
    let store = crate::frontend::automation::store::automation_store();
    let (error, set_error) = signal(None::<String>);
    let refresh = RwSignal::new(0_u64);
    let project_for_load = project.clone();
    let loaded = LocalResource::new(move || {
        refresh.track();
        let id = selected_trigger_id.get();
        let project = project_for_load.clone();
        let store = store.clone();
        async move {
            let result = match id {
                Some(id) => store.load_rule_inspector(project, id).await.map(Some),
                None => Ok(None),
            };
            (id, result)
        }
    });
    let inspector = Signal::derive(move || {
        loaded.get().and_then(|(id, result)| {
            (id == selected_trigger_id.get())
                .then(|| result.ok().flatten())
                .flatten()
        })
    });
    Effect::new(move |_| {
        set_error.set(loaded.get().and_then(|(id, result)| {
            (id == selected_trigger_id.get_untracked())
                .then(|| result.err().map(|error| error.to_string()))
                .flatten()
        }));
    });
    let (route_item_id, set_route_item_id) = signal(String::new());
    let (route_result, set_route_result) = signal(String::new());
    let project_for_route = project.clone();
    let route_service = automation_service();
    let explain = Action::new(move |item_id: &i64| {
        let item_id = *item_id;
        let service = route_service.clone();
        let project = project_for_route.clone();
        async move {
            let text = match service.explain_route(project, item_id).await {
                Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                Err(error) => format!("Routing explanation failed: {error}"),
            };
            set_route_result.set(text);
        }
    });

    view! {
        {move || {
            selected_trigger_id.get().map(|_| {
                let explain_route = move |_| {
                    match route_item_id.get_untracked().trim().parse::<i64>() {
                        Ok(item_id) => { if !explain.pending().get_untracked() { explain.dispatch(item_id); } }
                        Err(_) => set_route_result.set("Enter a numeric work-item ID.".to_owned()),
                    }
                };
                let content = match inspector.get() {
                    Some(value) => view! {
                        <AutomationRuleInspectorContent
                            inspector=value
                            project=project.clone()
                            refresh
                            set_error
                        />
                    }.into_any(),
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

#[component]
fn AutomationRuleInspectorContent(
    inspector: AutomationRuleInspectorView,
    project: String,
    refresh: RwSignal<u64>,
    set_error: WriteSignal<Option<String>>,
) -> impl IntoView {
    let service = automation_service();
    let detach_service = service.clone();
    let restore_service = service;
    let trigger = inspector.trigger;
    let trigger_id = trigger.id;
    let managed = trigger.managed_bundle_key.is_some();
    let managed_badge = trigger.managed_bundle_key.clone().map(|bundle| {
        view! { <span class="automation-managed-badge">{"Managed by "}{bundle}</span> }
    });
    let project_for_detach = project.clone();
    let detach_action = Action::new(move |_: &()| {
        let project = project_for_detach.clone();
        let service = detach_service.clone();
        async move {
            match service.detach_rule(project, trigger_id).await {
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
            let service = restore_service.clone();
            let restore_action = Action::new(move |_: &()| {
                let project = project.clone();
                let service = service.clone();
                async move {
                    match service
                        .restore_rule_revision(project, trigger_id, revision_id)
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
            let configuration = serde_json::to_string_pretty(&revision.configuration)
                .unwrap_or_else(|_| revision.configuration.to_string());
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

#[component]
fn BundleAdministration(project: String) -> impl IntoView {
    let service = automation_service();
    let inventory_store = crate::frontend::automation::store::automation_store();
    let validate_service = service.clone();
    let diff_service = service.clone();
    let apply_service = service.clone();
    let rows_service = service;
    let (yaml, set_yaml) = signal(String::new());
    let (loaded_file_name, set_loaded_file_name) = signal(Option::<String>::None);
    let (allow_deletions, set_allow_deletions) = signal(false);
    let (result, set_result) = signal(String::new());
    let (installed, set_installed) = signal(Vec::new());
    let (installed_refresh, set_installed_refresh) = signal(0_u64);
    let (pending_removal, set_pending_removal) = signal(Option::<String>::None);

    let project_for_inventory = project.clone();
    let inventory = LocalResource::new(move || {
        installed_refresh.track();
        let project = project_for_inventory.clone();
        let store = inventory_store.clone();
        async move { store.list_installed_bundles(project).await }
    });
    Effect::new(move |_| {
        if let Some(result) = inventory.get() {
            match result {
                Ok(bundles) => set_installed.set(bundles),
                Err(error) => set_result.set(format!("Could not load installed bundles: {error}")),
            }
        }
    });

    let validate_action = Action::new(move |_: &()| {
        let yaml = yaml.get_untracked();
        let service = validate_service.clone();
        async move {
            let text = match service.validate_bundle_yaml(yaml).await {
                Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                Err(error) => format!("Validation failed: {error}"),
            };
            set_result.set(text);
        }
    });
    let validate_pending = validate_action.pending();
    let validate = move |_| {
        validate_action.dispatch(());
    };
    let project_for_diff = project.clone();
    let diff_action = Action::new(move |_: &()| {
        let yaml = yaml.get_untracked();
        let project = project_for_diff.clone();
        let service = diff_service.clone();
        async move {
            let text = match service.diff_bundle_yaml(project, yaml).await {
                Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_default(),
                Err(error) => format!("Diff failed: {error}"),
            };
            set_result.set(text);
        }
    });
    let diff_pending = diff_action.pending();
    let diff = move |_| {
        diff_action.dispatch(());
    };
    let project_for_apply = project.clone();
    let apply_action = Action::new(move |_: &()| {
        let yaml = yaml.get_untracked();
        let project = project_for_apply.clone();
        let allow_deletions = allow_deletions.get_untracked();
        let service = apply_service.clone();
        async move {
            match service
                .apply_bundle_yaml(project, yaml, allow_deletions)
                .await
            {
                Ok(value) => {
                    set_result.set(serde_json::to_string_pretty(&value).unwrap_or_default());
                    set_pending_removal.set(None);
                    set_installed_refresh.update(|value| *value += 1);
                }
                Err(error) => set_result.set(format!("Apply failed: {error}")),
            }
        }
    });
    let apply_pending = apply_action.pending();
    let apply = move |_| {
        apply_action.dispatch(());
    };

    let on_file_change = move |event| {
        load_bundle_file(event, set_yaml, set_loaded_file_name, set_result);
    };
    let project_for_rows = project.clone();
    let installed_rows = move || {
        let project = project_for_rows.clone();
        let service = rows_service.clone();
        installed
            .get()
            .into_iter()
            .map(|bundle| {
                let export_project = project.clone();
                let export_key = bundle.bundle_key.clone();
                let export_service = service.clone();
                let export_action = Action::new(move |_: &()| {
                    let project = export_project.clone();
                    let bundle_key = export_key.clone();
                    let service = export_service.clone();
                    async move {
                        match service.export_bundle_yaml(project, bundle_key).await {
                            Ok(value) => {
                                set_yaml.set(value.yaml);
                                set_loaded_file_name.set(None);
                                set_result.set("Exported bundle into the YAML editor.".to_owned());
                            }
                            Err(error) => set_result.set(format!("Export failed: {error}")),
                        }
                    }

    });
    let export_pending = export_action.pending();
    let export = move |_| {
        export_action.dispatch(());
    };
                let remove_project = project.clone();
                let remove_key = bundle.bundle_key.clone();
                let remove_hash = bundle.manifest_hash.clone();
                let remove_service = service.clone();
                let request_remove_key = remove_key.clone();
                let remove_action = Action::new(move |_: &()| {
                    let project = remove_project.clone();
                    let bundle_key = request_remove_key.clone();
                    let expected_hash = remove_hash.clone();
                    let service = remove_service.clone();
                    async move {
                        match service
                            .remove_installed_bundle(project, bundle_key, expected_hash)
                            .await
                        {
                            Ok(value) => {
                                set_result.set(
                                    serde_json::to_string_pretty(&value).unwrap_or_default(),
                                );
                                set_pending_removal.set(None);
                                set_installed_refresh.update(|value| *value += 1);
                            }
                            Err(error) => set_result.set(format!("Removal failed: {error}")),
                        }
                    }

    });
    let remove_pending = remove_action.pending();
    let remove = move |_| {
                    if pending_removal.get_untracked().as_deref() != Some(remove_key.as_str()) {
                        set_pending_removal.set(Some(remove_key.clone()));
                        set_result.set(format!(
                            "Click Remove again to delete every object managed by '{}'.",
                            remove_key
                        ));
                        return;
                    }
        remove_action.dispatch(());
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
                            <button type="button" class="secondary" disabled=move || export_pending.get() on:click=export>
                                "Export into editor"
                            </button>
                            <button type="button" class="danger" disabled=move || remove_pending.get() on:click=remove>
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
                <button type="button" disabled=move || validate_pending.get() on:click=validate>"Validate"</button>
                <button type="button" disabled=move || diff_pending.get() on:click=diff>"Diff"</button>
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
                <button type="button" disabled=move || apply_pending.get() on:click=apply>"Apply"</button>
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
