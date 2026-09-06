mod graph;
mod markdown;

use crate::frontend::{
    components::selected_project_signal,
    services::{KnowledgeUiService, knowledge_ui_service},
};
use crudkit_leptos::{
    crud_instance_mgr::CrudInstanceMgrContext, crud_navigation::CrudNavigationScope,
    crud_navigation_confirmation::CrudNavigationConfirmation,
};
use dispatch_types::knowledge::*;
use graph::{KnowledgeGraph, Viewport, capture_pointer};
use leptos::prelude::*;
use leptos_meta::Title;

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Search,
    Edit,
    Create,
    Relationships,
    Diagnostics,
}
impl Tool {
    fn title(self) -> &'static str {
        match self {
            Self::Search => "Search",
            Self::Edit => "Edit document",
            Self::Create => "New document",
            Self::Relationships => "Relationships",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

/// The draft belongs to the workspace, so refreshes and failed saves cannot discard it.
#[derive(Clone, Copy)]
struct Editor {
    draft: RwSignal<String>,
    baseline: RwSignal<String>,
    fingerprint: RwSignal<Option<String>>,
    path: RwSignal<String>,
    saving: RwSignal<bool>,
}
impl Editor {
    fn new() -> Self {
        Self {
            draft: RwSignal::new(String::new()),
            baseline: RwSignal::new(String::new()),
            fingerprint: RwSignal::new(None),
            path: RwSignal::new(String::new()),
            saving: RwSignal::new(false),
        }
    }
    fn dirty(self) -> bool {
        self.draft.get() != self.baseline.get()
            || (self.fingerprint.get().is_none() && !self.path.get().is_empty())
    }
    fn open(self, document: &KnowledgeDocument) {
        self.path.set(document.summary.path.clone());
        self.draft.set(document.markdown.clone());
        self.baseline.set(document.markdown.clone());
        self.fingerprint
            .set(Some(document.summary.fingerprint.clone()));
    }
    fn discard(self) {
        self.draft.set(self.baseline.get_untracked());
        if self.fingerprint.get_untracked().is_none() {
            self.path.set(String::new());
        }
    }
}

#[component]
pub fn PageKnowledge() -> impl IntoView {
    let project = selected_project_signal();
    view! {
        <Title text="Knowledge · Dispatch"/>
        <div class="knowledge-route">
            {move || match project.get() {
                Some(project) => view! { <KnowledgeWorkspace project/> }.into_any(),
                None => view! { <main class="page-shell"><h1>"Knowledge"</h1><p>"Select a project to explore its knowledge."</p></main> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn KnowledgeWorkspace(project: String) -> impl IntoView {
    let project = StoredValue::new(project);
    let service = StoredValue::new(knowledge_ui_service());
    let graph = RwSignal::new(None::<KnowledgeView>);
    let selected = RwSignal::new("README.md".to_owned());
    let document = RwSignal::new(None::<KnowledgeDocument>);
    let editor = Editor::new();
    let active_tool = RwSignal::new(None::<Tool>);
    let message = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let document_error = RwSignal::new(None::<String>);
    let refresh = RwSignal::new(0_u64);
    let viewport = RwSignal::new(Viewport::default());
    let fitted = RwSignal::new(false);
    let list_mode = RwSignal::new(false);
    let width = RwSignal::new(56.0_f64);
    let resizing = RwSignal::new(false);
    let workspace_ref = NodeRef::<leptos::html::Section>::new();
    let navigation = expect_context::<CrudInstanceMgrContext>()
        .navigation_scope()
        .child();
    navigation.guard(Signal::derive(move || {
        editor.dirty() || editor.saving.get()
    }));

    // Local resources keep async document loading out of the SSR stream and retain the workbench.
    let graph_result = LocalResource::new(move || {
        refresh.track();
        let service = service.get_value();
        let project = project.get_value();
        async move { service.graph(project).await }
    });
    Effect::new(move || {
        if let Some(result) = graph_result.get() {
            match result {
                Ok(data) => {
                    if !fitted.get_untracked() {
                        viewport.set(graph::layout(&data).fit());
                        fitted.set(true);
                    }
                    if !data
                        .documents
                        .iter()
                        .any(|doc| doc.path == selected.get_untracked())
                    {
                        document.set(None);
                        if !editor.dirty_untracked() {
                            selected.set(
                                data.documents
                                    .iter()
                                    .find(|doc| doc.path == "README.md")
                                    .or(data.documents.first())
                                    .map(|doc| doc.path.clone())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                    graph.set(Some(data));
                    error.set(None);
                }
                Err(e) => {
                    graph.set(None);
                    document.set(None);
                    error.set(Some(error_message(e)));
                }
            }
        }
    });
    let document_result = LocalResource::new(move || {
        refresh.track();
        let path = selected.get();
        let service = service.get_value();
        let project = project.get_value();
        async move {
            if path.is_empty() {
                Ok(None)
            } else {
                service.document(project, path).await.map(Some)
            }
        }
    });
    Effect::new(move || {
        if let Some(result) = document_result.get() {
            match result {
                Ok(Some(data)) => {
                    if !editor.dirty_untracked()
                        && active_tool.get_untracked() != Some(Tool::Create)
                    {
                        editor.open(&data);
                    }
                    document.set(Some(data));
                    document_error.set(None);
                }
                Ok(None) => {
                    document.set(None);
                    document_error.set(None);
                }
                Err(e) => {
                    document.set(None);
                    document_error.set(Some(error_message(e)));
                }
            }
        }
    });
    let select = Callback::new(move |path: String| {
        if selected.get_untracked() == path {
            if active_tool.get_untracked() == Some(Tool::Search) {
                active_tool.set(None);
            }
            return;
        }
        navigation.attempt(
            move || {
                editor.discard();
                document.set(None);
                document_error.set(None);
                message.set(None);
                active_tool.set(None);
                selected.set(path.clone());
                if let Some(graph) = graph.get_untracked() {
                    viewport.update(|v| *v = graph::layout(&graph).center(&path, *v));
                }
            },
            || {},
        );
    });
    let open = Callback::new(move |tool: Tool| {
        if active_tool.get_untracked() == Some(tool) {
            return;
        }
        navigation.attempt(
            move || {
                editor.discard();
                message.set(None);
                if tool == Tool::Create {
                    editor.path.set(String::new());
                    editor.fingerprint.set(None);
                    editor.baseline.set(String::new());
                    editor.draft.set(String::new());
                } else if let Some(document) = document.get_untracked() {
                    editor.open(&document);
                }
                active_tool.set(Some(tool));
            },
            || {},
        );
    });
    let close = Callback::new(move |()| {
        if editor.saving.get_untracked() {
            return;
        }
        navigation.attempt(
            move || {
                editor.discard();
                if let Some(tool) = active_tool.get_untracked() {
                    restore_tool_focus(tool);
                }
                active_tool.set(None);
            },
            || {},
        );
    });
    Effect::new(move || {
        if active_tool.get().is_some() {
            request_animation_frame(focus_drawer);
        }
    });
    let save = Callback::new(move |()| {
        if editor.saving.get_untracked() {
            return;
        }
        let request = KnowledgeSaveRequest {
            path: editor.path.get_untracked(),
            expected_fingerprint: editor.fingerprint.get_untracked(),
            markdown: editor.draft.get_untracked(),
        };
        let saved_text = request.markdown.clone();
        let saved_path = request.path.clone();
        let service = service.get_value();
        let project = project.get_value();
        editor.saving.set(true);
        message.set(None);
        leptos::task::spawn_local(async move {
            let result = service.save(project, request).await;
            if editor.saving.try_get().is_none() {
                return;
            }
            editor.saving.set(false);
            match result {
                Ok(result) => {
                    editor.baseline.set(saved_text);
                    editor.fingerprint.set(Some(result.fingerprint));
                    selected.set(saved_path);
                    active_tool.set(Some(Tool::Edit));
                    message.set(Some("Document saved.".into()));
                    refresh.update(|v| *v += 1);
                }
                Err(e) => message.set(Some(error_message(e))),
            }
        });
    });
    let resize = Callback::new(move |x: f64| {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = workspace_ref.get_untracked() {
            let rect = element.get_bounding_client_rect();
            if rect.width() > 0.0 {
                width.set(((x - rect.left()) / rect.width() * 100.0).clamp(28.0, 72.0));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = x;
    });
    view! {
        <CrudNavigationConfirmation scope=navigation/>
        <main class="page-shell knowledge-page">
            <header class="knowledge-page-heading"><div><h1>"Knowledge"</h1><p>"Your project’s connected understanding."</p></div>
                <button type="button" aria-label="Refresh knowledge" on:click=move |_| refresh.update(|v| *v += 1)>"Refresh"</button>
            </header>
            {move || error.get().map(|error| view! { <p class="callout danger" role="alert">{error}</p> })}
            <section class="knowledge-workbench" on:keydown=move |event| { if event.key() == "Escape" && active_tool.get_untracked().is_some() { event.stop_propagation(); close.run(()); } }>
                <nav class="knowledge-tools-rail" aria-label="Knowledge tools"><strong>"Tools"</strong>
                    {[Tool::Search,Tool::Create,Tool::Edit,Tool::Relationships,Tool::Diagnostics].into_iter().map(|tool| view! {
                        <button type="button" class:active=move || active_tool.get() == Some(tool) aria-expanded=move || (active_tool.get() == Some(tool)).to_string() aria-controls="knowledge-tool-drawer" data-knowledge-tool=tool.title()
                            disabled=move || editor.saving.get() || (matches!(tool,Tool::Edit|Tool::Relationships) && document.get().is_none()) on:click=move |_| open.run(tool)>{tool.title()}</button>
                    }).collect_view()}
                    <span class="knowledge-document-count">{move || graph.with(|g| g.as_ref().map(|g| format!("{} documents",g.documents.len())).unwrap_or_else(|| "Reading knowledge…".into()))}</span>
                </nav>
                <section class="knowledge-workspace" node_ref=workspace_ref style=move || format!("--knowledge-graph-width: {}%",width.get()) class:resizing=move || resizing.get()>
                    <section class="knowledge-canvas-pane" aria-label="Knowledge navigation">
                        <header class="knowledge-canvas-heading"><div class="knowledge-pane-title"><h2>{move || if list_mode.get() { "Documents" } else { "Graph" }}</h2></div>
                            <div class="knowledge-view-toggle"><button type="button" aria-pressed=move || (!list_mode.get()).to_string() on:click=move |_| list_mode.set(false)>"Graph"</button><button type="button" aria-pressed=move || list_mode.get().to_string() on:click=move |_| list_mode.set(true)>"List"</button></div>
                        </header>
                        <div class="knowledge-graph-container" hidden=move || list_mode.get()><KnowledgeGraph graph selected viewport select/></div>
                        <div class="knowledge-document-list" hidden=move || !list_mode.get()>{move || graph.get().map(|data| view! { <DocumentList documents=data.documents select selected/> })}</div>
                        <Show when=move || graph.with(|g| g.as_ref().is_some_and(|g| g.documents.is_empty()))><p class="knowledge-empty-state">"No visible documents. Create a document or check the configured directory and exclusions."</p></Show>
                    </section>
                    <button type="button" class="knowledge-workspace-resizer" role="separator" aria-label="Resize graph and document" aria-orientation="vertical" aria-valuemin="28" aria-valuemax="72" aria-valuenow=move || width.get().round().to_string()
                        on:pointerdown=move |event| { if event.button() == 0 { event.prevent_default(); resizing.set(true); capture_pointer(&event,true); resize.run(f64::from(event.client_x())); } }
                        on:pointermove=move |event| { if resizing.get_untracked() { resize.run(f64::from(event.client_x())); } }
                        on:pointerup=move |event| { resizing.set(false); capture_pointer(&event,false); }
                        on:pointercancel=move |event| { resizing.set(false); capture_pointer(&event,false); }
                        on:keydown=move |event| { let next = match event.key().as_str() { "ArrowLeft" => width.get_untracked()-2.0, "ArrowRight" => width.get_untracked()+2.0, "Home" => 28.0, "End" => 72.0, _ => return }; event.prevent_default(); width.set(next.clamp(28.0,72.0)); }
                    ></button>
                    <section class="knowledge-document-pane" aria-label="Selected document">
                        <header class="knowledge-document-heading"><h2>"Document"</h2><button type="button" disabled=move || document.get().is_none() on:click=move |_| open.run(Tool::Edit)>"Edit"</button></header>
                        <div class="knowledge-document-content">
                            {move || document_error.get().map(|e| view! { <p role="alert">{e}</p> })}
                            {move || document.get().map(|doc| view! {
                                <p class="knowledge-document-path"><code>{doc.summary.path.clone()}</code></p>
                                <MarkdownPreview markdown=Signal::derive(move || doc.markdown.clone()) path=doc.summary.path select/>
                            })}
                            <Show when=move || document.get().is_none() && document_error.get().is_none()><p class="knowledge-empty-state">"Select a document to read its knowledge."</p></Show>
                        </div>
                    </section>
                </section>
                <button type="button" class="knowledge-tool-backdrop" class:shown=move || active_tool.get().is_some() tabindex="-1" aria-label="Close knowledge tool" on:click=move |_| close.run(())></button>
                <aside id="knowledge-tool-drawer" class="knowledge-tool-drawer" class:open=move || active_tool.get().is_some() aria-label=move || active_tool.get().map(Tool::title).unwrap_or("Knowledge tool")>
                    <header class="knowledge-tool-drawer-heading"><h2>{move || active_tool.get().map(Tool::title)}</h2><button type="button" aria-label="Close knowledge tool" disabled=move || editor.saving.get() on:click=move |_| close.run(())>"×"</button></header>
                    <div class="knowledge-tool-body">
                        {move || match active_tool.get() {
                            Some(Tool::Search) => view! { <SearchDrawer project=project.get_value() service=service.get_value() select selected refresh/> }.into_any(),
                            Some(Tool::Edit|Tool::Create) => view! { <EditDrawer editor document creating=active_tool.get() == Some(Tool::Create) save navigation select/> }.into_any(),
                            Some(Tool::Relationships) => view! { <Relationships document graph select edit=Callback::new(move |()| open.run(Tool::Edit))/> }.into_any(),
                            Some(Tool::Diagnostics) => view! { <Diagnostics graph select refresh/> }.into_any(),
                            None => ().into_any(),
                        }}
                        {move || message.get().map(|text| view! { <p role="status" class="knowledge-message">{text}</p> })}
                    </div>
                </aside>
            </section>
        </main>
    }
}

impl Editor {
    fn dirty_untracked(self) -> bool {
        self.draft.get_untracked() != self.baseline.get_untracked()
            || (self.fingerprint.get_untracked().is_none() && !self.path.get_untracked().is_empty())
    }
}

#[component]
fn DocumentList(
    documents: Vec<KnowledgeSummary>,
    select: Callback<String>,
    selected: RwSignal<String>,
) -> impl IntoView {
    view! { <ul class="knowledge-search-results">{documents.into_iter().map(|doc| { let path = doc.path.clone(); view! { <li><button type="button" class:selected=move || selected.get() == path on:click=move |_| select.run(doc.path.clone())><strong>{doc.title}</strong><code>{doc.id.unwrap_or_else(|| "Unorganized document".into())}</code><span>{doc.summary}</span></button></li> } }).collect_view()}</ul> }
}

#[component]
fn SearchDrawer(
    project: String,
    service: KnowledgeUiService,
    select: Callback<String>,
    selected: RwSignal<String>,
    refresh: RwSignal<u64>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let submitted = RwSignal::new(String::new());
    let offset = RwSignal::new(0usize);
    let result = LocalResource::new(move || {
        let text = submitted.get();
        let offset = offset.get();
        refresh.track();
        let service = service.clone();
        let project = project.clone();
        async move {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                service
                    .read(
                        project,
                        KnowledgeOperation::Search,
                        KnowledgeQuery {
                            text: Some(text),
                            offset,
                            ..Default::default()
                        },
                    )
                    .await
                    .map(Some)
            }
        }
    });
    view! {
        <p>"Search document text and titles. No AI run is needed."</p>
        <div class="knowledge-search-input"><label>"Search knowledge"<input type="search" placeholder="Find a document…" prop:value=move || query.get() on:input=move |e| query.set(event_target_value(&e)) on:keydown=move |e| { if e.key() == "Enter" { submitted.set(query.get_untracked()); offset.set(0); } }/></label><button type="button" on:click=move |_| { submitted.set(query.get_untracked()); offset.set(0); }>"Find"</button></div>
        {move || result.get().map(|result| match result { Ok(Some(view)) => view! { {view.documents.is_empty().then(|| view! {<p>"No matching documents."</p> })}<DocumentList documents=view.documents select selected/><div class="button-row"><button type="button" disabled=move || offset.get() == 0 on:click=move |_| offset.update(|n| *n = n.saturating_sub(20))>"Previous"</button>{view.next_offset.map(|next| view! { <button type="button" on:click=move |_| offset.set(next)>"More results"</button> })}</div> }.into_any(), Ok(None) => ().into_any(), Err(e) => view! { <p role="alert">{error_message(e)}</p> }.into_any() })}
    }
}

#[component]
fn EditDrawer(
    editor: Editor,
    document: RwSignal<Option<KnowledgeDocument>>,
    creating: bool,
    save: Callback<()>,
    navigation: CrudNavigationScope,
    select: Callback<String>,
) -> impl IntoView {
    let preview = RwSignal::new(false);
    view! {
        <label>"Document path"<input prop:value=move || editor.path.get() disabled=move || !creating || editor.saving.get() placeholder="topic.md" on:input=move |e| editor.path.set(event_target_value(&e))/></label>
        <p>"Edit Markdown and frontmatter together. Preview shows the document’s rendered prose."</p>
        <div class="knowledge-editor-toolbar"><button type="button" aria-pressed=move || (!preview.get()).to_string() on:click=move |_| preview.set(false)>"Markdown"</button><button type="button" aria-pressed=move || preview.get().to_string() on:click=move |_| preview.set(true)>"Preview"</button><button type="button" class="primary" disabled=move || editor.saving.get() || editor.path.get().trim().is_empty() || !editor.dirty() on:click=move |_| save.run(())>{move || if editor.saving.get() { "Saving…" } else { "Save" }}</button></div>
        <Show when=move || !creating && document.with(|doc| doc.as_ref().is_some_and(|doc| editor.fingerprint.with(|fingerprint| fingerprint.as_ref() != Some(&doc.summary.fingerprint))))>
            <p class="callout danger" role="alert">"This document changed on disk. Your draft is preserved. Compare it with the current document below, or reload it."</p>
        </Show>
        <Show when=move || !creating && document.with(|doc| doc.as_ref().is_some_and(|doc| editor.fingerprint.with(|fingerprint| fingerprint.as_ref() != Some(&doc.summary.fingerprint))))>
            <details class="knowledge-conflict-comparison"><summary>"Current document on disk"</summary><pre>{move || document.with(|doc| doc.as_ref().map(|doc| doc.markdown.clone()))}</pre></details>
        </Show>
        <textarea class="knowledge-markdown-editor" aria-label="Document Markdown" rows="22" hidden=move || preview.get() disabled=move || editor.saving.get() prop:value=move || editor.draft.get() on:input=move |e| editor.draft.set(event_target_value(&e))></textarea>
        <div hidden=move || !preview.get()>{move || view! { <MarkdownPreview markdown=editor.draft.into() path=editor.path.get() select/> }}</div>
        <Show when=move || !creating><button type="button" disabled=move || editor.saving.get() on:click=move |_| navigation.attempt(move || { if let Some(doc) = document.get_untracked() { editor.open(&doc); } }, || {})>"Reload current document"</button></Show>
    }
}

#[component]
fn Relationships(
    document: RwSignal<Option<KnowledgeDocument>>,
    graph: RwSignal<Option<KnowledgeView>>,
    select: Callback<String>,
    edit: Callback<()>,
) -> impl IntoView {
    view! {
        <p>"Relationships live in the document’s frontmatter."</p><button type="button" on:click=move |_| edit.run(())>"Edit relationships in Markdown"</button>
        {move || document.get().map(|doc| {
            let mut groups = vec![("Broader documents",doc.parents),("More detail",doc.children),("Dependents",doc.dependents),("Related documents",doc.related)];
            let depends = graph.with(|g| g.as_ref().map(|g| g.relations.iter().filter(|e| e.from == doc.summary.path && e.kind == KnowledgeRelationKind::DependsOn).map(|e| e.to.clone()).collect()).unwrap_or_default());
            groups.push(("Depends on",depends));
            view! { <div class="knowledge-relationships">{groups.into_iter().map(|(title,paths)| view! { <section><h3>{title}</h3>{paths.is_empty().then(|| view! {<p class="muted">"None"</p> })}<ul>{paths.into_iter().map(|path| { let label = graph.with(|g| g.as_ref().and_then(|g| g.documents.iter().find(|d| d.path == path)).map(|d| d.title.clone())).unwrap_or_else(|| path.clone()); view! { <li><button type="button" on:click=move |_| select.run(path.clone())>{label}</button></li> } }).collect_view()}</ul></section> }).collect_view()}<section><h3>"Source references"</h3><ul>{doc.metadata.sources.into_iter().map(|path| view! { <li><code>{path}</code></li> }).collect_view()}</ul></section></div> }
        })}
    }
}

#[component]
fn Diagnostics(
    graph: RwSignal<Option<KnowledgeView>>,
    select: Callback<String>,
    refresh: RwSignal<u64>,
) -> impl IntoView {
    view! { <p>"Check document structure, identity, and relationships. These checks do not establish semantic consistency with code."</p><button type="button" on:click=move |_| refresh.update(|v| *v += 1)>"Check now"</button>
        {move || graph.get().map(|graph| view! { {graph.diagnostics.is_empty().then(|| view! {<p>"No structural diagnostics."</p> })}<ul class="knowledge-diagnostic-list">{graph.diagnostics.into_iter().map(|d| { let path = d.path.clone(); let visible = graph.documents.iter().any(|doc| doc.path == path); view! { <li class="knowledge-diagnostic"><strong>{d.code.replace('_'," ")}</strong><p>{d.message}</p><button type="button" disabled=!visible on:click=move |_| select.run(path.clone())>{d.path}</button></li> } }).collect_view()}</ul> })}
    }
}

#[component]
fn MarkdownPreview(
    markdown: Signal<String>,
    path: String,
    select: Callback<String>,
) -> impl IntoView {
    view! { <article class="knowledge-markdown-preview" inner_html=move || self::markdown::render(&markdown.get(),&path) on:click=move |event| follow_document_link(event,select)></article> }
}

#[cfg(target_arch = "wasm32")]
fn follow_document_link(event: leptos::ev::MouseEvent, select: Callback<String>) {
    use wasm_bindgen::JsCast;
    if let Some(anchor) = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .and_then(|e| e.closest("a").ok().flatten())
    {
        if let Some(href) = anchor
            .get_attribute("href")
            .and_then(|href| href.strip_prefix("#knowledge:").map(str::to_owned))
        {
            event.prevent_default();
            let path = href.split('#').next().unwrap_or(&href);
            if let Ok(path) = urlencoding::decode(path) {
                select.run(path.into_owned());
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn follow_document_link(_event: leptos::ev::MouseEvent, _select: Callback<String>) {}
#[cfg(target_arch = "wasm32")]
fn restore_tool_focus(tool: Tool) {
    use wasm_bindgen::JsCast;
    if let Ok(Some(button)) =
        document().query_selector(&format!("[data-knowledge-tool=\"{}\"]", tool.title()))
    {
        if let Ok(button) = button.dyn_into::<web_sys::HtmlElement>() {
            let _ = button.focus();
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn restore_tool_focus(_tool: Tool) {}

#[cfg(target_arch = "wasm32")]
fn focus_drawer() {
    use wasm_bindgen::JsCast;
    if let Ok(Some(element)) = document().query_selector(".knowledge-tool-drawer.open input:not(:disabled), .knowledge-tool-drawer.open textarea:not(:disabled), .knowledge-tool-drawer.open .knowledge-tool-body button") {
        if let Ok(element) = element.dyn_into::<web_sys::HtmlElement>() { let _ = element.focus(); }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn focus_drawer() {}

fn error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError(message) => message,
        error => error.to_string(),
    }
}
