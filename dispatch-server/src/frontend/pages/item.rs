use crate::{
    frontend::{
        components::{ActivePage, cached_query, encode_path, top_bar},
        live_events::{item_event_matches, refetch_on_live_event},
        services::{item_service, project_cache},
    },
    shared::view_models::{
        AgentRunView, CodexAppServerStatusView, CommentView, ProjectLabelView, ProjectView,
        WorkItemRelationshipListEntry, WorkItemStateView, WorkItemView,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub project: String,
    pub item: WorkItemView,
    pub comments: Vec<CommentView>,
    pub relationships: Vec<WorkItemRelationshipListEntry>,
    pub label_suggestions: Vec<ProjectLabelView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub automation_runs: Vec<AgentRunView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageItem() -> impl IntoView {
    let params = use_params_map();
    let project = params.read_untracked().get("project");
    let item_id = params
        .read_untracked()
        .get("item_id")
        .and_then(|value| value.parse::<i64>().ok());
    let project_for_loader = project.clone();
    let project_for_events = project.clone();
    let service = item_service();
    let initial = service.cached_page_untracked(&project, item_id);
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || (project_for_loader.clone(), item_id),
        move |(project, item_id)| service_for_cache.cached_page(project, *item_id),
        move |(project, item_id)| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move { service.load_page(project, item_id).await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, move |event| {
        item_event_matches(event, project_for_events.as_deref(), item_id)
    });
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
        Signal::derive({
            let project = project.clone();
            move || project.clone()
        }),
        ActivePage::Board,
        Signal::derive(|| None),
        codex_status,
    );
    let board_href = project
        .as_deref()
        .map(|project| format!("/?project={}", encode_path(project)))
        .unwrap_or_else(|| "/".to_owned());
    let fallback_title = item_id
        .map(|item_id| format!("#{item_id}"))
        .unwrap_or_else(|| "Work item".to_owned());
    view! {
        <Title text="Dispatch"/>
        <div>
            {topbar}
            <main class="page-shell item-page">
                {move || {
                    result
                        .value
                        .get()
                        .map(|page| item_content(page, result.refresh))
                        .unwrap_or_else(|| {
                        view! {
                            <section class="item-header">
                                <a class="item-board-link" href=board_href.clone()>"Board"</a>
                                <h1>{fallback_title.clone()}</h1>
                            </section>
                        }
                        .into_any()
                    })
                }}
            </main>
        </div>
    }
}

mod content {
    use std::future::Future;

    use crate::{
        frontend::{
            ItemPage,
            components::{
                WorkItemStatesContext, claim_badge, encode_path, format_label, item_href,
                provide_work_item_states_context, run_token_usage_label, state_label,
            },
            crudkit::{crudkit_i64_id, work_items_crudkit_config_for_view},
            services::item_service,
        },
        shared::view_models::{
            AUTOMATION_BLOCKED_LABEL_KEY, AddCommentRequest, AgentRunView, AuthorType,
            CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest, DEFAULT_STATE_LABEL,
            FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView, STATE_LABEL_KEY,
            UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest, WorkItemLabelView,
            WorkItemRelationshipDirection, WorkItemRelationshipItemSummary,
            WorkItemRelationshipListEntry, WorkItemStateView, WorkItemView,
        },
    };
    use crudkit_leptos::crud_instance::CrudInstanceContext;
    use crudkit_leptos::{
        crud_instance_config::CrudNavigationConfig, crudkit_web::view::SerializableCrudView,
        prelude::*,
    };
    use leptos::prelude::*;
    use leptos_router::{NavigateOptions, hooks::use_navigate};

    pub(crate) fn item_content(page: ItemPage, refresh: Callback<()>) -> AnyView {
        let ItemPage {
            projects: _,
            active_project_names: _,
            project,
            item,
            comments,
            relationships,
            label_suggestions,
            work_item_states,
            automation_runs,
            api_base_url,
            codex_status: _,
        } = page;
        provide_work_item_states_context(work_item_states);
        let board_href = format!("/?project={}", encode_path(&project));
        let comment_action = format!(
            "/projects/{}/items/{}/comments",
            encode_path(&project),
            item.id
        );
        let header_title = format!("#{} {}", item.id, item.title);
        let item_state_display = state_label(&item).to_owned();
        let item_project_id = item.project_id;
        let item_id = item.id;
        let (item_editor_context, set_item_editor_context) = signal(None::<CrudInstanceContext>);
        let navigate = use_navigate();
        let board_href_for_exit = board_href.clone();
        let exit_to_board = Callback::new(move |()| {
            navigate(&board_href_for_exit, NavigateOptions::default());
        });
        let exit_to_board_for_link = exit_to_board;
        let editor_default_create_state = Signal::derive(|| DEFAULT_STATE_LABEL.to_owned());
        let item_detail_navigation = CrudNavigationConfig {
            show_delete: true,
            ..CrudNavigationConfig::embedded_single_entity()
        };
        let mut item_detail_config = work_items_crudkit_config_for_view(
            api_base_url,
            item_project_id,
            SerializableCrudView::Edit(crudkit_i64_id(item_id)),
            item_detail_navigation,
            editor_default_create_state,
            None,
            Signal::derive(Vec::<ProjectLabelView>::new),
        );
        item_detail_config.elements = without_crudkit_field(item_detail_config.elements, "id");
        let item_editor = view! {
            <div class="crudkit-item-detail" data-crudkit-leptos="work-item-detail">
                <CrudInstance
                    name="work-item-detail"
                    config=item_detail_config
                    on_exit=exit_to_board
                    on_context_created=Callback::new(move |context| {
                        set_item_editor_context.set(Some(context));
                    })
                />
            </div>
        };
        let (comment_author, set_comment_author) = signal(String::new());
        let (comment_body, set_comment_body) = signal(String::new());
        let service = item_service();
        let project_for_comment = project.clone();
        let comment_mutation = mutation_action(
            refresh,
            move || {
                set_comment_author.set(String::new());
                set_comment_body.set(String::new());
            },
            move |request: AddCommentRequest| {
                let service = service.clone();
                let project = project_for_comment.clone();
                async move { service.add_comment(project, item_id, request).await }
            },
        );
        let comment_pending = comment_mutation.pending();
        let comment_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if comment_pending.get_untracked() {
                return;
            }
            comment_mutation.dispatch(AddCommentRequest {
                author_type: AuthorType::User,
                author_name: Some(comment_author.get_untracked()),
                body: comment_body.get_untracked(),
            });
        };
        let claim = item
            .claimed_by
            .clone()
            .map(|agent| claim_badge(&project, agent, "Claimed", item.claimed_at.clone()));
        let finished = item.finished_at.clone().map(|finished_at| {
            view! { <span>"finished " {finished_at}</span> }
        });
        let automation_run_views = automation_runs_view(&project, automation_runs);
        let comment_views = comments
            .into_iter()
            .map(|comment| {
                let author = comment
                    .author_name
                    .unwrap_or_else(|| comment.author_type.as_storage().to_owned());
                let author = comment_author_view(&project, comment.author_type, author);
                view! {
                    <article>
                        <strong>{author}</strong>
                        <span>{comment.created_at}</span>
                        <p>{comment.body}</p>
                    </article>
                }
            })
            .collect::<Vec<_>>();
        let labels = item_labels_view(&project, &item, label_suggestions, refresh);
        let relationship_views = item_relationships_view(&project, &item, relationships, refresh);

        view! {
            <>
                    <section class="item-header">
                        <button
                            type="button"
                            class="link-button item-board-link"
                            on:click=move |_| {
                                if let Some(context) = item_editor_context.get_untracked() {
                                    context.request_leave();
                                } else {
                                    exit_to_board_for_link.run(());
                                }
                            }
                        >
                            "Board"
                        </button>
                        <h1>{header_title}</h1>
                    </section>
                    <section class="item-meta">
                        <span>{item_state_display}</span>
                        <span>"v" {item.version}</span>
                        {claim}
                        {finished}
                    </section>
                    <section class="item-settings panel">
                        <h2>"Item details"</h2>
                        {item_editor}
                    </section>
                    {labels}
                    {relationship_views}
                    {automation_run_views}
                    <section class="comments">
                        <h2>"Comments"</h2>
                        {comment_views}
                        <form method="post" action=comment_action on:submit=comment_submit>
                            <input
                                name="author_name"
                                placeholder="Your name"
                                prop:value=move || comment_author.get()
                                on:input=move |event| {
                                    set_comment_author.set(event_target_value(&event));
                                }
                            />
                            <textarea
                                name="body"
                                placeholder="Comment"
                                required
                                prop:value=move || comment_body.get()
                                on:input=move |event| {
                                    set_comment_body.set(event_target_value(&event));
                                }
                            ></textarea>
                            <button disabled=move || comment_pending.get()>"Add comment"</button>
                        </form>
                    </section>
            </>
        }
        .into_any()
    }

    fn mutation_action<Input, Mutate, MutationFuture, OnSuccess>(
        refresh: Callback<()>,
        on_success: OnSuccess,
        mutate: Mutate,
    ) -> Action<Input, Result<(), ServerFnError>>
    where
        Input: Clone + 'static,
        Mutate: Fn(Input) -> MutationFuture + Clone + 'static,
        MutationFuture: Future<Output = Result<(), ServerFnError>> + 'static,
        OnSuccess: Fn() + 'static,
    {
        let action = Action::new_local(move |input: &Input| {
            let input = input.clone();
            let mutate = mutate.clone();
            async move { mutate(input).await }
        });
        Effect::new(move |_| {
            let succeeded = action
                .value()
                .with(|result| result.as_ref().is_some_and(Result::is_ok));
            if succeeded {
                on_success();
                refresh.run(());
            }
        });
        action
    }

    fn without_crudkit_field<F: TypeErasedField>(
        elements: Vec<Elem<F>>,
        field_name: &str,
    ) -> Vec<Elem<F>> {
        elements
            .into_iter()
            .filter_map(|element| match element {
                Elem::Field((field, _)) if field.name() == field_name => None,
                Elem::Enclosing(enclosing) => Some(Elem::Enclosing(
                    without_crudkit_enclosing_field(enclosing, field_name),
                )),
                element => Some(element),
            })
            .collect()
    }

    fn without_crudkit_enclosing_field<F: TypeErasedField>(
        enclosing: Enclosing<F>,
        field_name: &str,
    ) -> Enclosing<F> {
        match enclosing {
            Enclosing::None(mut group) => {
                group.children = without_crudkit_field(group.children, field_name);
                Enclosing::None(group)
            }
            Enclosing::Tabs(tabs) => Enclosing::Tabs(
                tabs.into_iter()
                    .map(|mut tab| {
                        tab.group.children = without_crudkit_field(tab.group.children, field_name);
                        tab
                    })
                    .collect(),
            ),
            Enclosing::Card(mut group) => {
                group.children = without_crudkit_field(group.children, field_name);
                Enclosing::Card(group)
            }
        }
    }

    fn comment_author_view(project: &str, author_type: AuthorType, author: String) -> AnyView {
        if author_type == AuthorType::Agent
            && let Some(run_id) = infer_dispatch_run_id(&author)
        {
            let href = format!(
                "/projects/{}/automation/runs/{}/log",
                encode_path(project),
                run_id
            );
            return view! {
                <a class="comment-author-link" href=href>{author}</a>
            }
            .into_any();
        }

        view! { {author} }.into_any()
    }

    pub(crate) fn infer_dispatch_run_id(agent_id: &str) -> Option<i64> {
        let id = agent_id.strip_prefix("dispatch-run-")?;
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let run_id = id.parse::<i64>().ok()?;
        (run_id > 0).then_some(run_id)
    }

    fn item_relationships_view(
        project: &str,
        item: &WorkItemView,
        relationships: Vec<WorkItemRelationshipListEntry>,
        refresh: Callback<()>,
    ) -> AnyView {
        let add_action = format!(
            "/projects/{}/items/{}/relationships",
            encode_path(project),
            item.id
        );
        let (target_item_id, set_target_item_id) = signal(String::new());
        let (relationship_kind, set_relationship_kind) = signal(String::new());
        let service = item_service();
        let project_for_add = project.to_owned();
        let item_id = item.id;
        let add_mutation = mutation_action(
            refresh,
            move || {
                set_target_item_id.set(String::new());
                set_relationship_kind.set(String::new());
            },
            move |request: CreateWorkItemRelationshipRequest| {
                let service = service.clone();
                let project = project_for_add.clone();
                async move { service.add_relationship(project, item_id, request).await }
            },
        );
        let add_pending = add_mutation.pending();
        let add_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if add_pending.get_untracked() {
                return;
            }
            let Ok(target_work_item_id) = target_item_id.get_untracked().parse() else {
                return;
            };
            add_mutation.dispatch(CreateWorkItemRelationshipRequest {
                target_work_item_id,
                kind: relationship_kind.get_untracked(),
            });
        };
        let rows = relationships
            .into_iter()
            .map(|entry| item_relationship_row(project, item.id, entry, refresh))
            .collect::<Vec<_>>();
        let empty = rows.is_empty().then(|| {
            view! { <p class="muted">"No relationships"</p> }
        });

        view! {
        <section class="item-relationships panel">
            <h2>"Relationships"</h2>
            <div class="relationship-list">
                {empty}
                {rows}
            </div>
            <form class="relationship-add-form" method="post" action=add_action on:submit=add_submit>
                <input
                    type="number"
                    min="1"
                    name="target_work_item_id"
                    placeholder="target item id"
                    required
                    prop:value=move || target_item_id.get()
                    on:input=move |event| {
                        set_target_item_id.set(event_target_value(&event));
                    }
                />
                <input
                    name="kind"
                    placeholder="kind"
                    required
                    prop:value=move || relationship_kind.get()
                    on:input=move |event| {
                        set_relationship_kind.set(event_target_value(&event));
                    }
                />
                <button disabled=move || add_pending.get()>"Add relationship"</button>
            </form>
        </section>
    }
    .into_any()
    }

    fn item_relationship_row(
        project: &str,
        item_id: i64,
        entry: WorkItemRelationshipListEntry,
        refresh: Callback<()>,
    ) -> impl IntoView + 'static {
        let relationship = entry.relationship;
        let related = match entry.direction {
            WorkItemRelationshipDirection::Outgoing => relationship.target.clone(),
            WorkItemRelationshipDirection::Incoming => relationship.source.clone(),
        };
        let update_form_action = format!(
            "/projects/{}/items/{}/relationships/{}/update",
            encode_path(project),
            item_id,
            relationship.id
        );
        let delete_form_action = format!(
            "/projects/{}/items/{}/relationships/{}/delete",
            encode_path(project),
            item_id,
            relationship.id
        );
        let (kind, set_kind) = signal(relationship.kind.clone());
        let service = item_service();
        let delete_service = service.clone();
        let relationship_id = relationship.id;
        let project_for_update = project.to_owned();
        let project_for_delete = project.to_owned();
        let update_mutation = mutation_action(
            refresh,
            || {},
            move |request: UpdateWorkItemRelationshipRequest| {
                let service = service.clone();
                let project = project_for_update.clone();
                async move {
                    service
                        .update_relationship(project, item_id, relationship_id, request)
                        .await
                }
            },
        );
        let update_pending = update_mutation.pending();
        let update_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if !update_pending.get_untracked() {
                update_mutation.dispatch(UpdateWorkItemRelationshipRequest {
                    kind: kind.get_untracked(),
                });
            }
        };
        let delete_mutation = mutation_action(
            refresh,
            || {},
            move |(): ()| {
                let service = delete_service.clone();
                let project = project_for_delete.clone();
                async move {
                    service
                        .delete_relationship(project, item_id, relationship_id)
                        .await
                }
            },
        );
        let delete_pending = delete_mutation.pending();
        let delete_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if !delete_pending.get_untracked() {
                delete_mutation.dispatch(());
            }
        };
        let direction = entry.direction.to_string();
        let related_href = item_href(project, related.id);
        let source_link = relationship_endpoint_link(project, &relationship.source);
        let target_link = relationship_endpoint_link(project, &relationship.target);
        let related_state = relationship_item_state_label(&related).to_owned();

        view! {
            <article class="relationship-row">
                <div class="relationship-main">
                    <span class="relationship-direction">{direction}</span>
                    <strong>{relationship.kind.clone()}</strong>
                    <p>
                        {source_link}
                        <span class="relationship-kind">" -- " {relationship.kind.clone()} " --> "</span>
                        {target_link}
                    </p>
                    <a class="relationship-related" href=related_href>
                        "#"{related.id} " [" {related_state} "] " {related.title}
                    </a>
                </div>
                <form method="post" action=update_form_action class="relationship-kind-form" on:submit=update_submit>
                    <input
                        name="kind"
                        required
                        prop:value=move || kind.get()
                        on:input=move |event| set_kind.set(event_target_value(&event))
                    />
                    <button disabled=move || update_pending.get()>"Update"</button>
                </form>
                <form method="post" action=delete_form_action on:submit=delete_submit>
                    <button class="danger" disabled=move || delete_pending.get()>"Delete"</button>
                </form>
            </article>
        }
    }

    fn relationship_endpoint_link(
        project: &str,
        item: &WorkItemRelationshipItemSummary,
    ) -> impl IntoView + 'static + use<> {
        let href = item_href(project, item.id);
        let state = relationship_item_state_label(item).to_owned();
        let title = item.title.clone();
        let id = item.id;
        view! {
            <a href=href>"#"{id} " [" {state} "] " {title}</a>
        }
    }

    fn relationship_item_state_label(item: &WorkItemRelationshipItemSummary) -> &str {
        item.state.as_deref().unwrap_or("(no state)")
    }

    fn item_labels_view(
        project: &str,
        item: &WorkItemView,
        suggestions: Vec<ProjectLabelView>,
        refresh: Callback<()>,
    ) -> AnyView {
        let add_action = format!(
            "/projects/{}/items/{}/labels",
            encode_path(project),
            item.id
        );
        let suggestion_options = label_suggestion_options(&suggestions);
        let state_options = use_context::<WorkItemStatesContext>()
            .map(|context| context.states)
            .expect("work item states context should be provided before rendering item labels");
        let state_suggestions = state_options.get_untracked();
        let state_suggestion_options = state_suggestion_options(&state_suggestions);
        let (label_key, set_label_key) = signal(String::new());
        let (label_value, set_label_value) = signal(String::new());
        let service = item_service();
        let project_for_add = project.to_owned();
        let item_id = item.id;
        let item_version = item.version;
        let add_mutation = mutation_action(
            refresh,
            move || {
                set_label_key.set(String::new());
                set_label_value.set(String::new());
            },
            move |request: CreateWorkItemLabelRequest| {
                let service = service.clone();
                let project = project_for_add.clone();
                async move {
                    service
                        .add_label(project, item_id, request, item_version)
                        .await
                }
            },
        );
        let add_pending = add_mutation.pending();
        let add_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if !add_pending.get_untracked() {
                add_mutation.dispatch(CreateWorkItemLabelRequest {
                    key: label_key.get_untracked(),
                    value: Some(label_value.get_untracked()),
                });
            }
        };
        let rows = item
            .labels
            .iter()
            .cloned()
            .map(|label| item_label_row(project, item, label, state_options, refresh))
            .collect::<Vec<_>>();

        view! {
            <section class="item-labels panel">
                <h2>"Labels"</h2>
                <datalist id="label-key-suggestions">{suggestion_options}</datalist>
                <datalist id="state-value-suggestions">{state_suggestion_options}</datalist>
                <div class="label-list">{rows}</div>
                <form class="label-add-form" method="post" action=add_action on:submit=add_submit>
                    <input type="hidden" name="version" value=item.version.to_string()/>
                    <input
                        name="key"
                        list="label-key-suggestions"
                        placeholder="key"
                        required
                        prop:value=move || label_key.get()
                        on:input=move |event| set_label_key.set(event_target_value(&event))
                    />
                    <input
                        name="value"
                        list="state-value-suggestions"
                        placeholder="value"
                        prop:value=move || label_value.get()
                        on:input=move |event| set_label_value.set(event_target_value(&event))
                    />
                    <button disabled=move || add_pending.get()>"Add label"</button>
                </form>
            </section>
        }
        .into_any()
    }

    fn item_label_row(
        project: &str,
        item: &WorkItemView,
        label: WorkItemLabelView,
        work_item_states: ReadSignal<Vec<WorkItemStateView>>,
        refresh: Callback<()>,
    ) -> impl IntoView + 'static {
        let is_state = label.key == STATE_LABEL_KEY;
        let update_form_action = if is_state {
            format!("/projects/{}/items/{}/move", encode_path(project), item.id)
        } else {
            format!(
                "/projects/{}/items/{}/labels/{}/update",
                encode_path(project),
                item.id,
                label.id
            )
        };
        let delete_form_action = format!(
            "/projects/{}/items/{}/labels/{}/delete",
            encode_path(project),
            item.id,
            label.id
        );
        let initial_value = label.value.clone().unwrap_or_default();
        let rendered = format_label(&label.key, label.value.as_deref());
        let can_delete = label.key != STATE_LABEL_KEY;
        let blocked = label.key == AUTOMATION_BLOCKED_LABEL_KEY;
        let feedback_requested = label.key == FEEDBACK_REQUESTED_LABEL_KEY;
        let (key, set_key) = signal(label.key.clone());
        let (value, set_value) = signal(initial_value);
        let service = item_service();
        let delete_service = service.clone();
        let project_for_update = project.to_owned();
        let project_for_delete = project.to_owned();
        let item_id = item.id;
        let item_version = item.version;
        let label_id = label.id;
        let update_mutation = mutation_action(
            refresh,
            || {},
            move |(key, value): (String, String)| {
                let service = service.clone();
                let project = project_for_update.clone();
                async move {
                    if is_state {
                        service
                            .move_item(project, item_id, value, item_version)
                            .await
                    } else {
                        service
                            .update_label(
                                project,
                                item_id,
                                label_id,
                                UpdateWorkItemLabelRequest {
                                    key: Some(key),
                                    value: Some(Some(value)),
                                    expect_version: Some(item_version),
                                },
                            )
                            .await
                    }
                }
            },
        );
        let update_pending = update_mutation.pending();
        let update_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if !update_pending.get_untracked() {
                update_mutation.dispatch((key.get_untracked(), value.get_untracked()));
            }
        };
        let delete_mutation = mutation_action(
            refresh,
            || {},
            move |(): ()| {
                let service = delete_service.clone();
                let project = project_for_delete.clone();
                async move {
                    service
                        .delete_label(project, item_id, label_id, item_version)
                        .await
                }
            },
        );
        let delete_pending = delete_mutation.pending();
        let delete_submit = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            if !delete_pending.get_untracked() {
                delete_mutation.dispatch(());
            }
        };

        view! {
            <article class="label-row">
                <span
                    class="label-chip"
                    class:blocked=blocked
                    class:feedback=feedback_requested
                >
                    {rendered}
                </span>
                <form
                    method="post"
                    action=update_form_action
                    class=if is_state { "state-label-form" } else { "" }
                    on:submit=update_submit
                >
                    <input type="hidden" name="version" value=item.version.to_string()/>
                    {if is_state {
                        view! {
                            <input type="hidden" name="key" value=STATE_LABEL_KEY/>
                            <select
                                name="value"
                                class="state-label-select"
                                required
                                on:change=move |event| set_value.set(event_target_value(&event))
                            >
                                {move || state_label_option_views(work_item_states.get(), value.get())}
                            </select>
                        }
                        .into_any()
                    } else {
                        view! {
                            <input
                                name="key"
                                required
                                prop:value=move || key.get()
                                on:input=move |event| set_key.set(event_target_value(&event))
                            />
                            <input
                                name="value"
                                prop:value=move || value.get()
                                on:input=move |event| set_value.set(event_target_value(&event))
                            />
                        }
                        .into_any()
                    }}
                    <button disabled=move || update_pending.get()>"Update"</button>
                </form>
                {can_delete.then(|| view! {
                    <form method="post" action=delete_form_action on:submit=delete_submit>
                        <input type="hidden" name="version" value=item.version.to_string()/>
                        <button class="danger" disabled=move || delete_pending.get()>"Delete"</button>
                    </form>
                })}
            </article>
        }
    }

    fn state_label_option_views(
        states: Vec<WorkItemStateView>,
        current_value: String,
    ) -> Vec<AnyView> {
        let mut has_current = false;
        let mut options = Vec::new();
        for state in states {
            let selected = state.identifier == current_value;
            has_current |= selected;
            options.push(
                view! {
                    <option value=state.identifier selected=selected>
                        {state.name}
                    </option>
                }
                .into_any(),
            );
        }

        if !current_value.is_empty() && !has_current {
            let label = format!("{current_value} (unknown)");
            options.push(
                view! {
                    <option value=current_value selected=true>{label}</option>
                }
                .into_any(),
            );
        }

        if options.is_empty() {
            options.push(
                view! {
                    <option value="" selected=true>"No states available"</option>
                }
                .into_any(),
            );
        }

        options
    }

    fn label_suggestion_options(suggestions: &[ProjectLabelView]) -> Vec<impl IntoView> {
        let mut keys = suggestions
            .iter()
            .map(|label| label.key.clone())
            .collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .map(|key| view! { <option value=key></option> })
            .collect()
    }

    fn state_suggestion_options(states: &[WorkItemStateView]) -> Vec<impl IntoView> {
        states
            .iter()
            .map(|state| state.identifier.clone())
            .map(|value| view! { <option value=value></option> })
            .collect()
    }

    fn automation_runs_view(project: &str, runs: Vec<AgentRunView>) -> AnyView {
        if runs.is_empty() {
            return ().into_any();
        }

        let run_items = runs
            .into_iter()
            .map(|run| {
                let href = format!(
                    "/projects/{}/automation/runs/{}/log",
                    encode_path(project),
                    run.id
                );
                let tokens = run.token_usage.map(run_token_usage_label);
                view! {
                    <li>
                        <a href=href>"#" {run.id}</a>
                        " · "
                        {run.status.to_string()}
                        " · "
                        {run.mutability.to_string()}
                        {tokens.map(|tokens| view! {
                            <>
                                " · "
                                {tokens}
                            </>
                        })}
                        " · "
                        {run.result_summary}
                    </li>
                }
            })
            .collect::<Vec<_>>();

        view! {
            <section class="item-automation">
                <h2>"Automation runs"</h2>
                <ul class="compact-list">{run_items}</ul>
            </section>
        }
        .into_any()
    }

    #[cfg(test)]
    mod tests {
        use super::infer_dispatch_run_id;

        #[test]
        fn infers_run_id_from_dispatch_agent_name() {
            assert_eq!(infer_dispatch_run_id("dispatch-run-60"), Some(60));
        }

        #[test]
        fn ignores_non_run_agent_names() {
            assert_eq!(infer_dispatch_run_id("codex"), None);
            assert_eq!(infer_dispatch_run_id("dispatch-run-"), None);
            assert_eq!(infer_dispatch_run_id("dispatch-run-0"), None);
            assert_eq!(infer_dispatch_run_id("dispatch-run-+60"), None);
            assert_eq!(infer_dispatch_run_id("dispatch-run-abc"), None);
        }
    }
}

pub(crate) use content::{infer_dispatch_run_id, item_content};
