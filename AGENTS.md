# Repository Guidelines

## Design Source of Truth

The `knowledge/` directory is Dispatch's source of truth for accepted product behavior, requirements, and architecture.
Implementation supplies the executable detail and must satisfy the documented design.

Start repository work by reading `knowledge/README.md`, then follow the summaries and links into every relevant or
uncertain branch before inspecting implementation. Read progressively rather than loading the whole knowledge base.
Reuse already-read material during the same workstream; refresh affected documents when they change and extend the route
when source evidence expands the task. Search directly for terms or paths when document summaries do not route them
clearly.

Knowledge documents describe the accepted system in present tense: its behavior, contracts, constraints, architecture,
and consequential rationale. They are normative even when implementation has not caught up. Never turn `knowledge/`
into a changelog, progress report, implementation plan, TODO list, or work-item tracker. This rule applies to every
knowledge document, including overview, acceptance, migration, and compatibility documents; there is no exception for a
"transition" document.

For example, write "Knowledge jobs run independently of work items", not "The cleanup removes work-item coupling",
"Jobs remain to be implemented", or "Step 3 adds jobs". Describe migration behavior as enduring preservation and
compatibility guarantees, not as instructions for the current coding task. Keep task status, implementation gaps,
verification results, rollout steps, and links to planned work in task reports, PR descriptions, or planning files
outside `knowledge/`. An incomplete implementation does not weaken an accepted requirement or make it optional.

Read and edit Markdown directly. File content and frontmatter own knowledge; derived indexes and legacy metadata under
`knowledge/.dispatch/` do not override it. Do not invoke signing, reconciliation, or receipt commands to publish
ordinary document edits, and do not maintain legacy metadata alongside the Markdown.

Focused design documents:

- `knowledge/architecture.md`: process, crate, storage, and route boundaries.
- `knowledge/data-model.md`: projects, work items, comments, events, runs, automation, and settings.
- `knowledge/api.md`: custom endpoints, CrudKit boundaries, workflow semantics, and errors.
- `knowledge/cli.md`: CLI context, commands, and development or published resolution.
- `knowledge/workflows.md`: claims, completion, release, automation, recovery, and run logs.
- `knowledge/ui.md`: routes, workflow and administration surfaces, live updates, and browser coverage.
- `knowledge/knowledge.md`: entry point for documents, authoring, agents, automation, UI, and integrity guarantees.

Changes to user-visible behavior, workflow rules, API/CLI contracts, storage, automation, settings, or major UI
structure must follow the owning design or update it in the same change. Update the lowest affected explanation first,
then check broader summaries and relevant dependents. Preserve correct wording and report unresolved contradictions; do
not silently rewrite an accepted requirement to match a bug. Before completion, inspect the documentation impact and
report the changed contracts and relevant verification. Before finishing, review edited knowledge for status, TODO,
transition-plan, or task-history language and remove it. Report implementation gaps outside knowledge, alongside the
verification result.

## Layout

Dispatch uses standalone root-level Rust 2024 crates and intentionally has no root `Cargo.toml` or Cargo workspace.

- `dispatch-server/`: Axum server, Leptos UI, domain services, SQLite persistence, automation supervisor, styles,
  assets, and browser tests.
- `dispatch-types/`: shared request/response DTOs and enum types.
- `dispatch-api-client/`: typed HTTP client for Dispatch JSON endpoints.
- `dispatch-cli/`: standalone agent-facing `dispatch` CLI binary; it relays to a running server and must not open
  SQLite.
- `crudkit/`: Git submodule used as a local dependency; do not put Dispatch workflow rules there.
- `knowledge/`: authoritative product, workflow, UI, API, CLI, data-model, and architecture specifications for Dispatch.

Keep Dispatch-specific claim, progress, finish, release, automation, and board behavior in Dispatch-owned server
services and custom API endpoints, not CrudKit routes.

## Commands

