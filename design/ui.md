# UI Design

Dispatch's web UI is an operator surface for project setup, workflow visibility, automation control, and admin maintenance. It is server-rendered and hydrated with Leptos.

## Routes

Primary UI routes include:

```text
/                                      current workflow surface
/project                               selected-project administration
/projects                              project collection administration
/automation                            automation rule administration
/runs                                  automation run visibility
/system                                system and Codex administration
/api/docs                              local API reference
/projects/:project/items/:item_id      item detail
/projects/:project/automation/runs/:run_id/log
/error
```

The UI should keep project context visible and avoid hiding workflow state behind generic admin tables.

Dispatch-owned styling uses a compact density by default: small gaps, restrained panel padding, and no decorative whitespace that displaces workflow content. Relative `em` units and shared style tokens are the source of truth for spacing, type, controls, radii, breakpoints, and major layout dimensions so density stays consistent and can be tuned centrally. Generated CrudKit and Leptonic styles remain upstream-owned.

## Workflow Surface

The main workflow surface should make these states easy to inspect:

- backlog and all work;
- project-defined swim-lanes based on lane filters and lane ordering;
- same-group work item cards collected under a visible group header within each lane;
- in-progress work and claimant, including the triggering automation source when available and a frontend-derived elapsed claim timer from the claim start time;
- recent comments and progress;
- automation status;
- stale or blocked work;
- feedback-requested work that is waiting for a user answer;
- run logs and run outcomes, including linked operated work items, separate developer-instructions and user-prompt sections before output, live output for active runs, stable selected-run inspection during live refreshes, active-run cancellation, commit outcome, and created commit SHA visibility.
- per-run Codex token usage when reported by the agent runtime.
- Dispatch-owned workflow labels such as `state`, `dispatch:claimed-from-state`, `dispatch:automation-blocked`, and `dispatch:feedback-requested`.

Board and item-detail interactions call server actions or custom API endpoints so workflow rules remain centralized.
Run output renders as a compact, light-theme-readable timeline rather than a panel per raw stream event. Natural-language model output is the strongest visual element; commands, tool metadata, and command output use muted colors. Started and completed tool events with the same item id collapse into one visible entry, internal ids and ordinary turn bookkeeping are hidden by default, and successful zero exit codes do not add redundant badges. Completed command rows use the `Ran <command>` presentation. Command output shows at most two lines before inline expansion, and the revealed output remains the disclosure target so clicking it collapses the entry again; diff-like output continues to highlight added and removed lines. Unambiguous shell-wrapped single-file reads using `cat`, `sed -n`, `head`, or `tail` render as `Exploring <file>...`; complex, chained, multi-file, or otherwise ambiguous commands retain their recorded command presentation.

Completed or stale reasoning entries are hidden by default. An active run shows `Thinking...` only while its latest output event is an unmatched started reasoning item. When historical reasoning exists, a tiny client-side per-run switch restores it in timeline order as compact timing rows; reasoning with an empty body never creates an empty collapsible region. The switch survives live refreshes while the current page remains mounted but is not persisted across navigation or reload.
Hydrated item-detail label and comment controls save through the typed item service and keep the item page mounted, including current scroll position and nearby draft state.
Item detail keeps consistent spacing between its workflow panels whether rendered as a full page or inside the Board drawer. Label editor rows show the editable key and value once rather than repeating a summary chip beside identical fields; the Dispatch-owned `state` row instead identifies its fixed `State` key and exposes only the project-backed value picker. Label actions wrap as a distinct group without colliding with fields at drawer widths.
The selected project's compact workspace overview is a shared application dock pinned to the bottom of the browser window on every route. It keeps project path, Git status, copy, folder, and available editor actions visible without duplicating a Workspace panel in page content. The Board starts directly with its swim-lanes. It does not repeat the selected project as a page heading, show a general new-item action, expose runtime paths, or include project administration and maintenance panels. Every Board work item card shows its item id as `#{id}` at the leading edge of the muted footer metadata instead of showing the item's internal version.
Board swim-lanes always fill the height available between the top bar and the bottom workspace dock, including when a lane is empty. Overflowing work item cards scroll inside each lane so large lanes do not lengthen the page or continue behind the workspace dock.
Plain primary clicks on a Board card open the full item workflow in a right-side Leptonic drawer while leaving the uncovered desktop Board interactive. The card title, description, and labels remain one native link to the canonical item route, so middle-clicks and modified clicks keep normal full-page and new-tab behavior. Cards with linked automation runs show the total run count and the newest three status-colored run links; a plain primary click on a preview reuses the drawer for full run detail. When a displayed run owns the item's active claim, its row also shows the triggering automation source when available and a frontend-derived elapsed claim timer. Board cards do not repeat that active-run information in a separate progress pill.
Selecting the item or run already shown in the Board drawer is a no-op: it does not navigate, reload drawer data, or disturb in-progress editor state.

