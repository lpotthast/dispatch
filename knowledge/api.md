---
id: dispatch.api
refines:
  - dispatch.architecture
---

# API Design

Dispatch exposes a custom JSON API for domain workflows and a separate CrudKit API for ordinary admin resources. The
standalone CLI uses the custom JSON API through `dispatch-api-client`.

## API Principles

- All workflow operations are project-scoped.
- The server enforces ownership, item state, version safety, and automation rules.
- Custom workflow operations are not CrudKit endpoints.
- Request and response DTOs are shared through `dispatch-types`.
- API clients do not know about database paths or storage internals.
- Project and project-settings views expose the configured `knowledge_directory`, defaulting to `knowledge`. Checked
  relocation and document participation follow [Documents and relationships](knowledge-documents.md).

## Custom JSON Endpoints

Project endpoints:

```text
GET  /api/projects
GET  /api/projects/{project}
GET  /api/projects/{project}/settings
```

`GET /api/projects` is server-scoped and returns all available projects as a name-sorted `Vec<ProjectView>`.

Knowledge transport is project-scoped under `/api/projects/{project}/knowledge/...` and implements the
[agent interface](knowledge-agents.md), [job behavior](knowledge-automation.md),
and [user actions](knowledge-ui.md). [Iterative job endpoints](#iterative-knowledge-jobs) define discovery request and
action payloads; [initialization](knowledge-initialization.md) owns their admission and acceptance rules.

Immediate resources expose `root`, `node`, `documents`, `graph`, `search`, impact mapping, and `check`. They never start
an agent. Graph responses contain bounded summaries and typed relationships without document bodies. Document reads
bind to a registered working copy; edits, moves, and deletion use current-content preconditions and preserve unrelated
files. Document saves submit a path, original content fingerprint, and complete Markdown under the
[editing preconditions](knowledge-documents.md#editing-and-document-lifecycle).
[Knowledge UI](knowledge-ui.md#document-and-graph-workspace) owns conflict and draft presentation.

Job resources support starting and listing jobs, inspecting a job and its result, streamed progress, cancellation,
retry, agent progress/report submission, and proposal review/application. Starting AI work is asynchronous and returns a
job reference. Every job is independent of item IDs and is scoped to an immutable project. Runtime attribution and
permissions are verified by the server; supplying an ID cannot grant access.

Knowledge responses identify current content and working-copy/index context, include local diagnostics, and paginate
bounded collections explicitly. Errors distinguish missing context, missing document/root, ambiguous identity, invalid
metadata, excluded content, stale input/destination, denied operation, unavailable agent, and execution failure. They
preserve an actionable human explanation. A local document error must not turn unrelated reads into a global store
failure. The service reports the actual publication outcome even if a later index refresh or notification fails.

Work item endpoints:

```text
GET   /api/projects/{project}/items
POST  /api/projects/{project}/items
GET   /api/projects/{project}/items/{item_id}
PATCH /api/projects/{project}/items/{item_id}
```

`POST /items` creates the item, its canonical `state=<value>` label, and any
`initial_labels` supplied in the request in one server-side operation. Initial labels use the same key/value shape as
label-create requests; keys and values are trimmed, empty values become value-less labels, duplicate keys are rejected,
and `state` must be supplied through the create request's `state` field rather than duplicated in `initial_labels`. The
backwards-compatible `labels` alias is accepted for `initial_labels`.

Generic label add, update, and delete operations are for non-state labels. The
`state` label is the workflow state hook and must be changed through item create,
`PATCH /items/{item_id}` with `state`, or dedicated workflow transitions so move events, version checks, and workflow
rules stay centralized.

Work item relationship endpoints:

```text
GET    /api/projects/{project}/items/{item_id}/relationships
POST   /api/projects/{project}/items/{item_id}/relationships
PATCH  /api/projects/{project}/relationships/{relationship_id}
DELETE /api/projects/{project}/relationships/{relationship_id}
```

Relationship list responses include the relationship id, kind, direction relative to the requested item (`outgoing` when
the item is the source, `incoming` when the item is the target), source item summary, target item summary, and
timestamps. Create requests use the path item as the source and provide `target_work_item_id` plus a free-form `kind`.
Update requests replace only the trimmed kind. Delete responses include the deleted relationship snapshot.

Workflow endpoints:

```text
POST /api/projects/{project}/items/claim
POST /api/projects/{project}/items/{item_id}/progress
POST /api/projects/{project}/items/{item_id}/finish
POST /api/projects/{project}/items/{item_id}/release
POST /api/projects/{project}/items/{item_id}/request-feedback
```

Comment endpoints:

```text
GET  /api/projects/{project}/items/{item_id}/comments
POST /api/projects/{project}/items/{item_id}/comments
```

Automation endpoints:

```text
GET /api/projects/{project}/automation/runs
GET /api/projects/{project}/automation/runs/{run_id}/log
GET /api/projects/{project}/automation/sessions
GET /api/projects/{project}/automation/triggers
GET /api/projects/{project}/automation/triggers/{id_or_key}
POST /api/projects/{project}/automation/routing/explain
POST /api/projects/{project}/items/search
GET  /api/projects/{project}/work-groups
POST /api/projects/{project}/work-groups
POST /api/projects/{project}/work-groups/{group_key}/items
```

Item search returns `WorkItemPage { items, next_cursor }`, defaults to 50 rows, caps pages at 200, and uses a stable
updated-time/id cursor. Filters cover states, label conditions/selectors, title/description text, finished state,
creating run, producing trigger, relationship kind, and update time. Existing item-list behavior is unchanged.

Agent-context requests send `X-Dispatch-Agent-Id` and `X-Dispatch-Agent-Run-Id`. The server validates that the run
belongs to the addressed project and derives the same agent id, then cross-checks legacy body agent identifiers.
Requests without attribution headers remain operator/user requests. Work-group create and assignment use the same
attribution validation; multi-item assignment is atomic and rejects cross-project items or implicit moves from another
group.

Automation run responses use `AgentRunView`, which includes run mutability (`mutating` or `read_only`), separate
developer-instructions and user-prompt paths, and reported Codex token usage for the run when available. Usage is
reported as input tokens, cached input tokens, output tokens, and a derived total. Run-log responses expose developer
instructions and the user prompt as separate fields, include active in-memory session output while a run is still
ongoing, and fall back to the persisted output log when no active session is present.

Event endpoints:

```text
GET /api/projects/{project}/events
GET /api/projects/{project}/items/{item_id}/events
```

Operator automation endpoints live under `/operator/api/...` and cover rule/personality CRUD, manual scheduling,
revision list/restore/analytics, evaluation history, routing explanation, and bundle
validate/diff/apply/export/list/remove. This prefix defines supported authority separation and does not add
authentication in the local-first release.

Rule create and update endpoints enforce the same policy constraints as bundle imports and CrudKit writes. They reject
incompatible effect-specific fields, invalid produced-item configuration, invalid model/effort overrides, invalid
execution limits, and malformed postconditions before changing the rule or recording a revision. The
[data model](data-model.md) owns these shared policy constraints.

Bundle apply reconciles only objects managed by the same project and bundle key, rejects unmanaged name conflicts,
deletes rules before personalities, and commits the complete diff transactionally against an expected current hash.
Installed-bundle list returns only bundle keys whose latest history entry is `applied`, with managed object counts and
the current hash. Removal requires that hash, deletes only same-bundle managed objects in rule-before-personality order,
records a `removed` history entry, and aborts atomically when an outside rule still references a managed personality.
Validate, diff, and list never mutate.

Portable bundles use strict YAML schema version 1 with stable lowercase bundle/object keys made from letters, digits,
`.`, `_`, and `-`. Selectors are native YAML `Condition` structures and prompts are Markdown. Import renders normalized
CommonMark into stored rich text; export crosses the existing rich-text-to-Markdown boundary and hashes the canonical
typed manifest so formatting-only differences do not cause drift. Unknown versions/fields, invalid
selectors/models/efforts/postconditions, incompatible activation/effect fields, and unknown personality references are
rejected before mutation.

## Workflow Semantics

The [workflow contract](workflows.md) owns transition rules, ownership, release dispositions, and version safety. The API
maps requests to those server operations:

`claim` chooses an eligible item in the requested state and assigns it to the requesting agent. It does not use
`DISPATCH_CLAIMED_ITEM_ID` as an implicit input.

`progress` appends an agent progress comment and records a workflow event. The caller must be the claimant unless server
policy explicitly allows the update.

`finish` appends a completion report, marks the item done, clears active claim ownership, records finish metadata, and
emits events.

`release` appends an optional release comment, clears the claim, restores the claimed-from state, adds
`dispatch:automation-blocked` for the agent-facing endpoint, and emits events. Internal automation releases may use a
claimable disposition for successful unfinished runs, stale-claim recovery, or cancellation; those releases clear
transient workflow blockers so the item can re-enter claim selection.

`request-feedback` appends an agent-authored feedback request comment, clears the claim, restores the claimed-from
state, adds `dispatch:feedback-requested` and `dispatch:automation-blocked`, and emits events. The caller must own the
active claim. Automation must skip items with `dispatch:feedback-requested` until the label is removed after user
feedback has been handled.

`PATCH /items/{item_id}` is for item field updates and supports version safety. It is separate from workflow
transitions.

Relationship mutations enforce the [data-model invariants](data-model.md#work-items), including same-project endpoints,
uniqueness, and version/event updates to both items.

The former project-memory routes are not public API. Any retained legacy memory is considered only during an explicit
import; it does not silently become accepted knowledge. Historical data handling
follows [Knowledge integrity](knowledge-integrity.md).

## CrudKit Endpoints

CrudKit-generated routes are mounted under `/api` for ordinary admin resources:

- projects;
- work items;
- comments;
- agent tools;
- agent runs;
- automation rules;
- personalities;
- work item states;
- label keys;
- swim-lanes.

CrudKit adapters call the same domain services as custom endpoints for Dispatch-owned mutations. Generic reads and
validation persistence remain CrudKit responsibilities; hooks adapt validation without owning workflow writes. Claim,
progress, finish, release, and automation launch use the custom workflow endpoints.

Label-key CRUD is project-scoped configuration. Create accepts a key, optional `accent_color`, and requires
`persistent=true` because a newly configured key may have zero usage. Update can change the accent and persistence flag,
but built-in keys cannot be made non-persistent. Generic delete is rejected: non-built-in keys are forgotten by clearing
persistence after their final usage has been removed. Accent colors are normalized to lowercase `#rrggbb`.

`GET /api/projects/{project}/labels` returns active key/value summaries and also one zero-use, value-less summary for
each known persistent or built-in key with no active labels. Zero-use summaries have `usage_count=0` and no
`last_used_at`, allowing label suggestions to retain configured keys without inventing a label value.

Project delete is an exception to ordinary row-level CRUD implementation: CrudKit and the direct operator handler both
delegate to the authoritative project-deletion lifecycle. A successful response means automation has stopped, active
runs have been cancelled and reaped, Dispatch-owned runtime artifacts have been removed, and the database cascade has
completed. Cleanup or shutdown failure rejects deletion without removing the project row. A concurrent deletion already
operating on the same immutable project id is rejected as in progress.

Run administration updates typed tool metadata and project-scoped item references. An existing launch contract binds its
item attribution, and running legacy records retain their current attribution. Run creation requires prepared launch
inputs through automation; the generic admin create form cannot allocate an executable run. Run deletion closes
registration, persists knowledge-job cancellation when applicable, cancels and drains the process, and cleans owned
workspace and runtime artifacts. Final run deletion and automatic claim-release history commit together. Cleanup or
database failure keeps the run record available for retry.

Automation rule CRUD exposes the explicit run mutability and selected personality for work-consuming rules. Create and
update requests validate storage values `mutating` and `read_only`; new custom rules default to `mutating` unless the
operator chooses read-only. Consume-work create and update requests default a missing personality to the project
`Default` personality and reject missing or cross-project personality references. Rules remain `mutating` until an
operator explicitly selects read-only behavior.

Personality CRUD is project-scoped. Create and update requests trim and require `name`, keep `personality_description`
as free-form text, and enforce unique names within a project. Delete requests reject `Default` and reject any
personality referenced by an automation rule.

## Operator Mutation Endpoints

The server exposes operator mutation handlers for actions such as:

- creating, updating, and deleting projects;
- updating project prompts and settings;
- toggling project auto-commit and updating project commit, revert, and mutable Git command policy;
- updating the independent read-only automation concurrency limit;
- creating, updating, moving, deleting, and commenting on work items;
- creating, updating, and deleting work item relationships from item detail pages;
- starting, stopping, and recovering automation;
- canceling an individual active automation run;
- cleaning up worktrees;
- opening workspace folders or fixed editor targets such as RustRover and VS Code;
- creating, updating, deleting, and queueing evaluations for automation rules;
- creating, updating, and deleting project personalities;
- discovering agent tools;
- picking folders on the local system.

These endpoints are UI integration points, not the stable agent-facing API.
[UI interaction](ui.md#live-updates) uses typed frontend services backed by server functions, which invoke the same
authoritative backend services as the direct handlers.

Direct automation starts may include an explicit mutability value. Omitted mutability defaults to `mutating`;
work-producing evaluations ignore run mutability because they do not launch agents. Automation status responses include
aggregate running runs plus separate mutating/read-only running counts and the effective mutating allowance.

Project system prompt writes create `SystemPromptChanged` events containing the full post-write prompt snapshot.
Clearing system prompt history deletes only old prompt events; the current project system prompt remains on the project
record.

## Errors

API errors should be explicit enough for the CLI to show actionable output. Important error classes include:

- missing project context;
- unknown project or item;
- unknown relationship;
- cross-project relationship target;
- relationship source and target are the same item;
- empty relationship kind;
- duplicate relationship;
- invalid state transition;
- item already claimed;
- caller does not own the claim;
- stale expected version;
- automation tool unavailable;
- read-only launch unsupported by the selected agent tool;
- automation concurrency limit reached for the requested mutability;
- missing or cross-project automation personality;
- personality delete rejected because it is `Default` or still referenced by automation;
- run log unavailable.

The server should prefer structured error responses over plain text so CLI and future clients can distinguish user
errors from server failures.

<a id="knowledge-relocation"></a>

## Knowledge Store Relocation

Knowledge-directory relocation is an operator operation enforcing the
[document location contract](knowledge-documents.md#location-and-discovery).
[Knowledge UI](knowledge-ui.md#settings-and-degraded-operation) owns its preview and controls.

## Iterative knowledge jobs

Project-scoped `/api/projects/{project}/knowledge/jobs` lists jobs and accepts a typed start request with a stable
`request_id`, optional predecessor, additional context, active-time budget, reported-token budget, and application mode.
`/{id}` returns durable details; `/{id}/action` accepts cancel, retry, continue, apply, or reject. Retry/continue retain a
predecessor link and require a new idempotency key. `/{id}/coverage` accepts an optional aspect filter.

Active knowledge agents use `/{id}/source/{list|search|read}`, `/{id}/progress`, `/{id}/report`, and `/{id}/assess` with
matching project/run attribution. User controls and extraction history are unavailable to launched readers. Missing,
terminal, cross-project, or mismatched context fails. Source reads respect current exclusions and retained evidence;
assessments cannot account for unsupplied ranges. Markdown remains the authored authority.

`/api/projects/{project}/knowledge/job-settings` reads or updates the project's application mode. Job requests may
explicitly override it; otherwise admission captures the project default. Defaults and recurring admission follow
[knowledge settings policy](knowledge-automation.md#settings-scheduling-and-triggers).
