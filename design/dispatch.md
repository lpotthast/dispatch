# Dispatch Design Overview

Dispatch coordinates software work across a local project, a server-owned work item database, a web UI, and launched coding agents. The server is the source of truth for persistence and workflow rules. Agents use the `dispatch` CLI as an HTTP relay to the server; they do not open SQLite or write Dispatch state directly.

## Core Invariants

- The Dispatch server is the only process that owns or writes the database.
- Agent-facing commands go through the standalone `dispatch` CLI.
- The agent-facing CLI calls the Dispatch JSON API and never opens SQLite.
- Dispatch-launched agents receive a prepared environment and should normally omit repeated project, agent, and claimed item arguments.
- Server-side workflow rules are authoritative for project scope, ownership claims, item state, and version safety.

## Document Map

- [architecture.md](architecture.md): process boundaries, crate layout, storage ownership, and CrudKit usage.
- [data-model.md](data-model.md): projects, work items, comments, runs, automation rules, events, and settings.
- [api.md](api.md): custom JSON endpoints, UI form endpoints, streaming endpoints, and CrudKit boundaries.
- [cli.md](cli.md): standalone CLI contract, context resolution, commands, and development shim.
- [workflows.md](workflows.md): claim, progress, finish, release, automation launch, automation rules, stale claims, and run logs.
- [ui.md](ui.md): Leptos routes, admin surfaces, live workflow visibility, and browser coverage.
- [branding.md](branding.md): application icon concept, source prompt, assets, and iteration guidance.

## Repository Shape

Dispatch uses root-level Rust crates and intentionally has no root `Cargo.toml` workspace:

```text
dispatch-server/       server, SSR app, storage, automation, operator CLI
dispatch-types/        shared request and response DTOs
dispatch-api-client/   typed HTTP client
dispatch-cli/          standalone agent-facing CLI binary named dispatch
crudkit/               local CrudKit submodule dependency
dev-bin/dispatch       tracked development shim for the agent-facing CLI
```

The absence of a root workspace keeps the Dispatch crates independent from the `crudkit/` submodule workspace and avoids workspace dependency inheritance across repository boundaries. Repository-level `just` recipes call each crate with explicit `--manifest-path` values.

## Actors

- Human operators use the web UI and trusted server/operator commands.
- Dispatch automation launches coding agents with a prepared environment.
- Agents use only the agent-facing `dispatch` CLI for Dispatch work state.
- The server enforces all workflow transitions and owns the SQLite database.

## Design Boundary

CrudKit accelerates ordinary admin and CRUD surfaces such as projects, work items, comments, agent tools, agent runs, and automation rules. Dispatch-specific workflow behavior remains custom: claim, progress, finish, release, request feedback, stale-claim recovery, automation launch, run logs, live events, and board-oriented workflow views.