Board drawer state is encoded as `/?project=<project>&item=<id>[&run=<id>]`. Opening a drawer from a closed Board adds one history entry, while switching items or moving between item and run detail replaces that drawer entry. Browser Back closes the drawer, Forward restores it, and direct drawer URLs restore after reload. Missing, invalid, cross-project, or incorrectly linked records surface a brief error toast and clear the drawer without replacing the Board shell. Canonical item and run-log routes remain the explicit full-page destinations.

The drawer opens from the right. At desktop widths it occupies a second Board column below the always-visible top bar instead of covering the Board. Its default width is approximately 46% of the Board and a visible-on-hover handle on its left edge lets the operator resize it from 30% through 70%; the handle also supports keyboard resizing. The remaining Board column stays interactive and presents its swim lanes as one horizontally scrollable strip. Opening or resizing the drawer does not compress the lanes: they retain the width they would have in the closed, full-width Board, so every lane remains readable and reachable by horizontal scrolling. The drawer is shorter than the viewport, scrolls internally, and restores focus to the originating card when closed. Below 900px it uses the full available Board width below the top bar with a modal-style backdrop over the Board region and does not expose the resize handle. Close, Escape, Back, item/run switching, and canonical full-page actions all create navigation attempts through the embedded CrudKit editor's dirty guard; cancelling the leave confirmation restores the current drawer URL and selection without discarding the draft. The item drawer reuses the full item-detail workflow, and successful item deletion closes it. The run drawer reuses the full run metadata, prompts, actions, and compact output timeline and exposes `Back to item`, `Open full run`, and close actions.
Human-authored rich prose fields such as work item descriptions and automation prompts should use the Tiptap-backed editor in create and edit flows, while structured multiline fields such as selectors, writable-root lists, memory history, and commit policy text stay plain text controls.
Ordinary work item create and edit fields may be embedded CrudKit forms, including the Board new-item modal and item-detail editor, so those flows share field configuration and CrudKit dirty guards while Dispatch workflow controls remain custom. Item detail pages show the work item id in the top heading as `#{id}` with the title, so the item-detail editor does not repeat the id as a disabled input field.
The Board new-item modal lets operators add zero or more initial labels before saving. The state selector remains the canonical source for the `state=<value>` label; initial-label rows must not create their own `state` label, and CrudKit dirty guards cover edits to those rows.

Embedded CrudKit surfaces use `CrudNavigation` for all view changes, returns, and application-owned actions that could discard edited data. Navigation retains its accepted view while a URL change, drawer selection, modal close, or full-page action waits for one aggregate leave confirmation covering the targeted navigation scope. Accepting performs the navigation attempt exactly once. Cancelling keeps the accepted view and draft mounted and restores application-owned state such as the Board drawer URL. A nested drawer or editor protects only its own navigation subtree, while an attempt through an instance navigation scope includes every dirty descendant editor; dirty siblings do not block an attempt through a child navigation scope. A second navigation attempt cannot replace one already awaiting confirmation. Successful persistence uses committed follow-up navigation, so create, save-and-return, and delete do not show a second leave confirmation.
The application-level `CrudInstanceMgr` owns Dispatch's shared root navigation scope, and default CrudKit instances
automatically mount descendant navigation below it. Plain primary clicks on brand, primary-navigation, and Codex-status
links, plus project-switcher selections, attempt their same-tab route change through the manager navigation scope so every mounted
dirty CrudKit descendant participates in one leave confirmation. Native middle-click and modified-click behavior remains
unchanged because opening another tab does not discard the current editor.

Return callbacks are explicit navigation configuration, not component lifecycle callbacks. Closing or unmounting a route, modal, drawer, or whole CrudKit instance never invokes its configured return callback by itself.

Item detail pages show a relationships panel for every directed relationship touching the current work item. The panel distinguishes outgoing links where the current item is the source from incoming links where the current item is the target, shows the free-form relationship kind, shows source and target item id/title/state summaries, and links to the related item. Relationship add, update-kind, and delete controls call the typed item service, which uses the custom relationship service rather than CrudKit routes.

## Admin Surfaces

CrudKit is appropriate for ordinary resource administration:

