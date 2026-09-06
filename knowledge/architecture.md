---
id: dispatch.architecture
refines:
  - dispatch.index
---

# Architecture

Dispatch is a local-first Rust application with a server-rendered and hydrated Leptos UI. The server owns persistence,
workflow state, automation launch, and the HTTP API. The standalone CLI is an API client for agents and tooling.

## Process Boundaries

- `dispatch-server` is the only process that opens the operational database, currently SQLite.
- `dispatch-cli` resolves context, validates command shape, and calls `dispatch-api-client`.
- `dispatch-operator` is the operator-facing HTTP client for automation administration and bundle reconciliation.
- `dispatch-api-client` contains typed HTTP calls and error handling.
- `dispatch-types` contains shared DTOs, enum types, and request payloads.
- Knowledge parsing, ignore handling, graph derivation, lexical retrieval, and structural checks are deterministic
  mechanics. They remain independent of model execution and transport.
- `dispatch-server` owns knowledge jobs separately from work items and item automation, reusing shared agent execution,
  logs, cancellation, and resource accounting. [Knowledge System](knowledge.md) owns the knowledge contract.
- Launched agents never receive a database path and never use a database-opening CLI.

## Crate Responsibilities

### `dispatch-server`

The server crate contains:

- the Axum and Leptos application;
- SeaORM entities and schema migrations;
- storage initialization and database path handling;
- project, item, work-group, comment, automation, and event services;
- custom JSON API endpoints;
- CrudKit-backed admin endpoints;
- automation process launch and log capture;
- Dispatch-managed Codex homes, per-project Codex config/rules, and run-specific tool shims;
- the trusted server/operator CLI.

The legacy server CLI may accept `--database` because it is part of the trusted server surface. The standalone
`dispatch-operator` binary never opens SQLite; it uses `/operator/api/...` endpoints and is not placed on launched-agent
`PATH`.

The hydrated frontend is organized by route under `dispatch-server/src/frontend/pages/`, with one module per operator
page. The root application module only mounts the application shell and root providers. Shared UI behavior lives in
focused modules under `frontend/components/`.

Backend interaction is owned by focused service objects under `frontend/services/`. Production services wrap server
functions and other transport details, are provided once from the root layout through Leptos context, and expose typed
domain-oriented methods to pages and shared components. Their request callbacks are replaceable so consumers can be
tested with in-process mocks. Route modules may own their page response types, resources, and rendering, but they do not
define or invoke server functions or browser request clients directly. [UI Design](ui.md) owns layout, controls,
navigation, and refresh behavior, with [Knowledge UI](knowledge-ui.md) owning the knowledge workspace.

Cross-route browser caches are focused services provided through Leptos context and contain typed backend DTOs, not
rendered views, complete page response objects, or serialized payloads. Persistence through browser local storage is a
service boundary: values are decoded immediately into the typed reactive cache before consumers access them.

[Backend Metrics](metrics.md) owns in-process performance instrumentation and aggregation, including repository and SQL
timings and the board-loading query boundary.

### `dispatch-types`

This crate defines shared transport types for the API client and server. Examples include project views, work item
views, comments, agent runs, automation rules, workflow request payloads, and shared enum values.

Types in this crate describe the wire contract. Server-only persistence details stay in `dispatch-server`.

### `dispatch-api-client`

This crate provides typed HTTP methods for the custom JSON API. It is used by `dispatch-cli` and can be reused by future
tooling. It does not know about SQLite, SeaORM, Leptos, or server internals.

### `dispatch-cli`

This crate builds the `dispatch` binary used by agents. It is intentionally small: parse command arguments, resolve
context from flags and environment variables, call the typed API client, and print human or JSON output.

### `dispatch-operator`

This crate builds the operator-only automation administration client. It consumes YAML files for rule and personality
writes and manages bundles, revisions, scheduling, routing diagnostics, and analytics through HTTP.

## Knowledge Storage Boundary

Canonical knowledge is ordinary UTF-8 Markdown and small YAML frontmatter in a configured project directory, normally
`knowledge/`. Humans and authorized coding agents edit it with normal file tools. The server derives navigation, search,
and diagnostics from the current visible files and binds reads to the caller's registered working
copy. [Documents and relationships](knowledge-documents.md) owns metadata, ignore rules, editing, and local failure
behavior.

