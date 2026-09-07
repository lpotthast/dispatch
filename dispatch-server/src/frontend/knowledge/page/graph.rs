//! Deterministic refinement layout and a keyboard/pointer-accessible SVG viewport.
use dispatch_types::knowledge::{KnowledgeRelationKind, KnowledgeSummary, KnowledgeView};
use leptos::prelude::*;
use std::collections::BTreeMap;

pub(super) const WIDTH: f64 = 1000.0;
pub(super) const HEIGHT: f64 = 620.0;
const NODE_WIDTH: f64 = 196.0;
const NODE_HEIGHT: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}
impl Viewport {
    pub fn zoom_by(self, factor: f64) -> Self {
        let zoom = (self.zoom * factor).clamp(0.08, 3.0);
        let ratio = zoom / self.zoom;
        Self {
            x: WIDTH / 2.0 - (WIDTH / 2.0 - self.x) * ratio,
            y: HEIGHT / 2.0 - (HEIGHT / 2.0 - self.y) * ratio,
            zoom,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    document: KnowledgeSummary,
    x: f64,
    y: f64,
    organized: bool,
    diagnostic: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Layout {
    nodes: Vec<Node>,
    width: f64,
    height: f64,
}
impl Layout {
    pub fn center(&self, path: &str, viewport: Viewport) -> Viewport {
        self.nodes
            .iter()
            .find(|node| node.document.path == path)
            .map(|node| Viewport {
                x: WIDTH / 2.0 - (node.x + NODE_WIDTH / 2.0) * viewport.zoom,
                y: HEIGHT / 2.0 - (node.y + NODE_HEIGHT / 2.0) * viewport.zoom,
                ..viewport
            })
            .unwrap_or(viewport)
    }
    pub fn fit(&self) -> Viewport {
        let zoom = ((WIDTH - 64.0) / self.width)
            .min((HEIGHT - 64.0) / self.height)
            .clamp(0.08, 1.5);
        Viewport {
            x: (WIDTH - self.width * zoom) / 2.0,
            y: (HEIGHT - self.height * zoom) / 2.0,
            zoom,
        }
    }
}

pub(super) fn layout(graph: &KnowledgeView) -> Layout {
    let mut ranks = BTreeMap::from([("README.md".to_owned(), 0usize)]);
    // Input refinement is acyclic. Longest reachable rank puts shared children below every parent.
    for _ in 0..graph.documents.len() {
        let mut changed = false;
        for edge in graph
            .relations
            .iter()
            .filter(|edge| edge.kind == KnowledgeRelationKind::Refines)
        {
            if let Some(parent) = ranks.get(&edge.to).copied() {
                let rank = ranks.entry(edge.from.clone()).or_insert(0);
                if *rank < parent + 1 {
                    *rank = parent + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let unorganized_rank = ranks.values().max().copied().unwrap_or(0) + 1;
    let mut rows: BTreeMap<usize, Vec<KnowledgeSummary>> = BTreeMap::new();
    for document in &graph.documents {
        rows.entry(
            ranks
                .get(&document.path)
                .copied()
                .unwrap_or(unorganized_rank),
        )
        .or_default()
        .push(document.clone());
    }
    for row in rows.values_mut() {
        row.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then(a.path.cmp(&b.path))
        });
    }
    let width = rows.values().map(Vec::len).max().unwrap_or(1) as f64 * 236.0 + 56.0;
    let height = rows.keys().max().copied().unwrap_or(0) as f64 * 128.0 + 156.0;
    let mut nodes = vec![];
    for (rank, row) in rows {
        let start = (width - row.len() as f64 * 236.0 + 40.0) / 2.0;
        for (column, document) in row.into_iter().enumerate() {
            let organized = ranks.contains_key(&document.path);
            let diagnostic = graph.diagnostics.iter().any(|d| d.path == document.path);
            nodes.push(Node {
                document,
                x: start + column as f64 * 236.0,
                y: 48.0 + rank as f64 * 128.0,
                organized,
                diagnostic,
            });
        }
    }
    Layout {
        nodes,
        width,
        height,
    }
}

fn compact(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.into()
    } else {
        format!("{}…", text.chars().take(limit - 1).collect::<String>())
    }
}

#[component]
pub(super) fn KnowledgeGraph(
    graph: RwSignal<Option<KnowledgeView>>,
    selected: RwSignal<String>,
    viewport: RwSignal<Viewport>,
    select: Callback<String>,
) -> impl IntoView {
    let drag = RwSignal::new(None::<(i32, f64, f64, Viewport)>);
    let neighborhood = RwSignal::new(false);
    let visible = Memo::new(move |_| {
        graph.get().map(|mut graph| {
            if neighborhood.get() {
                let path = selected.get();
                let mut paths = vec![path.clone()];
                for edge in &graph.relations {
                    if edge.from == path || edge.to == path {
                        paths.push(edge.from.clone());
                        paths.push(edge.to.clone());
                    }
                }
                graph.documents.retain(|doc| paths.contains(&doc.path));
                graph
                    .relations
                    .retain(|edge| paths.contains(&edge.from) && paths.contains(&edge.to));
            }
            graph
        })
    });
    let drawing = Memo::new(move |_| visible.get().map(|graph| layout(&graph)));
    let fit = Callback::new(move |()| {
        if let Some(layout) = drawing.get_untracked() {
            viewport.set(layout.fit());
        }
    });
    view! {
        <div class="knowledge-canvas-controls">
            <div class="knowledge-zoom-controls">
                <button type="button" aria-label="Zoom out" on:click=move |_| viewport.update(|v| *v = v.zoom_by(1.0 / 1.2))>"−"</button>
                <span class="knowledge-zoom-value">{move || format!("{:.0}%", viewport.get().zoom * 100.0)}</span>
                <button type="button" aria-label="Zoom in" on:click=move |_| viewport.update(|v| *v = v.zoom_by(1.2))>"+"</button>
            </div>
            <button type="button" on:click=move |_| fit.run(())>"Fit"</button>
            <button type="button" on:click=move |_| select.run("README.md".into())>"Root"</button>
            <button type="button" aria-pressed=move || neighborhood.get().to_string() on:click=move |_| { neighborhood.update(|v| *v = !*v); fit.run(()); }>
                {move || if neighborhood.get() { "Whole project" } else { "Neighborhood" }}
            </button>
        </div>
        <div class="knowledge-canvas-frame">
            <svg class="knowledge-canvas" viewBox="0 0 1000 620" tabindex="0" role="group" aria-label="Knowledge graph. Drag to pan; use plus and minus to zoom, Home to fit."
                on:keydown=move |event| {
                    let mut next = viewport.get_untracked();
                    match event.key().as_str() {
                        "+" | "=" => next = next.zoom_by(1.2), "-" => next = next.zoom_by(1.0 / 1.2),
                        "ArrowLeft" => next.x += 40.0, "ArrowRight" => next.x -= 40.0,
                        "ArrowUp" => next.y += 40.0, "ArrowDown" => next.y -= 40.0,
                        "Home" => { event.prevent_default(); fit.run(()); return; }, _ => return,
                    }
                    event.prevent_default(); viewport.set(next);
                }
                on:wheel=move |event| {
                    event.prevent_default();
                    viewport.update(|v| {
                        if event.ctrl_key() || event.meta_key() { *v = v.zoom_by((-event.delta_y() * 0.0025).clamp(-0.35,0.35).exp()); }
                        else { v.x -= event.delta_x(); v.y -= event.delta_y(); }
                    });
                }
                on:pointerdown=move |event| {
                    if event.button() != 0 { return; }
                    event.prevent_default(); capture_pointer(&event, true);
                    let (x,y) = pointer_position(&event);
                    drag.set(Some((event.pointer_id(),x,y,viewport.get_untracked())));
                }
                on:pointermove=move |event| {
                    if let Some((id,x,y,start)) = drag.get_untracked() && id == event.pointer_id() {
                        let (next_x,next_y) = pointer_position(&event);
                        viewport.set(Viewport { x: start.x + next_x - x, y: start.y + next_y - y, ..start });
                    }
                }
                on:pointerup=move |event| { capture_pointer(&event, false); drag.set(None); }
                on:pointercancel=move |event| { capture_pointer(&event, false); drag.set(None); }
            >
                <defs><marker id="knowledge-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z"/></marker></defs>
                <g class="knowledge-graph-world" transform=move || { let v = viewport.get(); format!("translate({} {}) scale({})",v.x,v.y,v.zoom) }>
                    {move || drawing.get().zip(visible.get()).map(|(layout,graph)| {
                        let edges = graph.relations.iter().filter_map(|edge| {
                            let from = layout.nodes.iter().find(|n| n.document.path == edge.from)?;
                            let to = layout.nodes.iter().find(|n| n.document.path == edge.to)?;
                            let (sy,ty) = if from.y > to.y { (from.y, to.y + NODE_HEIGHT) } else { (from.y + NODE_HEIGHT, to.y) };
                            let (sx,tx) = (from.x + NODE_WIDTH / 2.0, to.x + NODE_WIDTH / 2.0);
                            let kind = match edge.kind { KnowledgeRelationKind::Refines => "refines", KnowledgeRelationKind::DependsOn => "depends-on", KnowledgeRelationKind::RelatedTo => "related-to" };
                            let description = format!("{} {} {}",from.document.title,kind,to.document.title);
                            let from = edge.from.clone(); let to = edge.to.clone();
                            Some(view! { <path class=format!("knowledge-graph-edge {kind}") class:selected=move || selected.with(|p| *p == from || *p == to)
                                d=format!("M {sx} {sy} C {sx} {} {tx} {} {tx} {ty}",(sy+ty)/2.0,(sy+ty)/2.0)
                                marker-end=(edge.kind != KnowledgeRelationKind::RelatedTo).then_some("url(#knowledge-arrow)")><title>{description}</title></path> })
                        }).collect_view();
                        let nodes = layout.nodes.into_iter().map(|node| {
                            let path = node.document.path.clone(); let click_path = path.clone(); let key_path = path.clone();
                            let title = compact(&node.document.title,25);
                            let subtitle = compact(node.document.id.as_deref().unwrap_or(&path),31);
                            let accessible = format!("{}{}", node.document.title, if node.organized { "" } else { ", unorganized" });
                            view! {
                                <g class="knowledge-graph-node" class:root=path == "README.md" class:unorganized=!node.organized class:diagnostic=node.diagnostic
                                    class:selected=move || selected.get() == path role="button" tabindex="0" aria-label=accessible data-knowledge-path=node.document.path
                                    on:pointerdown=move |event| event.stop_propagation()
                                    on:click=move |_| select.run(click_path.clone())
                                    on:keydown=move |event| { if matches!(event.key().as_str(),"Enter"|" ") { event.prevent_default(); event.stop_propagation(); select.run(key_path.clone()); } }
                                >
                                    <title>{format!("{}\n{}",node.document.title,node.document.summary)}</title>
                                    <rect x=node.x y=node.y width=NODE_WIDTH height=NODE_HEIGHT rx="6"/>
                                    <text class="knowledge-graph-node-title" x=node.x + NODE_WIDTH/2.0 y=node.y+24.0 text-anchor="middle">{title}</text>
                                    <text class="knowledge-graph-node-id" x=node.x + NODE_WIDTH/2.0 y=node.y+44.0 text-anchor="middle">{subtitle}</text>
                                </g>
                            }
                        }).collect_view();
                        view! { <g>{edges}{nodes}</g> }
                    })}
                </g>
            </svg>
        </div>
        <div class="knowledge-graph-legend" aria-label="Graph legend">
            <span><i class="legend-node root"></i>"Root"</span><span><i class="legend-node unorganized"></i>"Unorganized"</span>
            <span><i class="legend-edge"></i>"Refines → broader"</span><span><i class="legend-edge dependency"></i>"Depends on →"</span><span><i class="legend-edge related"></i>"Related"</span>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
fn pointer_position(event: &leptos::ev::PointerEvent) -> (f64, f64) {
    use wasm_bindgen::JsCast;
    let Some(element) = event
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    else {
        return (0.0, 0.0);
    };
    let rect = element.get_bounding_client_rect();
    let scale = (rect.width() / WIDTH)
        .min(rect.height() / HEIGHT)
        .max(0.001);
    (
        (f64::from(event.client_x()) - rect.left() - (rect.width() - WIDTH * scale) / 2.0) / scale,
        (f64::from(event.client_y()) - rect.top() - (rect.height() - HEIGHT * scale) / 2.0) / scale,
    )
}
#[cfg(not(target_arch = "wasm32"))]
fn pointer_position(event: &leptos::ev::PointerEvent) -> (f64, f64) {
    (f64::from(event.client_x()), f64::from(event.client_y()))
}

#[cfg(target_arch = "wasm32")]
pub(super) fn capture_pointer(event: &leptos::ev::PointerEvent, capture: bool) {
    use wasm_bindgen::JsCast;
    if let Some(element) = event
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    {
        if capture {
            let _ = element.set_pointer_capture(event.pointer_id());
        } else {
            let _ = element.release_pointer_capture(event.pointer_id());
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn capture_pointer(_event: &leptos::ev::PointerEvent, _capture: bool) {}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use dispatch_types::knowledge::KnowledgeRelation;
    #[test]
    fn shared_children_stay_below_all_parents_and_nodes_do_not_overlap() {
        let documents = ["README.md", "a.md", "b.md", "shared.md", "plain.md"]
            .map(|path| KnowledgeSummary {
                path: path.into(),
                id: Some(path.into()),
                title: path.into(),
                summary: String::new(),
                fingerprint: String::new(),
            })
            .to_vec();
        let relations = [
            ("a.md", "README.md"),
            ("b.md", "a.md"),
            ("shared.md", "a.md"),
            ("shared.md", "b.md"),
        ]
        .map(|(from, to)| KnowledgeRelation {
            from: from.into(),
            to: to.into(),
            kind: KnowledgeRelationKind::Refines,
        })
        .to_vec();
        let graph = KnowledgeView {
            project_id: 1,
            working_directory: String::new(),
            knowledge_directory: "knowledge".into(),
            index_generation: String::new(),
            document: None,
            documents,
            next_offset: None,
            diagnostics: vec![],
            relations,
        };
        let layout = layout(&graph);
        let y = |path: &str| {
            layout
                .nodes
                .iter()
                .find(|node| node.document.path == path)
                .unwrap()
                .y
        };
        assert_that!(&(y("shared.md") > y("b.md"))).is_true();
        assert_that!(
            &layout
                .nodes
                .iter()
                .find(|node| node.document.path == "plain.md")
                .unwrap()
                .organized
        )
        .is_false();
        for (position, a) in layout.nodes.iter().enumerate() {
            for b in &layout.nodes[position + 1..] {
                assert_that!(
                    &(a.x + NODE_WIDTH <= b.x
                        || b.x + NODE_WIDTH <= a.x
                        || a.y + NODE_HEIGHT <= b.y
                        || b.y + NODE_HEIGHT <= a.y)
                )
                .is_true();
            }
        }
        let fitted = layout.fit();
        assert_that!(&(fitted.x >= 0.0 && fitted.y >= 0.0 && fitted.zoom.is_finite())).is_true();
    }
}