Run commands from the repository root through `just`, which passes explicit `--manifest-path` values because there is no
root Cargo workspace.

- `just fmt`: format all Dispatch crates.
- `just check`: check server, CLI, API client, and types crates.
- `just test`: run standard Rust tests for Dispatch crates.
- `just clippy`: run clippy with `--all-targets -- -D warnings` for Dispatch crates.
- `just verify`: run formatting, tests, and clippy.
- `just serve`: run the server with `.dispatch/dispatch.sqlite3` on `127.0.0.1:4000`.
- `just cli <args>`: run the API-relay CLI.
- `just browser-test`: run the ignored browser integration test; use `just browser-test-visible` for UI debugging.

Server-local overrides: `DISPATCH_DATABASE`, `DISPATCH_BIND`, `DISPATCH_PROJECT`, and `DISPATCH_WORKSPACE_IDE`.
`just serve` sets `DISPATCH_DEVELOPMENT=1`, allowing automation to build the CLI from this source checkout. Published
runs require the standalone `dispatch` CLI on `PATH`.

## Agent-Facing Contract

The Dispatch server is the only process that owns or writes the database. Agents interact with Dispatch through the
`dispatch` CLI, and the CLI is an API relay to `DISPATCH_API_URL`.

Dispatch-launched agents receive `DISPATCH_API_URL`, `DISPATCH_PROJECT`, and `DISPATCH_AGENT_ID`. Work-consuming runs
additionally receive `DISPATCH_CLAIMED_ITEM_ID`. For the claimed item, prompts should use short commands such as
`dispatch item show --json`,
`dispatch label list --json`, `dispatch label add --key ...`, `dispatch comment list --json`,
`dispatch item progress --body ...`, `dispatch item finish --report ...`, and `dispatch item release --comment ...`.
Agents may edit item labels themselves when that clarifies routing, status, priority, environment, or follow-up needs.

Knowledge content and relationships live in ordinary Markdown and frontmatter. Authorized repository work edits those
files directly. The agent interface is documented in `knowledge/knowledge-agents.md`; launched agents receive
current-file root context and navigate relevant documents progressively. Read-only runs produce reports or proposed
patches and do not edit the project. The server remains the sole owner of the operational database.

CLI context resolution must prefer explicit flags, then environment variables. Missing required project, agent, or
claimed-item context must fail instead of creating implicit data.

## Code-Style

Organize modules by domain behavior rather than generic buckets.

Always use the <module-name>/mod.rs style for defining modules having other submodules.

## Frontend Styling

Do not edit generated `style/crudkit` or `style/leptonic` content unless regenerating from the upstream source
intentionally. Put Dispatch-owned styling under `dispatch-server/style/app/`.

## Testing

Place focused unit tests near the code they exercise. Browser coverage lives in `dispatch-server/tests/browser_test.rs`
and is ignored by default because it starts Dispatch and Chrome.

Use `assertr` for every test assertion instead of the standard `assert!`, `assert_eq!`, and `assert_ne!` macros. Decide
whether to use `assertr` in production code case by case.

In browser tests, use the Thirtyfour API to query and interact with the current page. Do not use `WebDriver::execute` or
`execute_async` with JavaScript strings when the same operation can be expressed through element queries, properties,
CSS values, actions, waits, CDP, Rust HTTP requests, or server/database fixtures.

When changing workflow paths, cover project scoping, claim ownership, progress, release, finish, stale-claim recovery,
and version-safety behavior. When changing CLI/API behavior, cover context resolution and server-backed endpoint
behavior.

## Git And PRs

Do not infer a project-specific commit format from history. Use short imperative subjects, for example
`Add API relay CLI`.

PRs should include a concise behavior summary, verification commands run, linked work item or issue when available, and
screenshots or notes for UI changes. Call out schema, CLI, API, automation prompt, or agent-instruction changes
explicitly.

## General

Make sure to always use consistent wording when speaking about one concept. In code. In comments. In reasoning.
Everything else is confusing.