The operational database owns durable knowledge jobs, run references, settings, findings, proposals, and application
outcomes. Retained artifacts hold input/output evidence, drafts, diffs, and logs. Derived indexes are rebuildable; job
history is not reconstructed from Markdown. Project deletion preserves workspace knowledge.

Background knowledge jobs analyze filtered prepared inputs and edit a draft. The service coordinates writers, checks
input/destination freshness and scope, and applies or presents the resulting diff according to project
settings. [Knowledge automation](knowledge-automation.md) owns scheduling, publication, and recovery. Git is optional
and background knowledge jobs do not stage, commit, reset, or push.

Deterministic knowledge operations live in `dispatch-server/src/backend/knowledge/`: `discovery` owns filesystem
participation, `documents` parses Markdown/frontmatter and derives relationships, and the parent module binds queries to
registered working copies. There is one production consumer, the server, so a separate `dispatch-knowledge-core` crate
adds no useful boundary. These modules have no model execution dependency. Extract a library only
when a real second consumer needs the same behavior; do not introduce a family of speculative knowledge crates.

`dispatch-types` owns the transport DTOs, and `dispatch-api-client` and `dispatch-cli` relay them. Knowledge jobs
belong to a separate server service using the shared agent runtime. The `knowledge/jobs` modules own durable admission,
source snapshots and aspect assessments, reading evaluation, sequential execution, and journaled publication. Shared
execution allocates no-item knowledge passes with normal run logs and purpose/job links. Deterministic file operations have no dependency on
agent execution.

## Storage

Dispatch persists operational state in SQLite through server-owned repository and service boundaries that
remain database-engine-neutral. The default database path is under the user's Dispatch data directory, while repository
development recipes pass `.dispatch/dispatch.sqlite3` explicitly. Storage-engine changes preserve durable state, including identities, counts, history,
relationships, and provenance; they never reconstruct or discard that state from the workspace.

Database writes must flow through server services. This keeps workflow checks in one process and prevents launched
agents from bypassing ownership, state, project, or version rules.

A server service that holds a database transaction must pass that exact connection through every project and path-scope
lookup instead of reacquiring the pool. Knowledge publication validates the project, working-copy scope, exclusions, and
current destination content before writing. File analysis and draft preparation stay outside short SQL transactions;
operational history and derived indexes follow the publication outcome described in the knowledge contract. The
supported SQLite configuration includes a single pooled connection; increasing the pool is not a correctness mechanism.

SeaORM and CrudKit persistence records mirror the current operational database and may represent enums or structured
configuration as text. These records are storage types, not workflow-domain types. Server services decode and validate
them at the persistence boundary before applying policy, starting automation, rendering UI data, or returning API views.
Invalid persisted values produce contextual service errors rather than panics or implicit fallback behavior.

Automation rule policy is owned by `backend/automation_triggers/policy.rs`. Operator requests and bundle imports share
its typed validation, while CrudKit writes and persisted-rule reads share its storage decoder and the same validation.
Postcondition structure is validated by the postcondition domain. Bundle handling owns portable manifest structure and
references; ordinary automation validation has no dependency on bundle validation or database lookups. Project-scoped
personality resolution remains a service operation.

Codex runtime state is Dispatch-owned local state under the user's Dispatch data directory. The shared managed Codex
home stores login/status state. Each project gets a project Codex home under that shared tree for generated
`config.toml`, `rules/*.rules`, sessions, logs, and SQLite state. Prepared knowledge jobs retain isolated Codex homes with their job artifacts and reuse shared authentication.
Project homes may symlink shared auth and skill assets
so projects can have independent runtime policy without requiring a new login for every project.

