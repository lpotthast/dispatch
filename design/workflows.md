# Workflows

Dispatch workflows are enforced by server services. The CLI and UI send intent; the server validates project scope, ownership, item state, and version safety.

## Claim

Claiming work assigns an eligible item to an agent.

Inputs include:

- project;
- agent id;
- desired source state, usually `open`.

The server chooses an unclaimed item from the requested state, skips items with `dispatch:automation-blocked` or `dispatch:feedback-requested`, records the source state in `dispatch:claimed-from-state`, marks the item `in_progress`, records claim ownership and timestamps, increments version, and emits workflow events. New claims capture the item's current `state` label as the release source and overwrite any stale `dispatch:claimed-from-state` label left on the item. Default automation requests the `open` state; user-defined automation selectors can target other labels but the blocked-label exclusion is implicit. `item claim` never defaults to `DISPATCH_CLAIMED_ITEM_ID`.

Claimable items must also be unfinished. Finished items are closed even if an operator later changes their `state` label to a value that matches a queue claim or automation selector.

If no eligible item exists, the API reports that condition without creating implicit work.

## Progress

Progress records an agent-authored status update on an item.

For the claimed item, launched agents normally run:

```text
dispatch item progress --body "Implemented parser split."
```

The server verifies that the item belongs to the project and that the caller can update the item. It then appends a comment, records an event, and updates item metadata.

## Finish

Finishing work records a completion report and closes the active item.

For the claimed item, launched agents normally run:

```text
dispatch item finish --report "Done. Verified with cargo test."
```

The server validates claim ownership, appends the completion report, marks the item `done`, clears active claim ownership, records finish metadata, increments version, and emits events.

## Release

Releasing work returns a claimed item to the pool without marking it done.

For the claimed item, launched agents normally run:

```text
dispatch item release --comment "Blocked by missing credentials."
```

The server validates claim ownership, appends the optional release comment, clears active claim ownership, restores the `state` label to the value captured in `dispatch:claimed-from-state`, increments version, and emits events. Agent-facing releases also add `dispatch:automation-blocked` so the item is not picked up again until a user or agent intentionally removes that label. Server-side claimable releases used for successful unfinished runs, stale-claim recovery, and cancellation clear transient workflow blockers such as `dispatch:automation-blocked` and `dispatch:feedback-requested` so the restored item can actually re-enter claim selection.

## Request Feedback

Requesting feedback pauses a claimed item because the agent needs a user answer before continuing.

For the claimed item, launched agents normally run:

```text
dispatch item request-feedback --body "Which provider should this integration target?"
```

The server validates claim ownership, appends the feedback request as an agent comment, clears active claim ownership, restores the `state` label to the value captured in `dispatch:claimed-from-state`, removes the claim-source bookkeeping label, adds `dispatch:feedback-requested` and `dispatch:automation-blocked`, increments version, and emits events. Automation skips feedback-requested items until the label is removed after the user response is handled. A later successful claim clears the pending feedback label so it represents only an active feedback wait.

## Item Updates

General item edits use the item update endpoint, not workflow endpoints. Updates can change title, description, state, and per-item agent overrides.

A single item update request is applied as one versioned change, even when it edits both item fields and the state label. Version checks protect against overwriting newer item state. Workflow transitions still use dedicated operations because they contain additional business rules.
Generic label add, update, and delete operations reject the reserved `state` label so state changes cannot bypass item move/update semantics or emit only generic label events.

## Automation Launch

When Dispatch launches an agent, it:

1. resolves the published `dispatch` CLI from `PATH`, or explicitly builds it from sources when `DISPATCH_DEVELOPMENT=1`; a missing CLI rejects the launch before Codex starts or work is claimed;
2. prepends the CLI directory to `PATH`;
3. sets `DISPATCH_API_URL`;
4. sets `DISPATCH_PROJECT`;
5. sets `DISPATCH_AGENT_ID` as `dispatch-run-<run-id>`;
6. sets `DISPATCH_CLAIMED_ITEM_ID` when the run has claimed work;
7. omits `DISPATCH_DATABASE`;
8. omits database paths from the prompt.

