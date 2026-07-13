# CLI Design

Dispatch has two command surfaces:

- the standalone agent-facing `dispatch` binary from `dispatch-cli`;
- trusted server/operator commands in `dispatch-server`.

Only the standalone `dispatch` binary is part of the agent contract.

## Agent-Facing Contract

Launched agents are instructed to use:

```text
dispatch item show --json
dispatch comment list --json
dispatch item progress --body "..."
dispatch item finish --report "..."
dispatch item release --comment "..."
dispatch item request-feedback --body "..."
dispatch memory append --body "Important project fact to remember."
```

For follow-up items or explicit cross-item work, item ids remain available:

```text
dispatch item show 124 --json
dispatch comment list 124 --json
dispatch item progress 124 --body "Updated follow-up context."
```

Agents should omit project, agent, and claimed item arguments for the claimed item because Dispatch sets the environment before launch.

Dispatch may also put a run-specific `git` shim first on `PATH`. That shim calls an internal `dispatch git ...` guard command, which reads `DISPATCH_GIT_POLICY_PATH` and `DISPATCH_REAL_GIT`, enforces the project mutable Git command policy, and then delegates to the real Git executable. This internal command is not part of the normal agent workflow; agents should run ordinary `git ...` commands and follow the generated prompt's allow-list.

## Context Resolution

The standalone CLI resolves context in this order:

- API URL: `--api-url`, `DISPATCH_API_URL`, `DISPATCH_URL`, then the default local URL.
- Project: `--project`, then `DISPATCH_PROJECT`.
- Agent: `--agent`, then `DISPATCH_AGENT_ID`.
- Claimed item: explicit positional item id, then `DISPATCH_CLAIMED_ITEM_ID`.

Commands that choose or create work do not default to the claimed item. This includes:

- `item claim`;
- `item list`;
- `item create`.

Commands that operate on an existing item accept an optional item id and may default to the claimed item:

- `item show [item-id]`;
- `item update [item-id]`;
- `item progress [item-id]`;
- `item finish [item-id]`;
- `item release [item-id]`;
- `item request-feedback [item-id]`;
- `item watch [item-id]`;
- `comment list [item-id]`;
- `comment add [item-id]`.
- `relationship list [item-id]`;
- `relationship add [item-id] --target <item-id> --kind "..."`.

Relationship update and delete commands take a relationship id directly and do not require a claimed-item context.

## Commands

Work item commands:

```text
dispatch item list [--state <state>] [--json]
dispatch item show [item-id] [--json]
dispatch item create --title "..." --description "..." [--state <state>] [--label <key[=value]>] [--json]
dispatch item update [item-id] [options] [--json]
dispatch item claim [--state <state-label>] [--json]
dispatch label list [item-id] [--json]
dispatch label add [item-id] --key "..." [--value "..."] [--json]
dispatch label update [item-id] <label-id> [--key "..."] [--value "..."] [--clear-value] [--json]
dispatch label delete [item-id] <label-id> [--json]
dispatch label suggestions [--json]
dispatch item progress [item-id] --body "..." [--json]
dispatch item finish [item-id] --report "..." [--json]
dispatch item release [item-id] [--comment "..."] [--json]
dispatch item request-feedback [item-id] --body "..." [--json]
dispatch item watch [item-id] [--since-version <n>] [--json]
```

Label commands manage ordinary non-state labels. The reserved `state` label is
changed through `dispatch item create --state ...`, `dispatch item update
--state ...`, or workflow commands so state movement uses the item workflow
path.

Relationship commands:

```text
dispatch relationship list [item-id] [--json]
dispatch relationship add [item-id] --target <item-id> --kind "is follow-up of" [--json]
dispatch relationship update <relationship-id> --kind "blocks" [--json]
dispatch relationship delete <relationship-id> [--json]
```

Relationship commands call the Dispatch JSON API. List output includes incoming and outgoing relationships touching the item, direction relative to that item, the source and target item summaries, and the free-form kind. Add uses the command item as the source and the `--target` item as the target. Update replaces only the kind. Delete removes only the specified relationship; it does not create or remove inverse relationships.

Comment commands:

```text
dispatch comment list [item-id] [--json]
dispatch comment add [item-id] --body "..." [--author "..."] [--author-type user|agent|system] [--json]
```

Memory commands:

```text
dispatch memory show [--json]
dispatch memory history [--json]
dispatch memory append --body "..." [--json]
dispatch memory set --body "..." [--json]
```

`memory append` and `memory set` require project and agent context. They write through the Dispatch API, never through Codex internal memory, and create attributed `MemoryChanged` events.

Internal automation command:

```text
dispatch git <git-args...>
```

This is used only by Dispatch's run-specific Git shim. It injects `--no-verify` for `git commit`, blocks force/mirror/prune/delete/empty-source delete-refspec/`+ref` pushes, and blocks reset modes outside the project policy.

Automation commands:

```text
dispatch automation runs [--limit <n>] [--json]
dispatch automation log <run-id> [--json]
dispatch automation triggers list [--json]
dispatch automation triggers create --name "..." --activation manual|work_item|cron|work_item_created --effect produce_work|consume_work --schedule "@every 15s" [--work-item-selector <json>] [--prompt "..."] [--json]
dispatch automation triggers schedule <trigger-id> [--json]
```

Global flags:

```text
--api-url <url>
--project <project>
--agent <agent-id>
```

## CLI Availability And Development Builds

Published Dispatch installations must provide the standalone `dispatch` CLI as an executable on the server's `PATH`. Before Codex starts or an item is claimed, the server resolves that executable and runs `dispatch --help` to verify that it is the agent-facing relay rather than an unrelated binary with the same name. A missing, non-executable, broken, or incorrect CLI rejects the automation launch because an agent without it cannot read or update Dispatch state.

Repository development is explicit. `just serve` and `just serve-release` set `DISPATCH_DEVELOPMENT=1`, which means the sibling `dispatch-cli/Cargo.toml` source is expected to exist. Before each agent launch, the server runs Cargo's build and freshness check outside the agent sandbox, then puts the resulting executable on the agent's `PATH`. `CARGO_TARGET_DIR`, then `DISPATCH_CLI_TARGET_DIR`, can override the default stable temporary `dispatch-cli-target` build directory.

Development runs share that target executable. If CLI source changes between launches, the next launch may rebuild and replace the executable path that an already-running agent uses for later `dispatch` invocations. This should be rare and is an accepted development-only limitation; Dispatch does not currently create a unique CLI copy per run.

## Server Operator CLI

The server crate also contains trusted commands for running the server and operating local state. That surface may accept database paths and perform privileged maintenance. It must not be presented as the normal agent-facing Dispatch interface.