The server's deletion service is the single authority shared by direct operator and CrudKit paths. It coordinates
admission, processes, runtime artifacts, and the database cascade by immutable project ID under the
[project-deletion workflow](workflows.md#project-deletion). CrudKit reaches the service through the project resource's
dedicated repository after before-delete validation; lifecycle hooks do not perform destructive cleanup. Detached
automation execution owns session completion through an unwind-safe guard.

Dispatch minimizes control-plane traffic to OpenAI. It performs one Codex readiness probe when the server starts and one
immediately before each actual automation run so authentication or an active rate-limit block fails before work is
claimed. It does not poll Codex status globally while idle, and enabling project automation does not add a probe before
the per-run check. Readiness probes read account and rate-limit state only. Detailed status requests, caching, and manual
refresh follow the [System-page contract](ui.md#admin-surfaces). Managed Codex config disables automatic update checks
and optional remote app or plugin catalogs that Dispatch automation does not use.

Every spawned Codex app-server has an owned process lifetime. Dispatch starts the configured executable on a loopback
WebSocket endpoint, uses the unmodified published SDK as the protocol client, and independently terminates and reaps the
process tree. A status probe exits after its responses are collected, and an automation app-server exits after its run
or recovery attempt ends. Cleanup does not depend on SDK client-drop behavior, so completed probes and runs cannot
retain background processes that continue refreshing remote catalogs.

Knowledge traversal, parsing, and indexing run on blocking workers rather than occupying async request workers.
[Knowledge UI](knowledge-ui.md) owns progressive loading and local-error presentation over working-copy-scoped data.

## Server Routes

The server exposes four classes of routes:

- Leptos UI routes for operators.
- Custom Dispatch JSON API routes under `/api/projects/...`.
- Operator automation JSON routes under `/operator/api/...`.
- CrudKit-generated API routes under `/api` for ordinary admin resources.

The operator prefix is an intentional supported-interface boundary, not an authentication boundary in this local-first
release. Custom Dispatch workflow endpoints are not CrudKit endpoints. CrudKit remains an admin accelerator, but its
automation and personality hooks use the same revision service as operator writes.

## Development Commands

The repository-level `Justfile` uses explicit crate manifests because there is no root workspace. Common commands are:

```text
just fmt
just check
just test
just clippy
just verify
just serve
just cli item list --json
just operator automation rule list --project demo
just browser-test
```

`just serve` runs `cargo leptos serve`, which builds once and starts the server with the repository-local database and
default bind address. It is not a watcher and does not restart the backend on source changes. The running server still
serves frontend artifacts from the server crate's shared `target/site` output directory. Because Dispatch disables
hashed frontend filenames, later `cargo leptos` builds, browser-test runs, or other UI verification commands can replace
`/pkg/dispatch.js`, `/pkg/dispatch_bg.wasm`, and `/pkg/dispatch.css`; browser refreshes or navigations may then show
newer frontend code while the already-running backend process remains unchanged.

`just serve` explicitly sets `DISPATCH_DEVELOPMENT=1` to use the source-built agent CLI.
[CLI availability](cli.md#cli-availability-and-development-builds) owns development and published resolution, validation,
and failure behavior.

Server tracing writes pretty logs to stderr. The default target filter is `info,tokio=warn,runtime=warn,sqlx=warn`,
which hides SQLx query noise while keeping warnings visible. Set `DISPATCH_SQLX_LOG=info` to opt SQLx query logs back
in, or set `DISPATCH_LOG` to a full `tracing_subscriber::filter::Targets` directive list such as `debug,sqlx=warn`.

## Managed Codex Log Storage

Every actual Codex readiness probe, including startup, pre-run, detailed System-page, and operator-forced checks,
inspects managed Codex log storage without changing readiness. The bounded scan covers only the shared managed-home root
and immediate numeric `projects/<project_id>` homes. It groups each `logs_*.sqlite` database with its `-wal` and `-shm`
sidecars, reports a family only when its logical size is strictly greater than 1 GiB, ignores symlinks and unrelated
runtime state, and preserves findings even when app-server initialization fails. Scan failures remain warning-only but
disable cleanup.

Readiness probes and log cleanup serialize through one managed-home operation lock. Cleanup first closes Codex session
admission only when no Codex session is active, re-scans and validates every target, then removes sidecars before the
database file. Missing files and an already-clean home are successful idempotent outcomes. The admission guard stays
held through unlinking so no new run can race cleanup.
