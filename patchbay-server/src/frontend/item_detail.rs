use crate::{
    frontend::{
        app::{
            ActivePage, ItemPage, WorkItemStatesContext, background_form_submit, claim_badge,
            encode_path, format_label, item_href, provide_work_item_states_context,
            run_token_usage_label, state_label, top_bar,
        },
        crudkit::{crudkit_i64_id, work_items_crudkit_config_for_view},
    },
    shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, AgentRunView, AuthorType, DEFAULT_STATE_LABEL,
        FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView, STATE_LABEL_KEY, WorkItemLabelView,
        WorkItemRelationshipDirection, WorkItemRelationshipItemSummary,
        WorkItemRelationshipListEntry, WorkItemStateView, WorkItemView,
    },
};
use crudkit_leptos::crud_instance::CrudInstanceContext;
use crudkit_leptos::{
    crud_instance_config::CrudNavigationConfig, crudkit_web::view::SerializableCrudView, prelude::*,
};
use leptos::prelude::*;
use leptos_router::{NavigateOptions, hooks::use_navigate};

pub(crate) fn item_content(page: ItemPage) -> AnyView {
    let ItemPage {
        projects,
        active_project_names,
        project,
        item,
        comments,
        relationships,
        label_suggestions,
        work_item_states,
        automation_runs,
        api_base_url,
        codex_status,
    } = page;
    provide_work_item_states_context(work_item_states);
    let topbar = top_bar(
        projects,
        active_project_names,
        Some(project.clone()),
        ActivePage::Board,
        None,
        codex_status,
    );
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
    let comment_submit = background_form_submit(true);
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
    let labels = item_labels_view(&project, &item, label_suggestions);
    let relationship_views = item_relationships_view(&project, &item, relationships);

    view! {
        <div>
            {topbar}
            <main class="page-shell item-page">
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
                        <input name="author_name" placeholder="Your name"/>
                        <textarea name="body" placeholder="Comment" required></textarea>
                        <button>"Add comment"</button>
                    </form>
                </section>
            </main>
        </div>
    }
    .into_any()
}

fn without_crudkit_field<F: TypeErasedField>(
    elements: Vec<Elem<F>>,
    field_name: &str,
) -> Vec<Elem<F>> {
    elements
        .into_iter()
        .filter_map(|element| match element {
            Elem::Field((field, _)) if field.name() == field_name => None,
            Elem::Enclosing(enclosing) => Some(Elem::Enclosing(without_crudkit_enclosing_field(
                enclosing, field_name,
            ))),
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
        && let Some(run_id) = infer_patchbay_run_id(&author)
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

pub(crate) fn infer_patchbay_run_id(agent_id: &str) -> Option<i64> {
    let id = agent_id.strip_prefix("patchbay-run-")?;
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
) -> AnyView {
    let add_action = format!(
        "/projects/{}/items/{}/relationships",
        encode_path(project),
        item.id
    );
    let add_submit = background_form_submit(true);
    let rows = relationships
        .into_iter()
        .map(|entry| item_relationship_row(project, item.id, entry))
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
                />
                <input name="kind" placeholder="kind" required/>
                <button>"Add relationship"</button>
            </form>
        </section>
    }
    .into_any()
}

fn item_relationship_row(
    project: &str,
    item_id: i64,
    entry: WorkItemRelationshipListEntry,
) -> impl IntoView + 'static {
    let relationship = entry.relationship;
    let related = match entry.direction {
        WorkItemRelationshipDirection::Outgoing => relationship.target.clone(),
        WorkItemRelationshipDirection::Incoming => relationship.source.clone(),
    };
    let update_action = format!(
        "/projects/{}/items/{}/relationships/{}/update",
        encode_path(project),
        item_id,
        relationship.id
    );
    let delete_action = format!(
        "/projects/{}/items/{}/relationships/{}/delete",
        encode_path(project),
        item_id,
        relationship.id
    );
    let update_submit = background_form_submit(false);
    let delete_submit = background_form_submit(false);
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
            <form method="post" action=update_action class="relationship-kind-form" on:submit=update_submit>
                <input name="kind" value=relationship.kind required/>
                <button>"Update"</button>
            </form>
            <form method="post" action=delete_action on:submit=delete_submit>
                <button class="danger">"Delete"</button>
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
    let add_submit = background_form_submit(true);
    let rows = item
        .labels
        .iter()
        .cloned()
        .map(|label| item_label_row(project, item, label, state_options))
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
                />
                <input
                    name="value"
                    list="state-value-suggestions"
                    placeholder="value"
                />
                <button>"Add label"</button>
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
) -> impl IntoView + 'static {
    let update_action = format!(
        "/projects/{}/items/{}/labels/{}/update",
        encode_path(project),
        item.id,
        label.id
    );
    let delete_action = format!(
        "/projects/{}/items/{}/labels/{}/delete",
        encode_path(project),
        item.id,
        label.id
    );
    let value = label.value.clone().unwrap_or_default();
    let rendered = format_label(&label.key, label.value.as_deref());
    let is_state = label.key == STATE_LABEL_KEY;
    let can_delete = label.key != STATE_LABEL_KEY;
    let blocked = label.key == AUTOMATION_BLOCKED_LABEL_KEY;
    let feedback_requested = label.key == FEEDBACK_REQUESTED_LABEL_KEY;
    let update_submit = background_form_submit(false);
    let delete_submit = background_form_submit(false);

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
                action=update_action
                class=if is_state { "state-label-form" } else { "" }
                on:submit=update_submit
            >
                <input type="hidden" name="version" value=item.version.to_string()/>
                {if is_state {
                    let value = value.clone();
                    view! {
                        <input type="hidden" name="key" value=STATE_LABEL_KEY/>
                        <select name="value" class="state-label-select" required>
                            {move || state_label_option_views(work_item_states.get(), value.clone())}
                        </select>
                    }
                    .into_any()
                } else {
                    view! {
                        <input name="key" value=label.key required/>
                        <input name="value" value=value/>
                    }
                    .into_any()
                }}
                <button>"Update"</button>
            </form>
            {can_delete.then(|| view! {
                <form method="post" action=delete_action on:submit=delete_submit>
                    <input type="hidden" name="version" value=item.version.to_string()/>
                    <button class="danger">"Delete"</button>
                </form>
            })}
        </article>
    }
}

fn state_label_option_views(states: Vec<WorkItemStateView>, current_value: String) -> Vec<AnyView> {
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
    use super::infer_patchbay_run_id;

    #[test]
    fn infers_run_id_from_patchbay_agent_name() {
        assert_eq!(infer_patchbay_run_id("patchbay-run-60"), Some(60));
    }

    #[test]
    fn ignores_non_run_agent_names() {
        assert_eq!(infer_patchbay_run_id("codex"), None);
        assert_eq!(infer_patchbay_run_id("patchbay-run-"), None);
        assert_eq!(infer_patchbay_run_id("patchbay-run-0"), None);
        assert_eq!(infer_patchbay_run_id("patchbay-run-+60"), None);
        assert_eq!(infer_patchbay_run_id("patchbay-run-abc"), None);
    }
}
