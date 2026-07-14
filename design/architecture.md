# Architecture

Dispatch is a local-first Rust application with a server-rendered and hydrated Leptos UI. The server owns persistence, workflow state, automation launch, and the HTTP API. The standalone CLI is an API client for agents and tooling.

## Process Boundaries

- `dispatch-server` is the only process that opens the SQLite database.
- `dispatch-cli` resolves context, validates command shape, and calls `dispatch-api-client`.
- `dispatch-operator` is the operator-facing HTTP client for automation administration and bundle reconciliation.
- `dispatch-api-client` contains typed HTTP calls and error handling.
- `dispatch-types` contains shared DTOs, enum types, and request payloads.
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

The legacy server CLI may accept `--database` because it is part of the trusted server surface. The standalone `dispatch-operator` binary never opens SQLite; it uses `/operator/api/...` endpoints and is not placed on launched-agent `PATH`.

The hydrated frontend is organized by route under `dispatch-server/src/frontend/pages/`, with one module per operator page. The root application module only mounts the application shell and root providers. Shared UI behavior lives in focused modules under `frontend/components/`.

Backend interaction is owned by focused service objects under `frontend/services/`. Production services wrap server functions and other transport details, are provided once from the root layout through Leptos context, and expose typed domain-oriented methods to pages and shared components. Their request callbacks are replaceable so consumers can be tested with in-process mocks. Route modules may own their page response types, resources, and rendering, but they do not define or invoke server functions or browser request clients directly.

Cross-route browser caches are focused services provided through Leptos context and contain typed backend DTOs, not rendered views, complete page response objects, or serialized payloads. Persistence through browser local storage is a service boundary: values are decoded immediately into the typed reactive cache before consumers access them.

### `dispatch-types`

This crate defines shared transport types for the API client and server. Examples include project views, work item views, comments, agent runs, automation rules, workflow request payloads, and shared enum values.

Types in this crate describe the wire contract. Server-only persistence details stay in `dispatch-server`.

### `dispatch-api-client`

This crate provides typed HTTP methods for the custom JSON API. It is used by `dispatch-cli` and can be reused by future tooling. It does not know about SQLite, SeaORM, Leptos, or server internals.

### `dispatch-cli`

This crate builds the `dispatch` binary used by agents. It is intentionally small: parse command arguments, resolve context from flags and environment variables, call the typed API client, and print human or JSON output.

### `dispatch-operator`

This crate builds the operator-only automation administration client. It consumes YAML files for rule and personality writes and manages bundles, revisions, scheduling, routing diagnostics, and analytics through HTTP.

## Storage

Dispatch persists data in SQLite through the server crate. The default database path is under the user's Dispatch data directory, while repository development recipes pass `.dispatch/dispatch.sqlite3` explicitly.

Database writes must flow through server services. This keeps workflow checks in one process and prevents launched agents from bypassing ownership, state, project, or version rules.

SeaORM and CrudKit persistence records mirror SQLite and may represent enums or structured configuration as text. These records are storage types, not workflow-domain types. Server services decode and validate them at the persistence boundary before applying policy, starting automation, rendering UI data, or returning API views. Invalid persisted values produce contextual service errors rather than panics or implicit fallback behavior.

Codex runtime state is Dispatch-owned local state under the user's Dispatch data directory. The shared managed Codex home stores login/status state. Each project gets a project Codex home under that shared tree for generated `config.toml`, `rules/*.rules`, sessions, logs, and SQLite state. Project homes may symlink shared auth and skill assets so projects can have independent runtime policy without requiring a new login for every project.

Dispatch minimizes control-plane traffic to OpenAI. It performs one Codex readiness probe when the server starts and one immediately before each actual automation run so authentication or an active rate-limit block fails before work is claimed. It does not poll Codex status globally while idle, and enabling project automation does not add a probe before the per-run check. Readiness probes read account and rate-limit state only. While an operator has `/codex` mounted, that page loads a detailed status immediately and refreshes it every five minutes, including the token-activity summary; duplicate page or live-event requests within four minutes share the most recent detailed result. The manual Refresh action always forces a new detailed check. Managed Codex config disables automatic update checks and optional remote app or plugin catalogs that Dispatch automation does not use.

Every spawned Codex app-server has an owned process lifetime. Dispatch starts the configured executable on a loopback WebSocket endpoint, uses the unmodified published SDK as the protocol client, and independently terminates and reaps the process tree. A status probe exits after its responses are collected, and an automation app-server exits after its run or recovery attempt ends. Cleanup does not depend on SDK client-drop behavior, so completed probes and runs cannot retain background processes that continue refreshing remote catalogs.

## Server Routes

The server exposes four classes of routes:

- Leptos UI routes for operators.
- Custom Dispatch JSON API routes under `/api/projects/...`.
- Operator automation JSON routes under `/operator/api/...`.
- CrudKit-generated API routes under `/api` for ordinary admin resources.

The operator prefix is an intentional supported-interface boundary, not an authentication boundary in this local-first release. Custom Dispatch workflow endpoints are not CrudKit endpoints. CrudKit remains an admin accelerator, but its automation and personality hooks use the same revision service as operator writes.

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

`just serve` runs `cargo leptos serve`, which builds once and starts the server with the repository-local database and default bind address. It is not a watcher and does not restart the backend on source changes. The running server still serves frontend artifacts from the server crate's shared `target/site` output directory. Because Dispatch disables hashed frontend filenames, later `cargo leptos` builds, browser-test runs, or other UI verification commands can replace `/pkg/dispatch.js`, `/pkg/dispatch_bg.wasm`, and `/pkg/dispatch.css`; browser refreshes or navigations may then show newer frontend code while the already-running backend process remains unchanged.

`just serve` explicitly sets `DISPATCH_DEVELOPMENT=1`. In that mode, automation builds the sibling `dispatch-cli` source crate before each agent launch and gives the resulting executable to the sandboxed agent. A published server does not assume source files exist: it requires an executable published `dispatch` CLI on `PATH` and rejects the automation launch before Codex starts or work is claimed when the CLI is unavailable.

Server tracing writes pretty logs to stderr. The default target filter is `info,tokio=warn,runtime=warn,sqlx=warn`, which hides SQLx query noise while keeping warnings visible. Set `DISPATCH_SQLX_LOG=info` to opt SQLx query logs back in, or set `DISPATCH_LOG` to a full `tracing_subscriber::filter::Targets` directive list such as `debug,sqlx=warn`.