- projects;
- work items;
- work item states;
- swim-lanes;
- comments;
- agent tools;
- agent runs;
- automation rules.
- personalities.

Dispatch-specific actions such as claim, release, finish, request feedback, automation launch, stale-claim recovery, and run-log viewing should remain custom UI flows. These actions carry workflow semantics that generic CRUD controls should not duplicate.

The Projects page stands on its own as collection administration for creating, listing, editing, and deleting projects. Selected-project administration lives on the singular Project page. That page owns the selected project's system prompt and memory, full Work items administration, work item states, swim-lanes, and maintenance actions such as worktree cleanup. The Automation page owns project automation policy alongside automation rules and personalities. System-wide Codex readiness and app-server tool configuration live on the System page rather than Projects.

Project selection is explicit URL-owned state. Pages and the workspace dock do not silently select
the first project when the URL has no project. When the selected project is deleted, the live
project-deleted event clears its typed browser caches and navigates to the unselected Projects page
without choosing a replacement. The project switcher shows a choose-project placeholder until the
operator explicitly selects another project.

Full Work items administration, work item state authoring, and swim-lane authoring live on the selected-project administration surface, not the main board or project collection. The board shows small lane edit controls that navigate to the selected swim-lane editor. New item creation is lane-scoped: eligible state-backed lanes show `+ Add` in the lane header and preselect that lane's state. The add control may appear on lane hover or keyboard focus on precise-pointer devices, but remains visible on narrow, coarse-pointer, and non-hover devices. Swim-lane filter create and edit forms expose structured label-condition controls for nested `All`/`Any` groups, label presence, flag labels, string equality, string inequality, and string-list membership while continuing to store the existing CrudKit `Condition` JSON string; invalid or unsupported existing filters remain editable through a raw JSON escape hatch.
On item detail pages, the `state` label's value editor should render as a state picker backed by the current project's authored work item states instead of a free-text value field. That picker submits through the item move/update workflow path, while ordinary label rows use generic label add/update/delete handlers.

The Codex app-server status panel should guide setup failures directly. When
Dispatch's managed Codex home is not signed in, the panel shows the exact
`CODEX_HOME` login command, the managed home path, and a refresh action instead
of relying on users to reconstruct the command from server logs.
The System page presents Codex readiness first and keeps Codex app-server tool
configuration in a separate section at the bottom. Tool discovery is exposed as
the agent-tools table's `Check Codex` resource action rather than a standalone
section action.
Codex readiness outside `/system` is a server-startup or pre-run snapshot rather
than a globally polled value. Mounting `/system` performs a detailed status check
and keeps that page current with a five-minute refresh while it remains mounted;
server-side single-flight caching prevents duplicate tabs or live-event refetches
from multiplying detailed probes. The operator-triggered Refresh action bypasses
that cache and performs a new detailed check immediately. Detailed checks include
token-activity usage; ordinary readiness checks omit that optional usage request.
After completing browser login, the operator uses Refresh to update the displayed
account state immediately.

## Project Settings

Project settings should expose:

- filesystem path, path health, and Git repository status;
- copy/open actions for the project folder and available RustRover or VS Code editor targets;
- system prompt and memory;
- system prompt and memory history snapshots, with manual history compaction;
- workspace mode;
- agent concurrency for mutating and read-only automation;
- pull request creation;
- current-branch auto-commit behavior;
- commit standard text for generated agent commit messages;
- current-branch failure revert strategy;
- mutable Git command policy as structured controls for `git add`, `git commit`, `git push`, `git reset`, and hard-reset mode;
- stale-claim timeout;
- worktree cleanup policy;
- default agent tool, model, and reasoning effort.

Settings changes should go through server handlers and be reflected in automation launches without requiring agents to know settings internals.
The model and reasoning effort controls should prevent known-incompatible Codex combinations, while server handlers remain authoritative for all API, CLI, and frontend-service submissions.
Selector/prompt-based automations do not expose a project-level refinement concurrency exception in settings. Read-only automation concurrency is a general setting, not a refinement-specific bypass.

Codex configuration generated from project settings should not be exposed as raw TOML in the main UI. Operators configure supported policy fields, and Dispatch generates the per-project Codex config and rules.

When a selected project uses the current-branch workspace mode, the top bar should include an Auto-Commit toggle next to the automation Start/Stop control so operators can quickly decide whether completed current-branch work should be committed by the agent.

