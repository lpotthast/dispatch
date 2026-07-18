use crate::frontend::{pages::WorkspaceBarData, services::project_service};
use crate::shared::view_models::{ProjectGitStatusView, ProjectView, WorkspaceEditorView};
use leptos::prelude::*;
use leptos_use::{UseElementSizeReturn, use_element_size};

use super::{cached_query, selected_project_signal};

#[derive(Clone, Copy)]
struct WorkspaceDockSize {
    height: RwSignal<Option<f64>>,
}

pub(crate) fn provide_workspace_dock_size() {
    provide_context(WorkspaceDockSize {
        height: RwSignal::new(None),
    });
}

pub(crate) fn workspace_dock_height() -> RwSignal<Option<f64>> {
    expect_context::<WorkspaceDockSize>().height
}

#[component]
pub(crate) fn WorkspaceBar() -> impl IntoView {
    let dock_ref = NodeRef::<leptos::html::Footer>::new();
    let UseElementSizeReturn { height, .. } = use_element_size(dock_ref);
    let dock_height = workspace_dock_height();
    Effect::new(move |_| {
        let height = height.get();
        if height > 0.0 {
            dock_height.set(Some(height));
        }
    });
    let selected_project = selected_project_signal();
    let service = project_service();
    let initial = service.cached_workspace_bar_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |selected_project| service_for_cache.cached_workspace_bar(selected_project),
        move |selected_project| {
            let service = service_for_load.clone();
            let selected_project = selected_project.clone();
            async move { service.load_workspace_bar(selected_project).await }
        },
    );
    view! {
        <footer class="workspace-dock" aria-label="Workspace" node_ref=dock_ref>
            <div class="workspace-bar">
                <span class="workspace-bar-title">"Workspace"</span>
                {move || {
                    result.value.get().map(|data| view! { <WorkspaceBarContent data/> })
                }}
            </div>
        </footer>
    }
}

#[component]
fn WorkspaceBarContent(data: WorkspaceBarData) -> impl IntoView {
    match data.project {
        Some(project) => view! {
            <WorkspaceDockContent project workspace_editors=data.workspace_editors/>
        }
        .into_any(),
        None => view! { <span class="muted">"Choose a project to inspect its workspace."</span> }
            .into_any(),
    }
}

#[component]
fn WorkspaceDockContent(
    project: ProjectView,
    workspace_editors: Vec<WorkspaceEditorView>,
) -> impl IntoView {
    let ProjectView {
        name,
        path,
        path_exists,
        git_status,
        ..
    } = project;
    let path = path.and_then(non_empty_string);
    let copy_available = path.is_some();
    let open_available = copy_available && path_exists;
    let display_path = path.clone().unwrap_or_else(|| "not configured".to_owned());
    let path_for_copy = path.unwrap_or_default();
    let (copy_message, set_copy_message) = signal(None::<String>);
    let status = view! {
        <span class=if path_exists {
            "workspace-status workspace-status-ok"
        } else {
            "workspace-status workspace-status-missing"
        }>
            {if path_exists { "Exists" } else { "Missing" }}
        </span>
    };
    let git_status = git_status.map(|status| view! { <WorkspaceGitStatus status/> });
    let editor_controls = workspace_editors
        .into_iter()
        .map(|editor| {
            let target = editor.target.clone();
            let label = format!("Open {}", editor.label);
            let icon_src = workspace_editor_icon_src(&editor.target);
            view! {
                <WorkspaceOpenButton
                    project=name.clone()
                    target
                    label
                    icon_src
                    disabled=!open_available
                />
            }
        })
        .collect::<Vec<_>>();

    view! {
        <div class="workspace-actions">
            <div class="workspace-path">
                <code>{display_path}</code>
                {status}
            </div>
            {git_status}
            <div class="workspace-buttons">
                <button
                    type="button"
                    class="secondary workspace-button"
                    disabled=!copy_available
                    on:click=move |_| {
                        copy_workspace_text(
                            path_for_copy.clone(),
                            "Copied path",
                            set_copy_message,
                        );
                    }
                >
                    "Copy path"
                </button>
                <WorkspaceOpenButton
                    project=name
                    target="folder".to_owned()
                    label="Open folder".to_owned()
                    icon_src=None
                    disabled=!open_available
                />
                {editor_controls}
                {move || {
                    copy_message
                        .get()
                        .map(|message| view! { <span class="workspace-copy-status">{message}</span> })
                }}
            </div>
        </div>
    }
}

