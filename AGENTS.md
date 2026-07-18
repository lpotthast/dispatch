# Repository Guidelines

## Design Source of Truth

The `design/` directory is Dispatch's spec-like source of truth for product behavior, functional requirements, and technical architecture. Implementation code contains the fine details, but it should implement the design described there rather than becoming the only place where decisions live.

Start with `design/dispatch.md` for the system overview, core invariants, and document map. Then consult the focused design docs before changing related behavior:

- `design/architecture.md`: process boundaries, crate responsibilities, storage ownership, and route classes.
- `design/data-model.md`: projects, work items, comments, events, agent tools, agent runs, automation, and settings.
- `design/api.md`: custom JSON endpoints, UI form endpoints, CrudKit boundaries, workflow semantics, and errors.
- `design/cli.md`: agent-facing CLI contract, context resolution, commands, and development or published resolution.
- `design/workflows.md`: claim, progress, finish, release, updates, automation launch, stale claims, run logs, and pull requests.
- `design/ui.md`: Leptos routes, workflow surface, admin surfaces, project settings, live updates, and browser coverage.

Any change that affects user-visible behavior, workflow rules, API or CLI contracts, storage shape, automation, project settings, or major UI structure must either follow the existing design docs or update `design/` in the same change so the design remains authoritative.

## Layout

Dispatch uses standalone root-level Rust 2024 crates and intentionally has no root `Cargo.toml` or Cargo workspace.

- `dispatch-server/`: Axum server, Leptos UI, domain services, SQLite persistence, automation supervisor, styles, assets, and browser tests.
- `dispatch-types/`: shared request/response DTOs and enum types.
- `dispatch-api-client/`: typed HTTP client for Dispatch JSON endpoints.
- `dispatch-cli/`: standalone agent-facing `dispatch` CLI binary; it relays to a running server and must not open SQLite.
- `crudkit/`: Git submodule used as a local dependency; do not put Dispatch workflow rules there.
- `design/`: authoritative product, workflow, UI, API, CLI, data-model, and architecture specifications for Dispatch.

Keep Dispatch-specific claim, progress, finish, release, automation, and board behavior in Dispatch-owned server services and custom API endpoints, not CrudKit routes.

## Commands

Run commands from the repository root through `just`, which passes explicit `--manifest-path` values because there is no root Cargo workspace.

- `just fmt`: format all Dispatch crates.
- `just check`: check server, CLI, API client, and types crates.
- `just test`: run standard Rust tests for Dispatch crates.
- `just clippy`: run clippy with `--all-targets -- -D warnings` for Dispatch crates.
- `just verify`: run formatting, tests, and clippy.
- `just serve`: run the server with `.dispatch/dispatch.sqlite3` on `127.0.0.1:4000`.
- `just cli <args>`: run the API-relay CLI.
- `just browser-test`: run the ignored browser integration test; use `just browser-test-visible` for UI debugging.

Server-local overrides: `DISPATCH_DATABASE`, `DISPATCH_BIND`, `DISPATCH_PROJECT`, and `DISPATCH_WORKSPACE_IDE`.
`just serve` sets `DISPATCH_DEVELOPMENT=1`, allowing automation to build the CLI from this source checkout. Published runs require the standalone `dispatch` CLI on `PATH`.

## Agent-Facing Contract

The Dispatch server is the only process that owns or writes the database. Agents interact with Dispatch through the `dispatch` CLI, and the CLI is an API relay to `DISPATCH_API_URL`.

Dispatch-launched agents receive `DISPATCH_API_URL`, `DISPATCH_PROJECT`, `DISPATCH_AGENT_ID`, and `DISPATCH_CLAIMED_ITEM_ID`. For the claimed item, prompts should use short commands such as `dispatch item show --json`, `dispatch label list --json`, `dispatch label add --key ...`, `dispatch comment list --json`, `dispatch item progress --body ...`, `dispatch item finish --report ...`, and `dispatch item release --comment ...`. Agents may edit item labels themselves when that clarifies routing, status, priority, environment, or follow-up needs.

Project memory is Dispatch-owned storage, not Codex internal memory. Agents should persist important run discoveries with `dispatch memory append --body ...` and use `dispatch memory set --body ...` only for intentional full rewrites; memory writes must go through the Dispatch CLI/API so they create attributed `MemoryChanged` events.

CLI context resolution must prefer explicit flags, then environment variables. Missing required project, agent, or claimed-item context must fail instead of creating implicit data.

## Style

Use Rustfmt defaults. Keep Rust names idiomatic: `snake_case` for modules, functions, and fields; `PascalCase` for types and Leptos components; `SCREAMING_SNAKE_CASE` for constants. Organize modules by domain behavior rather than generic buckets.

Do not edit generated `style/crudkit` or `style/leptonic` content unless regenerating from the upstream source intentionally. Put Dispatch-owned styling under `dispatch-server/style/app/`.

## Testing

Place focused unit tests near the code they exercise. Browser coverage lives in `dispatch-server/tests/browser_test.rs` and is ignored by default because it starts Dispatch and Chrome.

Use `assertr` for every test assertion instead of the standard `assert!`, `assert_eq!`, and `assert_ne!` macros. Decide whether to use `assertr` in production code case by case.

In browser tests, use the Thirtyfour API to query and interact with the current page. Do not use `WebDriver::execute` or `execute_async` with JavaScript strings when the same operation can be expressed through element queries, properties, CSS values, actions, waits, CDP, Rust HTTP requests, or server/database fixtures.

When changing workflow paths, cover project scoping, claim ownership, progress, release, finish, stale-claim recovery, and version-safety behavior. When changing CLI/API behavior, cover context resolution and server-backed endpoint behavior.

## Git And PRs

Do not infer a project-specific commit format from history. Use short imperative subjects, for example `Add API relay CLI`.

PRs should include a concise behavior summary, verification commands run, linked work item or issue when available, and screenshots or notes for UI changes. Call out schema, CLI, API, automation prompt, or agent-instruction changes explicitly.

## General

Make sure to always use consistent wording when speaking about one concept. In code. In comments. In reasoning. Everything else is confusing.