Quick settings controls such as the top-bar Auto-Commit toggle use the Leptonic toggle component, update optimistically in the hydrated UI, and send the persistence request through the typed project service in the background. If the request fails, the control should roll back to its previous state instead of navigating or reloading the page.

The application dock is the only workspace-opening surface. Its actions use the selected project's configured path. Editor opening is a server-local fixed allowlist for RustRover and VS Code; unavailable editors should not be shown, and browser requests must not accept arbitrary commands. The dock should state whether the project path is in a Git repository and, when it is, show the current branch plus added/deleted line counts. Run detail views keep the recorded working directory as plain metadata.

Automation rule administration should show and edit each work-consuming rule's mutability with `mutating` and `read_only` choices and its selected project-local personality by name. Automation rule lists should show personality names rather than raw ids. Personality administration lives near automation administration as a normal project-scoped admin resource with create, edit, list, and delete controls. Personality descriptions are plain multiline prompt text. Deleting `Default` or any personality still referenced by automation should surface the server's clear rejection message. Automation status should show total running runs plus separate mutating and read-only counts, and run list/detail views should display the persisted run mutability so historical logs remain understandable after a rule changes.

Automation administration also exposes a visible installed-bundle inventory with managed-object counts, YAML file selection into the editor, validate/diff/apply/export, and an inline confirmed removal action. It also exposes managed-object identity, produced-work configuration, exclusivity, rule execution limits, concurrency groups, timeouts, and semantic postconditions. Managed objects reject individual edit/delete until detached. Operator diagnostics show route winners and admission blockers; revision, evaluation, and analytics data support historical inspection and restore.

Item detail displays immutable origin with links to a source run and automation context where available. Run detail displays trigger/personality revisions, system-prompt event, effective input hash and timeout/group, semantic outcome/failures, and attributed created/modified item links. Exact model input remains in the role-separated run artifacts.

## Live Updates

The UI uses project and item event streams to refresh workflow state. Event streams are hints for refreshing the current view; persisted records remain the source of truth.

Route components render a stable page shell and top bar immediately. Navigation, the route heading, and any data-independent controls must not be hidden behind a page-level resource, suspense boundary, or `Loading...` fallback. Controls that need server state render from route information, cached values, or safe defaults and then update reactively. A route must never replace its entire visible page with a loading state.

Typed server-function responses flow into reactive signals that update only the elements and panels which depend on the returned data. Refreshes keep already rendered content mounted until replacement data is available. Frontend services surface request failures through brief Leptonic error toasts; a failure does not replace or unmount the route.

Idempotent read requests exposed by frontend services are cached by their focused service. Persistent browser caches use `leptos_use` local-storage signals, are keyed by the request input, and retain typed DTOs. Route rendering reads a cached value synchronously and then revalidates it in the background; a successful response updates the cache and only the dependent reactive DOM. Mutation requests are not cached. Serialized response strings and application JSON blobs are not retained as the in-memory cache representation.

Each operator route has its own module under `dispatch-server/src/frontend/pages/`. Route modules own their page response type, cached query signal, and page-specific reactive rendering, but the query does not gate the route shell or top bar. View-bearing route sections and shared controls are Leptos `#[component]`s; Dispatch-owned mutation controls are reactive buttons, toggles, and editors rather than HTML form submissions. Focused, context-provided services own server functions, browser request clients, local-storage caches, and transport details; pages load or mutate backend state only through typed service methods. Service request implementations are replaceable for component and page tests.

The application module owns only the shell and root providers. Genuinely shared UI behavior is split into focused modules under `dispatch-server/src/frontend/components/` rather than accumulating in the application module.

## Browser Coverage

The browser suite entry point lives in `dispatch-server/tests/browser_test.rs`. It starts the
Dispatch test application, configures the browser runner, and registers the named test cases. Each
named browser test lives in its own module under `dispatch-server/tests/browser_test_suite/`;
reusable application, fixture, request, WebDriver, modal, and layout support lives under
`browser_test_suite/common/`. The suite is run explicitly with:

```text
just browser-test
```

The browser test should continue to cover UI placement and workflow visibility after changes to Leptos layouts, generated admin surfaces, or automation controls.

Automation browser coverage includes the structured/raw selector boundary, YAML file selection, bundle diff/apply/inventory/removal, managed identity and detach behavior, revision restore, produced-work and postcondition fields, provenance links, admission diagnostics, semantic failure visibility, and grouped cards on the board. The optional `examples/automation/engineering-review.yaml` fixture proves exact lens production and grouping plus planner, scout, verifier, and implementation routing without introducing review semantics into Dispatch itself; it is never auto-applied.
