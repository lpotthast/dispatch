use crate::{
    frontend::pages::RuntimeConfigView,
    shared::view_models::{AgentRunView, ProjectGitStatusView, ProjectView, WorkspaceEditorView},
};
use leptos::prelude::*;

use super::encode_path;

pub(crate) fn project_workspace_panel(
    project: &str,
    project_view: &ProjectView,
    workspace_editors: Vec<WorkspaceEditorView>,
    return_to: String,
) -> AnyView {
    workspace_actions(
        "Path",
        project_view.path.clone(),
        Some(project_view.path_exists),
        project_view.git_status.clone(),
        Some(format!("/projects/{}/workspace/open", encode_path(project))),
        workspace_editors,
        return_to,
    )
}

pub(crate) fn run_workspace_actions(
    project: &str,
    run: &AgentRunView,
    workspace_editors: Vec<WorkspaceEditorView>,
    return_to: String,
) -> AnyView {
    workspace_actions(
        "working dir",
        non_empty_string(run.working_dir.clone()),
        None,
        None,
        Some(format!(
            "/projects/{}/automation/runs/{}/workspace/open",
            encode_path(project),
            run.id
        )),
        workspace_editors,
        return_to,
    )
}

fn workspace_actions(
    label: &'static str,
    path: Option<String>,
    path_exists: Option<bool>,
    git_status: Option<ProjectGitStatusView>,
    open_action: Option<String>,
    workspace_editors: Vec<WorkspaceEditorView>,
    return_to: String,
) -> AnyView {
    let path = path.and_then(non_empty_string);
    let copy_available = path.is_some();
    let open_available = copy_available && path_exists.unwrap_or(true);
    let display_path = path.clone().unwrap_or_else(|| "not configured".to_owned());
    let copy_path = path.clone().unwrap_or_default();
    let (copy_message, set_copy_message) = signal(None::<String>);
    let status = path_exists.map(|exists| {
        view! {
            <span class=if exists {
                "workspace-status workspace-status-ok"
            } else {
                "workspace-status workspace-status-missing"
            }>
                {if exists { "Exists" } else { "Missing" }}
            </span>
        }
    });
    let git_status = git_status.map(workspace_git_status);
    let open_controls = open_action.map(|action| {
        let folder_action = action.clone();
        let folder_return = return_to.clone();
        let editor_controls = workspace_editors
            .into_iter()
            .map(|editor| {
                let editor_action = action.clone();
                let editor_return = return_to.clone();
                let target = editor.target.clone();
                let label = format!("Open {}", editor.label);
                let icon_src = workspace_editor_icon_src(&editor.target);
                view! {
                    <form method="post" action=editor_action>
                        <input type="hidden" name="target" value=target/>
                        <input type="hidden" name="return_to" value=editor_return/>
                        <button type="submit" class="secondary workspace-button" disabled=!open_available>
                            {icon_src.map(|src| view! {
                                <img class="workspace-button-icon" src=src alt="" aria-hidden="true"/>
                            })}
                            <span>{label}</span>
                        </button>
                    </form>
                }
            })
            .collect::<Vec<_>>();
        view! {
            <>
                <form method="post" action=folder_action>
                    <input type="hidden" name="target" value="folder"/>
                    <input type="hidden" name="return_to" value=folder_return/>
                    <button type="submit" class="secondary workspace-button" disabled=!open_available>
                        "Open folder"
                    </button>
                </form>
                {editor_controls}
            </>
        }
    });
    let path_for_copy = copy_path.clone();

    view! {
        <div class="workspace-actions">
            <div class="workspace-path">
                <span class="workspace-label">{label}</span>
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
                {open_controls}
                {move || {
                    copy_message
                        .get()
                        .map(|message| view! { <span class="workspace-copy-status">{message}</span> })
                }}
            </div>
        </div>
    }
    .into_any()
}

fn workspace_git_status(status: ProjectGitStatusView) -> AnyView {
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

pub(crate) fn runtime_panel(runtime: RuntimeConfigView, return_to: String) -> AnyView {
    view! {
        <section class="runtime-panel panel">
            <div class="panel-heading">
                <h2>"Runtime"</h2>
            </div>
            <div class="runtime-paths">
                {database_path_actions(&runtime, return_to)}
                {readonly_path_row("Database directory", runtime.database_directory)}
                {readonly_path_row("Codex home", runtime.codex_home_path)}
                {readonly_path_row("Codex config", runtime.codex_config_path)}
            </div>
        </section>
    }
    .into_any()
}

fn database_path_actions(runtime: &RuntimeConfigView, return_to: String) -> AnyView {
    let database_path = runtime.database_path.clone();
    let path_for_copy = database_path.clone();
    let (copy_message, set_copy_message) = signal(None::<String>);

    view! {
        <div class="workspace-actions">
            <div class="workspace-path">
                <span class="workspace-label">"Database file"</span>
                <code>{database_path}</code>
                <span class="workspace-status workspace-status-ok">"Active"</span>
            </div>
            <div class="workspace-buttons">
                <button
                    type="button"
                    class="secondary workspace-button"
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
                <form method="post" action="/system/database/open">
                    <input type="hidden" name="return_to" value=return_to/>
                    <button type="submit" class="secondary workspace-button">
                        "Open directory"
                    </button>
                </form>
                {move || {
                    copy_message
                        .get()
                        .map(|message| view! { <span class="workspace-copy-status">{message}</span> })
                }}
            </div>
        </div>
    }
    .into_any()
}

fn readonly_path_row(label: &'static str, path: String) -> AnyView {
    view! {
        <div class="workspace-actions">
            <div class="workspace-path">
                <span class="workspace-label">{label}</span>
                <code>{path}</code>
            </div>
        </div>
    }
    .into_any()
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
