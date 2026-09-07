use crate::frontend::queries::query_resource;
use crate::frontend::{
    items::components::encode_path,
    live_events::{item_event_matches, refetch_on_live_event},
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params;

#[component]
pub fn PageItem() -> impl IntoView {
    let params = use_params::<crate::frontend::routes::ItemParams>();
    let project = Memo::new(move |_| params.get().ok().and_then(|params| params.project));
    let item_id = Memo::new(move |_| {
        params
            .get()
            .ok()
            .and_then(|params| params.item_id)
            .filter(|id| *id > 0)
    });
    let store = crate::frontend::items::store::item_store();
    let initial = store.cached_page_untracked(&project.get_untracked(), item_id.get_untracked());
    let store_for_cache = store.clone();
    let store_for_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let result = query_resource(
        initial,
        move || (project.get(), item_id.get()),
        move |(project, item_id)| store_for_cache.cached_page(project, *item_id),
        move |(project, item_id)| {
            let store = store_for_load.clone();
            let project = project.clone();
            async move { store.load_page(project, item_id).await }
        },
        move |(project, id), value| store_for_seed.seed_page(project, id, value),
        move |(project, id)| store_for_refresh.invalidate_page(project, id),
    );
    refetch_on_live_event(result.refresh, move |event| {
        item_event_matches(event, project.get().as_deref(), item_id.get())
    });
    let (interactive, set_interactive) = signal(false);
    Effect::new(move |_| set_interactive.set(true));
    let board_href = move || {
        project
            .get()
            .as_deref()
            .map(|project| format!("/?project={}", encode_path(project)))
            .unwrap_or_else(|| "/".to_owned())
    };
    let fallback_title = move || {
        item_id
            .get()
            .map(|item_id| format!("#{item_id}"))
            .unwrap_or_else(|| "Work item".to_owned())
    };
    view! {
            <Title text="Dispatch"/>
            <div>
                <main class="page-shell item-page">
                    <Transition fallback=move || view! {
                        <section class="item-header"><a class="item-board-link" href=board_href>"Board"</a><h1>{fallback_title}</h1></section>
                    }>
                        <For
                            each=move || result.value.get()
                            key=|page| (page.item.project_id, page.item.id)
                            children=move |initial| {
                                let identity = (initial.item.project_id, initial.item.id);
                                let page = Signal::derive(move || result.value.get().filter(|page| (page.item.project_id, page.item.id) == identity).unwrap_or_else(|| initial.clone()));
                                view! { <ItemContent page refresh=result.refresh interactive/> }
                            }
                        />
    </Transition>
    <crate::frontend::components::QueryFeedback pending=result.pending error=result.error refresh=result.refresh/>
                </main>
            </div>
        }
}

mod content {
    use std::future::Future;

    use crate::{
        frontend::{
            crudkit::crudkit_i64_id,
            items::{
                components::{
                    WorkItemStatesContext, claim_badge, encode_path, item_href, state_label,
                },
                crud::work_items_crudkit_config_for_view,
                service::item_service,
                types::ItemDetail,
            },
            runs::panels::run_token_usage_label,
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
        crud_instance_config::CrudBuiltinViewControls, crudkit_web::view::CrudView, prelude::*,
    };
    use leptos::prelude::*;
    use leptos_router::{NavigateOptions, hooks::use_navigate};

    #[component]
    pub(crate) fn ItemContent(
        page: Signal<ItemDetail>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
    ) -> impl IntoView {
        let project = page.get_untracked().project;
        let header_title =
            move || page.with(|page| format!("#{} {}", page.item.id, page.item.title));
        let board_href = format!("/?project={}", encode_path(&project));
        let navigate = use_navigate();
        let board_href_for_exit = board_href.clone();
        let exit_to_board = Callback::new(move |()| {
            navigate(&board_href_for_exit, NavigateOptions::default());
        });
        let (item_editor_context, set_item_editor_context) = signal(None::<CrudInstanceContext>);
        view! {
            <section class="item-header">
                <button
                    type="button"
                    class="link-button item-board-link"
                    on:click=move |_| {
                        if let Some(context) = item_editor_context.get_untracked() {
                            context.navigation.return_from_current();
                        } else {
                            exit_to_board.run(());
                        }
                    }
                >
                    "Board"
                </button>
                <h1>{header_title}</h1>
            </section>
            <ItemDetailContent
                page
                refresh
                interactive
                on_return=exit_to_board
                on_context_created=Callback::new(move |context| {
                    set_item_editor_context.set(Some(context));
                })
                on_run_click=None
            />
        }
    }

    #[component]
    pub(crate) fn ItemDetailContent(
        page: Signal<ItemDetail>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
        on_return: Callback<()>,
        on_context_created: Callback<CrudInstanceContext>,
        on_run_click: Option<Callback<(leptos::ev::MouseEvent, i64)>>,
    ) -> impl IntoView {
        let api_base_url = crate::frontend::http::HttpService::get().api_base_url();
        let ItemDetail {
            project,
            item,
            comments: _,
            relationships: _,
            label_suggestions: _,
            work_item_states: _,
            automation_runs: _,
        } = page.get_untracked();
        crate::frontend::items::components::provide_work_item_states_signal(Signal::derive(
            move || page.get().work_item_states,
        ));
        let item_project_id = item.project_id;
        let item_id = item.id;
        let editor_default_create_state = Signal::derive(|| DEFAULT_STATE_LABEL.to_owned());
        let item_detail_controls = CrudBuiltinViewControls {
            show_delete: true,
            ..CrudBuiltinViewControls::embedded_single_entity()
        };
        let initial_view = CrudView::edit(crudkit_i64_id(item_id));
        let mut item_detail_config = work_items_crudkit_config_for_view(
            api_base_url,
            item_project_id,
            initial_view,
            item_detail_controls,
            editor_default_create_state,
            None,
            Signal::derive(Vec::<ProjectLabelView>::new),
        );
        item_detail_config.elements = without_crudkit_field(item_detail_config.elements, "id");
        let instance_created = Callback::new(move |context: CrudInstanceContext| {
            context.navigation.return_with(move || on_return.run(()));
            on_context_created.run(context);
        });
        let item_editor = view! {
            <div class="crudkit-item-detail" data-crudkit-leptos="work-item-detail">
                <CrudInstance
                    name="work-item-detail"
                    config=item_detail_config
                    on_context_created=instance_created
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
        let comment_click = move |_| {
            if comment_pending.get_untracked() {
                return;
            }
            comment_mutation.dispatch(AddCommentRequest {
                author_type: AuthorType::User,
                author_name: Some(comment_author.get_untracked()),
                body: comment_body.get_untracked(),
            });
        };
        let project_for_runs = project.clone();
        let automation_run_views = move || {
            view! {
                <AutomationRuns project=project_for_runs.clone() runs=page.get().automation_runs on_run_click/>
            }
        };
        let project_for_comments = project.clone();
        let comment_views = move || {
            let project = &project_for_comments;
            page.get()
                .comments
                .into_iter()
                .map(|comment| {
                    let author = comment
                        .author_name
                        .unwrap_or_else(|| comment.author_type.as_storage().to_owned());
                    let author = comment_author_view(project, comment.author_type, author);
                    view! {
                        <article>
                            <strong>{author}</strong>
                            <span>{comment.created_at}</span>
                            <p>{comment.body}</p>
                        </article>
                    }
                })
                .collect::<Vec<_>>()
        };
        let item = Signal::derive(move || page.get().item);
        let labels = view! {
            <ItemLabels
                project=project.clone()
                item
                suggestions=Signal::derive(move || page.get().label_suggestions)
                refresh
                interactive
            />
        };
        let relationship_views = view! {
            <ItemRelationships
                project=project.clone()
                item
                relationships=Signal::derive(move || page.get().relationships)
                refresh
                interactive
            />
        };

        view! {
            <div class="item-detail-content">
                {move || view! { <ItemMetadata page=page.get() on_run_click/> }}
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
                    <div class="comment-add-controls">
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
                        <button
                            type="button"
                            disabled=move || !interactive.get() || comment_pending.get()
                            on:click=comment_click
                        >
                            "Add comment"
                        </button>
                    </div>
                </section>
            </div>
        }
        .into_any()
    }

    #[component]
    fn ItemMetadata(
        page: ItemDetail,
        on_run_click: Option<Callback<(leptos::ev::MouseEvent, i64)>>,
    ) -> impl IntoView {
        let project = page.project;
        let item = page.item;
        let item_state_display = state_label(&item).to_owned();
        let claim = item
            .claimed_by
            .clone()
            .map(|agent| claim_badge(&project, agent, "Claimed", item.claimed_at.clone()));
        let finished = item.finished_at.clone().map(|finished_at| {
            view! { <span>"finished " {finished_at}</span> }
        });
        let work_group = item.work_group.clone().map(|group| {
            view! {
                <span class="item-work-group" data-work-group-key=group.key.clone()>
                    "group " <strong>{group.name}</strong> <code>{group.key.clone()}</code>
                </span>
            }
        });
        let item_origin = item.origin.clone().map(|origin| {
            let run_link = origin.agent_run_id.map(|run_id| {
                let href = crate::frontend::routes::routes::RunLog.materialize(encode_path(&project), run_id);
                view! {
                    <a
                        href=href
                        on:click=move |event| {
                            if let Some(on_run_click) = on_run_click {
                                on_run_click.run((event, run_id));
                            }
                        }
                    >
                        "run #" {run_id}
                    </a>
                }
            });
            let automation_link = origin.trigger_name.clone().map(|name| {
                let href = format!("/automation?project={}", encode_path(&project));
                view! { <a href=href>{name}</a> }
            });
            view! {
                <section class="item-origin panel" data-testid="item-origin">
                    <h2>"Origin"</h2>
                    <dl>
                        <dt>"kind"</dt>
                        <dd>{origin.kind.as_storage()}</dd>
                        {run_link.map(|link| view! { <><dt>"run"</dt><dd>{link}</dd></> })}
                        {automation_link.map(|link| view! { <><dt>"automation"</dt><dd>{link}</dd></> })}
                        {origin.trigger_revision_id.map(|id| view! {
                            <><dt>"trigger revision"</dt><dd>"#" {id}</dd></>
                        })}
                        {origin.producing_evaluation_id.map(|id| view! {
                            <><dt>"evaluation"</dt><dd>"#" {id}</dd></>
                        })}
                        {origin.bundle_key.map(|key| view! {
                            <><dt>"bundle"</dt><dd>{key}</dd></>
                        })}
                    </dl>
                </section>
            }
        });
        view! {
            <section class="item-meta">
                <span>{item_state_display}</span><span>"v" {item.version}</span>
                {claim}{finished}{work_group}
            </section>
            {item_origin}
        }
    }

    fn mutation_action<Input, Mutate, MutationFuture, OnSuccess>(
        refresh: Callback<()>,
        on_success: OnSuccess,
        mutate: Mutate,
    ) -> Action<Input, Result<(), ServerFnError>>
    where
        Input: Clone + Send + Sync + 'static,
        Mutate: Fn(Input) -> MutationFuture + Clone + Send + Sync + 'static,
        MutationFuture: Future<Output = Result<(), ServerFnError>> + Send + 'static,
        OnSuccess: Fn() + 'static,
    {
        let action = Action::new(move |input: &Input| {
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
            let href =
                crate::frontend::routes::routes::RunLog.materialize(encode_path(project), run_id);
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

    #[component]
    fn ItemRelationships(
        project: String,
        item: Signal<WorkItemView>,
        relationships: Signal<Vec<WorkItemRelationshipListEntry>>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
    ) -> AnyView {
        let (target_item_id, set_target_item_id) = signal(String::new());
        let (relationship_kind, set_relationship_kind) = signal(String::new());
        let service = item_service();
        let project_for_add = project.clone();
        let item_id = item.get_untracked().id;
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
        let add_click = move |_| {
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
        let rows = view! { <For each=move || relationships.get() key=|entry| entry.relationship.id children=move |initial| {
            let id = initial.relationship.id;
            let entry = Signal::derive(move || relationships.get().into_iter().find(|entry| entry.relationship.id == id).unwrap_or_else(|| initial.clone()));
            view! { <ItemRelationshipRow project=project.clone() item_id entry refresh interactive/> }
        }/> };
        let empty = move || {
            relationships
                .get()
                .is_empty()
                .then(|| view! { <p class="muted">"No relationships"</p> })
        };

        view! {
            <section class="item-relationships panel">
                <h2>"Relationships"</h2>
                <div class="relationship-list">
                    {empty}
                    {rows}
                </div>
                <div class="relationship-add-controls">
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
                    <button
                        type="button"
                        disabled=move || !interactive.get() || add_pending.get()
                        on:click=add_click
                    >
                        "Add relationship"
                    </button>
                </div>
            </section>
        }
        .into_any()
    }

    #[component]
    fn ItemRelationshipRow(
        project: String,
        item_id: i64,
        entry: Signal<WorkItemRelationshipListEntry>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
    ) -> impl IntoView + 'static {
        let initial = entry.get_untracked();
        let relationship = initial.relationship;
        let related = match initial.direction {
            WorkItemRelationshipDirection::Outgoing => relationship.target.clone(),
            WorkItemRelationshipDirection::Incoming => relationship.source.clone(),
        };
        let (kind, set_kind) = signal(relationship.kind.clone());
        let baseline = RwSignal::new(relationship.kind.clone());
        Effect::new(move |_| {
            let saved = entry.get().relationship.kind;
            if kind.get_untracked() == baseline.get_untracked() {
                set_kind.set(saved.clone());
            }
            baseline.set(saved);
        });
        let service = item_service();
        let delete_service = service.clone();
        let relationship_id = relationship.id;
        let project_for_update = project.clone();
        let project_for_delete = project.clone();
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
        let update_click = move |_| {
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
        let delete_click = move |_| {
            if !delete_pending.get_untracked() {
                delete_mutation.dispatch(());
            }
        };
        let direction = initial.direction.to_string();
        let related_href = item_href(&project, related.id);
        let source = relationship.source.clone();
        let target = relationship.target.clone();
        let related_state = relationship_item_state_label(&related).to_owned();

        view! {
            <article class="relationship-row">
                <div class="relationship-main">
                    <span class="relationship-direction">{direction}</span>
                    <strong>{move || entry.get().relationship.kind}</strong>
                    <p>
                        <RelationshipEndpointLink project=project.clone() item=source/>
                        <span class="relationship-kind">" -- " {move || entry.get().relationship.kind} " --> "</span>
                        <RelationshipEndpointLink project=project.clone() item=target/>
                    </p>
                    <a class="relationship-related" href=related_href>
                        "#"{related.id} " [" {related_state} "] " {related.title}
                    </a>
                </div>
                <div class="relationship-kind-controls">
                    <input
                        name="kind"
                        required
                        prop:value=move || kind.get()
                        on:input=move |event| set_kind.set(event_target_value(&event))
                    />
                    <button
                        type="button"
                        disabled=move || !interactive.get() || update_pending.get()
                        on:click=update_click
                    >
                        "Update"
                    </button>
                </div>
                <div class="relationship-delete-controls">
                    <button
                        type="button"
                        class="danger"
                        disabled=move || !interactive.get() || delete_pending.get()
                        on:click=delete_click
                    >
                        "Delete"
                    </button>
                </div>
            </article>
        }
    }

    #[component]
    fn RelationshipEndpointLink(
        project: String,
        item: WorkItemRelationshipItemSummary,
    ) -> impl IntoView {
        let href = item_href(&project, item.id);
        let state = relationship_item_state_label(&item).to_owned();
        let title = item.title;
        let id = item.id;
        view! {
            <a href=href>"#"{id} " [" {state} "] " {title}</a>
        }
    }

    fn relationship_item_state_label(item: &WorkItemRelationshipItemSummary) -> &str {
        item.state.as_deref().unwrap_or("(no state)")
    }

    #[component]
    fn ItemLabels(
        project: String,
        item: Signal<WorkItemView>,
        suggestions: Signal<Vec<ProjectLabelView>>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
    ) -> AnyView {
        let suggestion_options = move || label_suggestion_options(&suggestions.get());
        let state_options = use_context::<WorkItemStatesContext>()
            .map(|context| context.states)
            .expect("work item states context should be provided before rendering item labels");
        let state_suggestions = state_options.get_untracked();
        let state_suggestion_options = state_suggestion_options(&state_suggestions);
        let (label_key, set_label_key) = signal(String::new());
        let (label_value, set_label_value) = signal(String::new());
        let service = item_service();
        let project_for_add = project.clone();
        let item_id = item.get_untracked().id;
        let item_version = move || item.with_untracked(|item| item.version);
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
                        .add_label(project, item_id, request, item_version())
                        .await
                }
            },
        );
        let add_pending = add_mutation.pending();
        let add_click = move |_| {
            if !add_pending.get_untracked() {
                add_mutation.dispatch(CreateWorkItemLabelRequest {
                    key: label_key.get_untracked(),
                    value: Some(label_value.get_untracked()),
                });
            }
        };
        let rows = view! { <For each=move || item.get().labels key=|label| (label.id, label.key == STATE_LABEL_KEY) children=move |initial| {
            let id = initial.id;
            let label = Signal::derive(move || item.get().labels.into_iter().find(|label| label.id == id).unwrap_or_else(|| initial.clone()));
            view! { <ItemLabelRow project=project.clone() item label work_item_states=state_options refresh interactive/> }
        }/> };

        view! {
            <section class="item-labels panel">
                <h2>"Labels"</h2>
                <datalist id="label-key-suggestions">{suggestion_options}</datalist>
                <datalist id="state-value-suggestions">{state_suggestion_options}</datalist>
                <div class="label-list">{rows}</div>
                <div class="label-add-controls">
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
                    <button
                        type="button"
                        disabled=move || !interactive.get() || add_pending.get()
                        on:click=add_click
                    >
                        "Add label"
                    </button>
                </div>
            </section>
        }
        .into_any()
    }

    #[component]
    fn ItemLabelRow(
        project: String,
        item: Signal<WorkItemView>,
        label: Signal<WorkItemLabelView>,
        work_item_states: Signal<Vec<WorkItemStateView>>,
        refresh: Callback<()>,
        interactive: ReadSignal<bool>,
    ) -> impl IntoView + 'static {
        let initial = label.get_untracked();
        let is_state = initial.key == STATE_LABEL_KEY;
        let initial_value = initial.value.clone().unwrap_or_default();
        let can_delete = initial.key != STATE_LABEL_KEY;
        let blocked = initial.key == AUTOMATION_BLOCKED_LABEL_KEY;
        let feedback_requested = initial.key == FEEDBACK_REQUESTED_LABEL_KEY;
        let row_key = initial.key.clone();
        let (key, set_key) = signal(initial.key.clone());
        let (value, set_value) = signal(initial_value);
        let baseline = RwSignal::new((
            initial.key.clone(),
            initial.value.clone().unwrap_or_default(),
        ));
        Effect::new(move |_| {
            let saved = label.get();
            let saved = (saved.key, saved.value.unwrap_or_default());
            if (key.get_untracked(), value.get_untracked()) == baseline.get_untracked() {
                set_key.set(saved.0.clone());
                set_value.set(saved.1.clone());
            }
            baseline.set(saved);
        });
        let service = item_service();
        let delete_service = service.clone();
        let project_for_update = project.clone();
        let project_for_delete = project;
        let item_id = item.get_untracked().id;
        let item_version = move || item.with_untracked(|item| item.version);
        let label_id = initial.id;
        let update_mutation = mutation_action(
            refresh,
            || {},
            move |(key, value): (String, String)| {
                let service = service.clone();
                let project = project_for_update.clone();
                async move {
                    if is_state {
                        service
                            .move_item(project, item_id, value, item_version())
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
                                    expect_version: Some(item_version()),
                                },
                            )
                            .await
                    }
                }
            },
        );
        let update_pending = update_mutation.pending();
        let update_click = move |_| {
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
                        .delete_label(project, item_id, label_id, item_version())
                        .await
                }
            },
        );
        let delete_pending = delete_mutation.pending();
        let delete_click = move |_| {
            if !delete_pending.get_untracked() {
                delete_mutation.dispatch(());
            }
        };

        view! {
            <article
                class="label-row"
                class:blocked=blocked
                class:feedback=feedback_requested
                data-label-key=row_key
            >
                <div
                    class=if is_state {
                        "label-update-controls state-label-controls"
                    } else {
                        "label-update-controls"
                    }
                >
                    {if is_state {
                        view! {
                            <div class="label-fixed-key">
                                <span>"Key"</span>
                                <strong>"State"</strong>
                            </div>
                            <label class="label-field">
                                <span>"Value"</span>
                                <select
                                    name="value"
                                    class="state-label-select"
                                    required
                                    on:change=move |event| set_value.set(event_target_value(&event))
                                >
                                    {move || state_label_option_views(work_item_states.get(), value.get())}
                                </select>
                            </label>
                        }
                        .into_any()
                    } else {
                        view! {
                            <label class="label-field">
                                <span>"Key"</span>
                                <input
                                    name="key"
                                    required
                                    prop:value=move || key.get()
                                    on:input=move |event| set_key.set(event_target_value(&event))
                                />
                            </label>
                            <label class="label-field">
                                <span>"Value"</span>
                                <input
                                    name="value"
                                    prop:value=move || value.get()
                                    on:input=move |event| set_value.set(event_target_value(&event))
                                />
                            </label>
                        }
                        .into_any()
                    }}
                    <div class="label-row-actions">
                        <button
                            type="button"
                            disabled=move || !interactive.get() || update_pending.get()
                            on:click=update_click
                        >
                            "Update"
                        </button>
                        {can_delete.then(move || view! {
                            <button
                                type="button"
                                class="danger label-delete-button"
                                disabled=move || !interactive.get() || delete_pending.get()
                                on:click=delete_click
                            >
                                "Delete"
                            </button>
                        })}
                    </div>
                </div>
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

    fn label_suggestion_options(suggestions: &[ProjectLabelView]) -> Vec<impl IntoView + use<>> {
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

    #[component]
    fn AutomationRuns(
        project: String,
        runs: Vec<AgentRunView>,
        on_run_click: Option<Callback<(leptos::ev::MouseEvent, i64)>>,
    ) -> AnyView {
        if runs.is_empty() {
            return ().into_any();
        }

        let run_items = runs
            .into_iter()
            .map(|run| {
                let href = crate::frontend::routes::routes::RunLog
                    .materialize(encode_path(&project), run.id);
                let tokens = run.token_usage.map(run_token_usage_label);
                view! {
                    <li>
                        <a
                            href=href
                            on:click=move |event| {
                                if let Some(on_run_click) = on_run_click {
                                    on_run_click.run((event, run.id));
                                }
                            }
                        >
                            "#" {run.id}
                        </a>
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
        use assertr::prelude::*;

        #[test]
        fn infers_run_id_from_dispatch_agent_name() {
            assert_that!(&(infer_dispatch_run_id("dispatch-run-60"))).is_equal_to(Some(60));
        }

        #[test]
        fn ignores_non_run_agent_names() {
            assert_that!(&(infer_dispatch_run_id("codex"))).is_equal_to(None);
            assert_that!(&(infer_dispatch_run_id("dispatch-run-"))).is_equal_to(None);
            assert_that!(&(infer_dispatch_run_id("dispatch-run-0"))).is_equal_to(None);
            assert_that!(&(infer_dispatch_run_id("dispatch-run-+60"))).is_equal_to(None);
            assert_that!(&(infer_dispatch_run_id("dispatch-run-abc"))).is_equal_to(None);
        }
    }
}

pub(crate) use content::{ItemContent, ItemDetailContent, infer_dispatch_run_id};
