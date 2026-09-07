use super::{
    BoardDrawerSelection, BoardItemView, BoardItemsSection, BoardRunPreview,
    intercepts_board_drawer_click,
};
use crate::{
    frontend::{
        items::{
            components::{
                claim_elapsed_timer, claim_source_label, encode_path, format_label, preview,
            },
            creation::{CreateItemOpenRequest, state_identifier_from_lane_filter},
            detail::infer_dispatch_run_id,
        },
        runs::panels::run_status_class,
    },
    shared::{
        label_conditions::ValidatedLabelCondition,
        view_models::{
            AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY, SwimLaneItemOrder,
            SwimLaneView, WorkItemGroupSummaryView,
        },
    },
};
use leptos::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

const BOARD_LANE_GAP_PX: f64 = 8.0;

type BoardItems = Memo<HashMap<i64, Arc<BoardItemView>>>;
type LabelAccents = Memo<Arc<BTreeMap<String, String>>>;

#[component]
pub(super) fn BoardView(
    project: String,
    section: Signal<Option<BoardItemsSection>>,
    open_create_item: Callback<CreateItemOpenRequest>,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    // One item snapshot is shared by every lane that includes it. Lane projections
    // contain only membership and presentation metadata, so item edits do not
    // invalidate whole lanes or groups.
    let items = Memo::new(move |_| {
        section.with(|section| {
            section
                .as_ref()
                .map(|section| {
                    section
                        .items
                        .iter()
                        .map(|item| (item.item.id, Arc::new(item.clone())))
                        .collect()
                })
                .unwrap_or_default()
        })
    });
    let lanes = Memo::new(move |_| {
        section.with(|section| {
            section
                .as_ref()
                .map(board_lane_data)
                .unwrap_or_default()
                .into_iter()
                .map(|lane| (lane.lane.id, lane))
                .collect::<HashMap<_, _>>()
        })
    });
    let lane_ids = Memo::new(move |_| {
        section.with(|section| {
            section
                .as_ref()
                .map(|section| {
                    section
                        .swim_lanes
                        .iter()
                        .map(|lane| lane.id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    });
    let label_accent_colors = Memo::new(move |_| {
        Arc::new(
            section
                .with(|section| {
                    section
                        .as_ref()
                        .map(|section| section.label_accent_colors.clone())
                })
                .unwrap_or_default(),
        )
    });
    let misconfigured_item_count = Memo::new(move |_| {
        section
            .with(|section| {
                section
                    .as_ref()
                    .map(|section| section.misconfigured_item_count)
            })
            .unwrap_or_default()
    });

    view! {
        <div class="board-stack">
            <section class="board" style=move || board_lane_width(lane_ids.with(Vec::len))>
                <For
                    each=move || lane_ids.get()
                    key=|id| *id
                    children=move |lane_id| {
                        let lane = Memo::new(move |_| {
                            lanes.with(|lanes| lanes.get(&lane_id).cloned())
                        });
                        view! {
                            <BoardLane
                                project=project.clone()
                                lane_id
                                lane
                                items
                                label_accent_colors
                                open_create_item
                                open_drawer
                            />
                        }
                    }
                />
            </section>
            {move || board_state_warning(misconfigured_item_count.get())}
        </div>
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BoardLaneData {
    lane: SwimLaneView,
    cards: LaneCards,
    item_count: usize,
}

fn board_lane_data(section: &BoardItemsSection) -> Vec<BoardLaneData> {
    section
        .swim_lanes
        .iter()
        .map(|lane| {
            let mut items = ValidatedLabelCondition::new(&lane.filter)
                .map(|condition| {
                    section
                        .items
                        .iter()
                        .filter(|item| condition.matches(&item.item.labels))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            sort_lane_items(&mut items, lane.item_order);
            BoardLaneData {
                lane: lane.clone(),
                item_count: items.len(),
                cards: group_lane_items(items),
            }
        })
        .collect()
}

fn board_lane_width(lane_count: usize) -> String {
    let lane_count = lane_count.max(1) as f64;
    format!(
        "--board-lane-width: calc({:.6}cqw - {:.3}px);",
        100.0 / lane_count,
        BOARD_LANE_GAP_PX * (lane_count - 1.0) / lane_count,
    )
}

#[component]
fn BoardLane(
    project: String,
    lane_id: i64,
    lane: Memo<Option<BoardLaneData>>,
    items: BoardItems,
    label_accent_colors: LabelAccents,
    open_create_item: Callback<CreateItemOpenRequest>,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    let name = Memo::new(move |_| {
        lane.with(|lane| {
            lane.as_ref()
                .map(|lane| lane.lane.name.clone())
                .unwrap_or_default()
        })
    });
    let create_state = Memo::new(move |_| {
        lane.with(|lane| {
            lane.as_ref()
                .filter(|lane| lane.lane.can_create_items)
                .and_then(|lane| state_identifier_from_lane_filter(&lane.lane.filter))
        })
    });
    let rows = Memo::new(move |_| {
        lane.with(|lane| {
            lane.as_ref()
                .map(|lane| lane.cards.order.clone())
                .unwrap_or_default()
        })
    });
    let edit_href = lane_edit_href(&project, lane_id);
    view! {
        <section class="lane">
            <header class="lane-header">
                <div class="lane-heading">
                    <h2>{move || name.get()}</h2>
                    <span class="lane-count">{move || lane.with(|lane| lane.as_ref().map(|lane| lane.item_count).unwrap_or_default())}</span>
                </div>
                <div class="lane-actions">
                    <Show when=move || create_state.with(Option::is_some)>
                        <button type="button" class="lane-add" on:click=move |_| {
                            if let Some(state) = create_state.get_untracked() {
                                open_create_item.run(CreateItemOpenRequest::SingleState(state));
                            }
                        }>
                            "+ Add"
                        </button>
                    </Show>
                    <a class="lane-edit" href=edit_href
                        title=move || format!("Edit {}", name.get())
                        aria-label=move || format!("Edit {}", name.get())>
                        "⚙"
                    </a>
                </div>
            </header>
            <div class="lane-cards">
                <For
                    each=move || rows.get()
                    key=|key| *key
                    children=move |key| {
                        match key {
                            LaneCardKey::Item(item_id) => view! {
                                <ItemCard project=project.clone() item_id items label_accent_colors open_drawer />
                            }.into_any(),
                            LaneCardKey::Group(group_id) => {
                                let group = Memo::new(move |_| lane.with(|lane| lane.as_ref().and_then(|lane| lane.cards.groups.get(&group_id).cloned())));
                                view! { <BoardGroup project=project.clone() group items label_accent_colors open_drawer /> }.into_any()
                            }
                        }
                    }
                />
            </div>
        </section>
    }
}

#[component]
fn BoardGroup(
    project: String,
    group: Memo<Option<BoardGroupData>>,
    items: BoardItems,
    label_accent_colors: LabelAccents,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    let metadata =
        Memo::new(move |_| group.with(|group| group.as_ref().map(|group| group.group.clone())));
    let item_ids = Memo::new(move |_| {
        group.with(|group| {
            group
                .as_ref()
                .map(|group| group.items.clone())
                .unwrap_or_default()
        })
    });
    let key = move || {
        metadata.with(|group| {
            group
                .as_ref()
                .map(|group| group.key.clone())
                .unwrap_or_default()
        })
    };
    view! {
        <section class="work-item-card-group" data-work-group-key=key>
            <header>
                <div>
                    <strong>{move || metadata.with(|group| group.as_ref().map(|group| group.name.clone()).unwrap_or_default())}</strong>
                    <code>{key}</code>
                </div>
                <span>{move || format!("{} in this lane", item_ids.with(Vec::len))}</span>
            </header>
            <div class="work-item-card-group-items">
                <For each=move || item_ids.get() key=|id| *id children=move |item_id| {
                    view! { <ItemCard project=project.clone() item_id items label_accent_colors open_drawer /> }
                } />
            </div>
        </section>
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct LaneCards {
    order: Vec<LaneCardKey>,
    groups: HashMap<i64, BoardGroupData>,
}

#[derive(Clone, Debug, PartialEq)]
struct BoardGroupData {
    group: WorkItemGroupSummaryView,
    items: Vec<i64>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LaneCardKey {
    Item(i64),
    Group(i64),
}

fn group_lane_items(items: Vec<&BoardItemView>) -> LaneCards {
    let mut cards = LaneCards::default();
    for item in items {
        let Some(group) = &item.item.work_group else {
            cards.order.push(LaneCardKey::Item(item.item.id));
            continue;
        };
        let entry = cards.groups.entry(group.id).or_insert_with(|| {
            cards.order.push(LaneCardKey::Group(group.id));
            BoardGroupData {
                group: group.clone(),
                items: Vec::new(),
            }
        });
        entry.items.push(item.item.id);
    }
    cards
}

fn board_state_warning(misconfigured_item_count: i64) -> AnyView {
    if misconfigured_item_count > 0 {
        let item_word = if misconfigured_item_count == 1 {
            "item"
        } else {
            "items"
        };
        let verb = if misconfigured_item_count == 1 {
            "has"
        } else {
            "have"
        };
        let message =
            format!("{misconfigured_item_count} {item_word} {verb} an unknown or missing state.");

        view! {
            <section class="board-state-warning" role="status">
                <strong>"State warning"</strong>
                <span>{message}</span>
                <a href="#work-items-admin">"Review work items"</a>
            </section>
        }
        .into_any()
    } else {
        ().into_any()
    }
}

fn lane_edit_href(project: &str, lane_id: i64) -> String {
    format!(
        "/project?project={}&edit_swim_lane={}#swim-lanes",
        encode_path(project),
        lane_id
    )
}

fn sort_lane_items(items: &mut [&BoardItemView], item_order: SwimLaneItemOrder) {
    match item_order {
        SwimLaneItemOrder::UpdatedAsc => items.sort_by(|left, right| {
            left.item
                .updated_at
                .cmp(&right.item.updated_at)
                .then_with(|| left.item.id.cmp(&right.item.id))
        }),
        SwimLaneItemOrder::CreatedDesc => items.sort_by(|left, right| {
            right
                .item
                .created_at
                .cmp(&left.item.created_at)
                .then_with(|| right.item.id.cmp(&left.item.id))
        }),
        SwimLaneItemOrder::CreatedAsc => items.sort_by(|left, right| {
            left.item
                .created_at
                .cmp(&right.item.created_at)
                .then_with(|| left.item.id.cmp(&right.item.id))
        }),
        SwimLaneItemOrder::IdDesc => items.sort_by_key(|item| std::cmp::Reverse(item.item.id)),
        SwimLaneItemOrder::IdAsc => items.sort_by_key(|item| item.item.id),
        SwimLaneItemOrder::TitleAsc => {
            items.sort_by_cached_key(|item| (item.item.title.to_lowercase(), item.item.id))
        }
        SwimLaneItemOrder::TitleDesc => items.sort_by_cached_key(|item| {
            std::cmp::Reverse((item.item.title.to_lowercase(), item.item.id))
        }),
        SwimLaneItemOrder::UpdatedDesc => items.sort_by(|left, right| {
            right
                .item
                .updated_at
                .cmp(&left.item.updated_at)
                .then_with(|| right.item.id.cmp(&left.item.id))
        }),
    }
}

#[component]
fn ItemCard(
    project: String,
    item_id: i64,
    items: BoardItems,
    label_accent_colors: LabelAccents,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    // Keyed cards look up their current snapshot in constant time. Reading a
    // borrowed map avoids cloning the board for every card and every field.
    let board_item = Memo::new(move |_| items.with(|items| items.get(&item_id).cloned()));
    let description = Memo::new(move |_| {
        board_item.with(|item| {
            item.as_ref()
                .map(|item| preview(&item.item.description_excerpt))
                .unwrap_or_default()
        })
    });
    let labels = Memo::new(move |_| {
        board_item.with(|item| {
            item.as_ref()
                .map(|item| item.item.labels.clone())
                .unwrap_or_default()
        })
    });
    let run_data = Memo::new(move |_| {
        board_item.with(|item| {
            item.as_ref().map(|item| {
                (
                    item.run_count,
                    item.recent_runs.clone(),
                    item.item.claim_source.clone(),
                    item.item.claimed_by.clone(),
                    item.item.claimed_at.clone(),
                )
            })
        })
    });
    let href = crate::frontend::routes::routes::Item.materialize(encode_path(&project), item_id);
    view! {
        <article class="card" class:claimed=move || board_item.with(|item| item.as_ref().is_some_and(|item| item.item.claimed_by.is_some()))>
            <a class="card-main-link" href=href data-board-item-id=item_id on:click=move |event| {
                if intercepts_board_drawer_click(&event) {
                    event.prevent_default();
                    open_drawer.run(BoardDrawerSelection { item_id, run_id: None });
                }
            }>
                <h3>{move || board_item.with(|item| item.as_ref().map(|item| item.item.title.clone()).unwrap_or_default())}</h3>
                <p>{move || description.get()}</p>
                <div class="card-labels">
                    {move || labels.get().into_iter().map(|label| {
                        let blocked = label.key == AUTOMATION_BLOCKED_LABEL_KEY;
                        let feedback_requested = label.key == FEEDBACK_REQUESTED_LABEL_KEY;
                        let text = format_label(&label.key, label.value.as_deref());
                        let key = label.key.clone();
                        let accent = Memo::new(move |_| label_accent_colors.with(|colors| colors.get(&key).cloned()));
                        view! {
                            <span class="label-chip" class:blocked=blocked class:feedback=feedback_requested
                                class:accented=move || accent.with(Option::is_some)
                                data-label-key=label.key
                                style=move || accent.get().map(|color| format!("--label-accent: {color};"))>
                                {text}
                            </span>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </a>
            {move || run_data.get().map(|(run_count, recent_runs, claim_source, claimed_by, claimed_at)| {
                let active_claim_run_id = claim_source.as_ref().map(|source| source.run_id)
                    .or_else(|| claimed_by.as_deref().and_then(infer_dispatch_run_id));
                let active_claim_source = claim_source_label(claim_source.as_ref());
                view! {
                    <CardRuns project=project.clone() item_id run_count recent_runs
                        active_claim_run_id active_claim_source active_claimed_at=claimed_at open_drawer />
                }
            })}
            <footer>
                <span class="card-item-id">"#" {item_id}</span>
                <span>{move || board_item.with(|item| item.as_ref().map(|item| item.item.comment_count).unwrap_or_default())} " comments"</span>
                <span>{move || board_item.with(|item| item.as_ref().map(|item| item.item.updated_at.clone()).unwrap_or_default())}</span>
            </footer>
        </article>
    }
}

#[component]
fn CardRuns(
    project: String,
    item_id: i64,
    run_count: usize,
    recent_runs: Vec<BoardRunPreview>,
    active_claim_run_id: Option<i64>,
    active_claim_source: Option<String>,
    active_claimed_at: Option<String>,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    let run_links = recent_runs
        .into_iter()
        .map(|run| {
            let run_id = run.id;
            let href =
                crate::frontend::routes::routes::RunLog.materialize(encode_path(&project), run_id);
            let status = run.status.to_string();
            let status_class = run_status_class(run.status);
            let summary = (!run.result_summary.trim().is_empty()).then_some(run.result_summary);
            let claim_context = (active_claim_run_id == Some(run_id)
                && (active_claim_source.is_some() || active_claimed_at.is_some()))
            .then(|| {
                let source = active_claim_source.clone();
                let elapsed = claim_elapsed_timer(active_claimed_at.clone());
                view! {
                    <span class="card-run-context" title="Active claim">
                        {source.map(|source| view! {
                            <span class="card-run-source" title="Automation source">{source}</span>
                        })}
                        {elapsed}
                    </span>
                }
            });
            view! {
                <a
                    class=format!("card-run-preview {status_class}")
                    href=href
                    on:click=move |event| {
                        if intercepts_board_drawer_click(&event) {
                            event.prevent_default();
                            open_drawer.run(BoardDrawerSelection {
                                item_id,
                                run_id: Some(run_id),
                            });
                        }
                    }
                >
                    <strong>"#" {run_id}</strong>
                    <span class="card-run-status">{status}</span>
                    {summary.map(|summary| view! {
                        <span class="card-run-summary">{summary}</span>
                    })}
                    {claim_context}
                </a>
            }
        })
        .collect::<Vec<_>>();
    (run_count > 0).then(|| {
        let label = if run_count == 1 {
            "Run 1".to_owned()
        } else {
            format!("Runs {run_count}")
        };
        view! {
            <section class="card-runs" aria-label=label.clone()>
                <div class="card-run-count">{label.clone()}</div>
                <div class="card-run-previews">{run_links}</div>
            </section>
        }
    })
}

#[cfg(test)]
mod tests;
