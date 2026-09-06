use super::*;
use crate::shared::view_models::BoardWorkItemView;
use assertr::prelude::*;
use crudkit_leptos::crudkit_core::condition::Condition;

fn item(id: i64, title: &str, group: Option<i64>) -> BoardItemView {
    BoardItemView {
        item: BoardWorkItemView {
            id,
            title: title.to_owned(),
            description_excerpt: String::new(),
            labels: Vec::new(),
            claimed_by: None,
            claimed_at: None,
            claim_source: None,
            created_at: id.to_string(),
            updated_at: id.to_string(),
            comment_count: 0,
            work_group: group.map(|id| WorkItemGroupSummaryView {
                id,
                key: format!("group-{id}"),
                name: format!("Group {id}"),
            }),
        },
        run_count: 0,
        recent_runs: Vec::new(),
    }
}

#[test]
fn item_content_changes_do_not_invalidate_lane_membership() {
    let mut section = BoardItemsSection {
        items: vec![item(1, "First", None), item(2, "Second", Some(1))],
        swim_lanes: vec![SwimLaneView {
            id: 1,
            project_id: 1,
            identifier: "all".to_owned(),
            name: "All".to_owned(),
            position: 0,
            filter: Condition::All(Vec::new()),
            item_order: SwimLaneItemOrder::IdAsc,
            can_create_items: false,
            created_at: String::new(),
            updated_at: String::new(),
        }],
        work_item_states: Vec::new(),
        label_accent_colors: BTreeMap::new(),
        misconfigured_item_count: 0,
    };
    let before = board_lane_data(&section);
    section.items[0].item.title = "Edited".to_owned();
    section.items[0].item.comment_count = 1;
    section.items[1].run_count = 3;
    section
        .label_accent_colors
        .insert("state".to_owned(), "#ff0000".to_owned());
    assert_that!(board_lane_data(&section)).is_equal_to(before);
}

#[test]
fn groups_keep_first_position_and_member_order_without_colliding_with_item_ids() {
    let items = [
        item(1, "Standalone", None),
        item(2, "First grouped", Some(1)),
        item(3, "Between", None),
        item(4, "Second grouped", Some(1)),
    ];
    let rows = group_lane_items(items.iter().collect());
    assert_that!(rows.order).is_equal_to(vec![
        LaneCardKey::Item(1),
        LaneCardKey::Group(1),
        LaneCardKey::Item(3),
    ]);
    assert_that!(&rows.groups[&1].items).is_equal_to(vec![2, 4]);
}

#[test]
fn lane_sorting_preserves_all_orders_and_deterministic_title_ties() {
    let items = [item(1, "b", None), item(2, "A", None), item(3, "a", None)];
    for (order, expected) in [
        (SwimLaneItemOrder::UpdatedAsc, vec![1, 2, 3]),
        (SwimLaneItemOrder::UpdatedDesc, vec![3, 2, 1]),
        (SwimLaneItemOrder::CreatedAsc, vec![1, 2, 3]),
        (SwimLaneItemOrder::CreatedDesc, vec![3, 2, 1]),
        (SwimLaneItemOrder::IdAsc, vec![1, 2, 3]),
        (SwimLaneItemOrder::IdDesc, vec![3, 2, 1]),
        (SwimLaneItemOrder::TitleAsc, vec![2, 3, 1]),
        (SwimLaneItemOrder::TitleDesc, vec![1, 3, 2]),
    ] {
        let mut sorted = items.iter().collect::<Vec<_>>();
        sort_lane_items(&mut sorted, order);
        assert_that!(sorted.iter().map(|item| item.item.id).collect::<Vec<_>>())
            .is_equal_to(expected);
    }
}