Dispatch sends role-separated model input. Codex thread developer instructions contain Dispatch's execution contract, explicit instruction precedence, the effective sandbox/Git/commit policy, trusted project instructions, the selected personality, and the automation trigger instructions. The turn's user prompt starts with the claimed work-item title and description, followed by a launch-time item-state snapshot and project memory. Work-item descriptions and automation trigger prompts are converted from stored Tiptap HTML to Markdown at this boundary; Dispatch preserves the original rich text in storage.

Project memory remains in the user prompt because agents may write it. The prompt labels it as historical reference rather than instructions and requires drift-prone facts to be verified. Dispatch does not inject comment history into the launch prompt. Instead, claimed-item agents must fetch `dispatch item show --json` and `dispatch comment list --json` at startup and again before choosing a terminal transition, so current comments enter context only through live reads.

For work-consuming automation runs started from an automation rule, Dispatch resolves the rule's selected project-local personality before building the role-separated input. A non-empty personality is included before the automation-specific trigger instructions in the developer instructions. The empty `Default` personality is behavior-neutral and does not add a section. Work-producing automation and direct starts that do not launch from a personality-bearing consume-work rule do not need personality injection.

The developer contract gives the common claimed-item command path and points agents to `dispatch <command> --help` for less common operations instead of embedding the entire CLI grammar. Internal Git-shim environment variables are implementation details and are not advertised in model instructions. The effective run policy still enumerates the Git commands expected to work, commit/revert behavior, and sandbox blocker reporting.

For auditability, every run persists developer instructions and the user prompt as two independent Markdown files with separate database paths, API fields, CLI sections, and UI sections. Dispatch never combines new run input into one prompt artifact. When migrating a historical run whose legacy prompt artifact contains both roles, Dispatch preserves access by initializing both role-specific paths from that legacy path before removing the combined database column. If that migration is rolled back, equal role-specific paths reuse the shared artifact; differing paths are combined into a legacy Markdown artifact containing explicit developer-instructions and user-prompt sections before the role-specific columns are removed. The live Codex call supplies the same two role-specific strings through their respective SDK fields. Repository conventions remain in `AGENTS.md`, which Codex discovers from the run working directory rather than Dispatch duplicating them in its generated input.

## Automation Rule Behavior

Automation rules either produce work items or consume work items. Work-consuming automation has an explicit run mutability, either `mutating` or `read_only`, and a selected project-local personality. The rule prompt tells the launched agent how to handle the claimed item, including whether the expected outcome is implementation, refinement, verification, review preparation, or another project-specific workflow. The selected personality is a reusable prompt fragment injected before that trigger prompt.

Queued automation evaluations are consumed only while automation is running for that project. If an operator queues an evaluation for a stopped project, the pending evaluation count remains on the trigger until the project automation loop is active again; another project's running automation must not consume it.

When a launched agent exits successfully while its item is still claimed, Dispatch releases the temporary claim back to the claimed-from state and clears transient workflow blockers. This lets prompt-directed metadata, refinement, or verification consumers leave the underlying implementation work available for later automation, including after a manually triggered retry of previously blocked work. Failed runs still release with automation blocked so a broken prompt, missing context, or sandbox failure does not loop indefinitely. An agent can call `dispatch item request-feedback --body ...` when it needs a user answer before work can continue; it can call `dispatch item release --comment ...` for technical blockers or handoffs that need human triage but are not a concrete feedback request.

Dispatch ships editable default consumers for label-routed story preparation:

- a read-only refiner for items labeled `needs-refinement`;
- a read-only verifier for items labeled `needs-verification`.

Their prompts tell agents not to implement the work and not to call `dispatch item finish` for successful refinement or verification. The verifier may move an unnecessary item to a terminal workflow state only when that state is already evident from the project's user-defined workflow vocabulary; Dispatch does not hardcode a universal state value for that instruction.

Review-style work that should not run automatically is modeled as work-producing automation: a manual evaluation creates a review item with the expensive prompt, and a work-consuming automation can later run an agent against that item.

When one run creates several items that form one human-visible unit, it creates a stable project work group and submits all created item ids in one atomic assignment. Grouping does not imply ordering or dependency and does not replace relationships. The board renders same-group items together within each lane, and the item detail exposes the group key/name. A group may span lanes as its items move through independent states.

For Codex-backed launches, Dispatch prepares a project-specific Codex home before the run starts. The project home contains generated Codex config and rules derived from project settings, while shared Codex auth and skills are linked from Dispatch's shared managed Codex home when present. The run sets `CODEX_HOME` and `CODEX_SQLITE_HOME` to that project home so settings, rules, logs, sessions, and SQLite state are isolated per project.

