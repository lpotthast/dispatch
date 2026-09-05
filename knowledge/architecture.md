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
  logs, cancellation, and resource accounting. [Knowledge System](knowledge.md) defines the replacement contract;
  current legacy components are subject to [the transition plan](knowledge-transition.md).
- Launched agents never receive a database path and never use a database-opening CLI.

## Crate Responsibilities

### `dispatch-server`

The server crate contains:

- the Axum and Leptos application;
- SeaORM entities and migrations;
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
define or invoke server functions or browser request clients directly.

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

The current `dispatch-knowledge-core` crate contains the legacy signed-store implementation. The replacement may reuse a
small deterministic library, but retaining its existing signing, persistence, record codecs, or crate decomposition is
not a requirement.

Cross-route browser caches are focused services provided through Leptos context and contain typed backend DTOs, not
rendered views, complete page response objects, or serialized payloads. Persistence through browser local storage is a
service boundary: values are decoded immediately into the typed reactive cache before consumers access them.

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

## Storage

Dispatch currently persists operational state in SQLite through server-owned repository and service boundaries that
remain database-engine-neutral. The default database path is under the user's Dispatch data directory, while repository
development recipes pass `.dispatch/dispatch.sqlite3` explicitly. A later PostgreSQL cutover must migrate and verify
durable state, including identities, counts, history, relationships, and provenance; it never reconstructs or discards
that state from the workspace.

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

Codex runtime state is Dispatch-owned local state under the user's Dispatch data directory. The shared managed Codex
home stores login/status state. Each project gets a project Codex home under that shared tree for generated
`config.toml`, `rules/*.rules`, sessions, logs, and SQLite state. Project homes may symlink shared auth and skill assets
so projects can have independent runtime policy without requiring a new login for every project.

Project lifecycle coordination is keyed by immutable database project id. The server's deletion service is the single
authority shared by direct operator and CrudKit paths; it coordinates the automation controller, process-session
registry, filesystem artifacts, Git workspaces and refs, managed Codex state, and finally the operational-database
cascade. Name remains a reusable routing key, but it is not used to associate live process or cleanup state across
project lifetimes. Deletion admission is single-flight per project id: a concurrent duplicate is rejected while
different project ids remain independent. Session entries and deletion admission share one synchronized state boundary.
After deletion closes admission, a late session start is rejected as cancelled without entering the registry, so an
observed empty session set remains stable. Automation-controller activation is registered synchronously through this
same boundary: deletion either observes and stops the registered controller or closes admission first and rejects the
activation. Controller entries, cancellation receivers, and scheduler snapshots are keyed by immutable project id, so a
snapshot from an old project lifetime cannot activate a same-name replacement. The owning deletion holds an admission
permit. Failure or task cancellation drops the permit and reopens admission for retry; successful row deletion
synchronously converts it to a process-lifetime tombstone, permanently rejecting delayed starts for that old id. A
duplicate caller never owns a permit and cannot release the owner's state. CrudKit reaches the service through the
project resource's dedicated repository only after its before-delete validation succeeds; project lifecycle hooks do not
perform destructive cleanup. Detached automation execution likewise owns session completion through an unwind-safe
guard.

Dispatch minimizes control-plane traffic to OpenAI. It performs one Codex readiness probe when the server starts and one
immediately before each actual automation run so authentication or an active rate-limit block fails before work is
claimed. It does not poll Codex status globally while idle, and enabling project automation does not add a probe before
the per-run check. Readiness probes read account and rate-limit state only. While an operator has `/system` mounted,
that page loads a detailed status immediately and refreshes it every five minutes, including the token-activity summary;
duplicate page or live-event requests within four minutes share the most recent detailed result. The manual Refresh
action always forces a new detailed check. Managed Codex config disables automatic update checks and optional remote app
or plugin catalogs that Dispatch automation does not use.

Every spawned Codex app-server has an owned process lifetime. Dispatch starts the configured executable on a loopback
WebSocket endpoint, uses the unmodified published SDK as the protocol client, and independently terminates and reaps the
process tree. A status probe exits after its responses are collected, and an automation app-server exits after its run
or recovery attempt ends. Cleanup does not depend on SDK client-drop behavior, so completed probes and runs cannot
retain background processes that continue refreshing remote catalogs.

Knowledge UI reads use current visible files and working-copy-scoped derived data. Expensive traversal, parsing, and
indexing run on blocking workers rather than occupying async request workers. Initial navigation does not load all
document bodies or historical job artifacts. Local document errors do not invalidate unrelated readable content.

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

`just serve` explicitly sets `DISPATCH_DEVELOPMENT=1`. In that mode, automation builds the sibling `dispatch-cli` source
crate before each agent launch and gives the resulting executable to the sandboxed agent. A published server does not
assume source files exist: it requires an executable published `dispatch` CLI on `PATH` and rejects the automation
launch before Codex starts or work is claimed when the CLI is unavailable.

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