#[component]
fn WorkspaceOpenButton(
    project: String,
    target: String,
    label: String,
    icon_src: Option<&'static str>,
    disabled: bool,
) -> impl IntoView {
    let service = project_service();
    let (pending, set_pending) = signal(false);
    let open = move |_| {
        if pending.get_untracked() || disabled {
            return;
        }
        set_pending.set(true);
        let service = service.clone();
        let project = project.clone();
        let target = target.clone();
        leptos::task::spawn_local(async move {
            let _ = service.open_workspace(project, target).await;
            set_pending.set(false);
        });
    };

    view! {
        <button
            type="button"
            class="secondary workspace-button"
            disabled=move || disabled || pending.get()
            on:click=open
        >
            {icon_src.map(|src| view! {
                <img class="workspace-button-icon" src=src alt="" aria-hidden="true"/>
            })}
            <span>{label}</span>
        </button>
    }
}

#[component]
fn WorkspaceGitStatus(status: ProjectGitStatusView) -> impl IntoView {
    if !status.is_repository {
        let message = match status.error {
            Some(error) => view! {
                <span class="workspace-status workspace-status-missing" title=error>
                    "Git unavailable"
                </span>
            }
            .into_any(),
            None => view! {
                <span class="workspace-status workspace-status-neutral">
                    "Not a Git repository"
                </span>
            }
            .into_any(),
        };
        return view! { <div class="workspace-git-status">{message}</div> }.into_any();
    }

    let branch = status.branch.unwrap_or_else(|| "unknown branch".to_owned());
    let additions = format!("+{}", status.added_lines);
    let deletions = format!("-{}", status.deleted_lines);
    let diff_status = status.error.map(|error| {
        view! {
            <span class="workspace-status workspace-status-missing" title=error>
                "Diff unavailable"
            </span>
        }
    });

    view! {
        <div class="workspace-git-status">
            <span class="workspace-status workspace-status-ok">"Git repository"</span>
            <span class="workspace-git-branch">{branch}</span>
            <span class="workspace-git-diff" aria-label="Git line diff">
                <span class="workspace-git-added">{additions}</span>
                <span class="workspace-git-deleted">{deletions}</span>
            </span>
            {diff_status}
        </div>
    }
    .into_any()
}

fn workspace_editor_icon_src(target: &str) -> Option<&'static str> {
    match target {
        "rustrover" => Some("/icons/workspace-rustrover.svg"),
        "vscode" => Some("/icons/workspace-vscode.svg"),
        _ => None,
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn copy_workspace_text(
    text: String,
    success_message: &'static str,
    set_copy_message: WriteSignal<Option<String>>,
) {
    leptos::task::spawn_local(async move {
        let message = match write_clipboard_text(text).await {
            Ok(()) => success_message.to_owned(),
            Err(err) => err,
        };
        set_copy_message.set(Some(message));
    });
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export async function dispatchCopyText(text) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.setAttribute('readonly', '');
  textarea.style.position = 'fixed';
  textarea.style.left = '-9999px';
  textarea.style.top = '0';
  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();
  const copied = document.execCommand('copy');
  textarea.remove();
  if (!copied) {
    throw new Error('Copy failed');
  }
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = dispatchCopyText)]
    async fn js_copy_text(text: &str) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}

#[cfg(not(feature = "ssr"))]
async fn write_clipboard_text(text: String) -> Result<(), String> {
    js_copy_text(&text)
        .await
        .map(|_| ())
        .map_err(js_error_message)
}

#[cfg(feature = "ssr")]
async fn write_clipboard_text(_text: String) -> Result<(), String> {
    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn js_error_message(value: wasm_bindgen::JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| "Copy failed".to_owned())
}