Immediately before an actual Codex-backed run, Dispatch performs one minimal readiness probe that reads account and rate-limit state but not the optional token-activity summary. This is the authoritative pre-claim check. Starting the project automation scheduler does not perform an additional probe, and Dispatch does not poll readiness while idle. The probe app-server exits before run preparation continues; the app-server used for the agent turn exits when that run or recovery attempt ends.

Codex app-server stream transport interruptions that are consistent with host sleep, reconnect, broken pipe, or timeout behavior are recoverable. Dispatch keeps the run in `running`, keeps any claimed work item claimed, appends a concise recovery note to the active run output, and restarts Codex against the same persisted thread before asking it to continue. Recovery is bounded by an explicit retry limit. Explicit operator cancellation, project automation stop, server shutdown cancellation, the automation timeout, and non-retryable Codex turn failures remain terminal and continue to use the existing cancellation or failure release behavior.

Dispatch repairs stale shared-asset symlinks in a project Codex home before launch. During each app-server run, Dispatch captures a bounded stderr diagnostic alongside the structured run log. When launch or execution fails, the run summary and automatic claim-release comment put the root cause reported by Codex first, followed by the SDK and transport details; a generic transport closure must not hide an available process error.

## Project Deletion

Project deletion is an ordered server lifecycle, not a raw project-row delete. Dispatch first
closes run admission for the immutable project id, stops its automation scheduler, cancels every
registered run including runs that have not spawned a child process yet, and waits for all sessions
to finish. A session registered after deletion begins receives cancellation immediately.

After processes have stopped, Dispatch removes every project-owned runtime artifact: per-run
developer instructions, user prompts, structured output, Codex stderr diagnostics, Git policy
files, run shim directories, isolated Git worktrees, `dispatch/*` run branches, and the project's
managed Codex home. Missing artifacts are treated as already cleaned; any other cleanup failure
aborts the database deletion so the operator can correct the problem and retry. Dispatch never
deletes the configured source workspace itself.

Only after cleanup succeeds does Dispatch delete the project row and its cascading project data.
Both custom operator handlers and CrudKit deletion use this same lifecycle. Completion publishes a
project-deleted live event containing both the deleted id and name, so a same-name replacement is
never confused with the deleted project.

### Produced work

Before production, Dispatch validates the complete produced-work specification and records an evaluation at the current trigger revision. Deduplication and item creation occur in the same transaction. A duplicate evaluation records `skipped_duplicate` and the reused unfinished item without modifying it. New items atomically receive state, labels, item execution overrides, immutable origin, and `ItemCreated` attribution.

### Routing and admission

Ordinary matching rules retain fairness scoring. Routing is exclusive per item: if any due and admission-eligible exclusive rule matches an item, all non-exclusive matches for that item are suppressed. Exclusive rules use numeric priority first, then fairness and stable id for equal-priority ties. A candidate that becomes stale before claim is skipped while scanning continues.

Project mutating/read-only limits apply first, followed by per-rule caps and project-scoped concurrency-group mutexes. Routing explanation reports selector clauses, due/admission state, fairness, exclusive suppression, blockers, current winner, and bounded match examples for unsaved rules.

### Semantic postconditions

Dispatch captures item/label state, event and created-item lineage, workspace baseline, and configured revision before launch. After the process exits and commit validation runs—but before automatic release—it evaluates only events and item origins attributed to that run. Any complete alternative outcome set passes.

A semantic failure marks the run failed, persists each failed assertion, appends a readable log error, and preserves all item, Git, workspace, and metadata changes. If the item remains claimed, normal failed-run release/blocking applies. Dispatch never rewrites a finish, release, or feedback transition the agent already completed. Commit policy and semantic outcomes remain independently visible.

The optional engineering-review fixture makes the planner create exactly one item for each of six named lenses, assign all six to one group, and finish only when a postcondition verifies the exact total, each lens-specific selector, and shared non-empty group membership. Scout-created candidates inherit that human-visible grouping through explicit agent assignment; Dispatch itself does not understand review lenses or candidate semantics.

## Automation Concurrency

Mutating and read-only runs use independent admission limits. A mutating run can start only when the running mutating count is below the workspace-constrained code-edit allowance derived from `max_code_edit_agents`; current-branch projects still cap that allowance at one. A read-only run can start only when the running read-only count is below `max_read_only_agents`; setting the read-only limit to zero disables read-only automation admission. Running read-only runs do not consume mutating slots, and running mutating runs do not consume read-only slots.

Queued automation evaluation and work-item polling evaluate admission against the candidate trigger's mutability. Status views expose both running counts and the effective mutating allowance so skipped or rejected starts can explain which limit was reached.

## Workspaces

Project settings choose the workspace policy:

- current branch;
- dedicated Git branch;
- Git worktree.

When worktrees or branches are used, run records capture the working directory, branch, and cleanup status. Cleanup can be manual or automatic after successful runs, depending on project settings.

Read-only runs do not allocate isolated branches or worktrees. They use the project checkout as their working directory with a read-only Codex sandbox and a read-only sandbox policy with network access enabled. Read-only Codex launches ignore project writable-root settings and project sandbox mode because the run mutability requires no project writable roots.

## Commit And Revert Policy

Project settings define an automation commit policy. `auto_commit` defaults to on and controls whether current-branch mutating runs are instructed to commit completed work before finishing. Agents generate the commit message from the completed diff and follow the project commit standard text when it is configured, otherwise they infer the repository's existing commit style.

Current-branch runs are instructed to inspect the initial git status, commit completed work only when auto-commit is enabled, and revert their own changes before releasing incomplete work. The current-branch failure revert strategy defaults to manual revert and can be changed to Git reset for projects that intentionally allow that more destructive cleanup path.

Git branch and Git worktree runs are always instructed to commit before ending the run. If the work is incomplete in those modes, agents commit useful partial work and release the item with an explanation instead of reverting the workspace, because the isolated branch or worktree preserves context for follow-up work without polluting the base workspace.

Dispatch records the run-level commit requirement and final commit outcome. The server captures the workspace Git state before launching the agent and compares it with the state after the agent process exits. Runs record created commit SHA(s), `skipped_no_changes` when no new commit or workspace change was detected, `skipped_no_git_repo` when the workspace is not a Git repository, and `missing_required` when a required commit was absent while new uncommitted changes remained. Completed agent processes with `missing_required` are marked as failed at the run level; the server records this without rewriting item history that the agent already reported through workflow commands.

Project settings also define the mutable Git command policy. New and migrated projects allow `git add`, `git commit`, `git push`, and `git reset` by default. `git commit` must use `--no-verify`; Dispatch's Git guard injects it when omitted and rejects `--verify`. Pushes must not be force, mirror, prune, delete, empty-source delete-refspec, or `+ref` pushes. `git reset --hard` is allowed only when the hard-reset policy allows it for isolated Git branch or Git worktree runs; it is blocked for current-branch runs by default.

Dispatch expresses the broad allow-list through generated Codex rules in the project Codex home. A run-specific `git` shim remains necessary for argument checks that prefix rules cannot express, such as a force-push flag appearing after the remote name. The generated prompt includes the effective Git commands expected to work for the run.

Read-only runs always receive a run-level Git policy with `add`, `commit`, `push`, and `reset` disabled regardless of the project's mutating Git policy. They have no commit requirement, do not request pull requests, and record commit handling as not required. The generated prompt states that project files may not be edited, mutable Git commands are unavailable, no commit is required, and sandbox or Git blockers should be reported instead of worked around. Read-only runs may still update Dispatch-owned metadata through authorized CLI/API calls when their prompt asks for that work.

## Stale Claims

Projects define a stale-claim timeout. Server maintenance can recover expired claims by clearing ownership and making the item available again.

Claim recovery is a server workflow. Agents should release work explicitly when they cannot continue, but they do not perform database maintenance themselves.

## Run Logs

Automation output is captured by the server and exposed through run-log endpoints and UI routes. While a run is active, run-log views should use the in-memory session output so operators can inspect intermediate output before the persisted log file is written. Agents and tools should request logs through the API instead of reading log paths directly.

## Pull Requests

When project settings request pull request creation, successful mutating automation can run the configured GitHub CLI flow from the prepared workspace and record the resulting PR URL on the run.

Pull request creation is a server-side post-run operation. Failure to create a PR should be recorded on the run without rewriting the completed item state unless server policy requires it.
