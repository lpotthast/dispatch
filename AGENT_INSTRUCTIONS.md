# Patchbay Agent Instructions

`patchbay` (a CLI, available on PATH) is the source of truth for work state, labels, and project memory.

## Prepared Context

Patchbay-launched agents receive:

```sh
PATCHBAY_API_URL=<api-url>
PATCHBAY_PROJECT=<project-name>
PATCHBAY_AGENT_ID=patchbay-run-<run-id>
PATCHBAY_CLAIMED_ITEM_ID=<item-id>
```

When `PATCHBAY_CLAIMED_ITEM_ID` is set, commands taking an `[item-id]` default to that id; omit the item id for normal claimed-item work. `item list`, `item create`, and `item claim` do not use the claimed item. Use an explicit item id only when intentionally addressing another item. Use `--project`, `--agent`, or `--api-url` only when deliberately overriding the prepared context.

## Runtime Sandbox

Patchbay-launched agents run through the Codex SDK in the project's configured sandbox. The default is a restricted workspace-write sandbox with network access enabled. Additional writable host paths can be configured on the Patchbay project as extra writable roots, and projects can opt into a less restricted sandbox mode when a tool cannot work inside the workspace sandbox at all.

If a command fails in a way that looks caused by sandbox restrictions, such as denied writes to tool homes, caches, browser profiles, macOS app registration, or other host resources outside the workspace, do not work around it. Report the blocker in progress/final output and tell the user which project extra writable root or sandbox mode change would likely be needed.

## CLI Quick Reference

Work item and comment commands:

```text
patchbay item list [--state <state-label>] [--json]
patchbay item show [item-id] [--json]
patchbay item create --title "..." --description "..." [--state <state-label>] [--agent-model MODEL] [--agent-reasoning-effort none|minimal|low|medium|high|xhigh] [--json]
patchbay item update [item-id] [--title "..."] [--description "..."] [--state <state-label>] [--agent-model MODEL] [--clear-agent-model] [--agent-reasoning-effort none|minimal|low|medium|high|xhigh] [--clear-agent-reasoning-effort] [--expect-version N] [--json]
patchbay item claim [--state <current-state-label>] [--json]
patchbay item progress [item-id] --body "..." [--json]
patchbay item finish [item-id] --report "..." [--json]
patchbay item release [item-id] [--comment "..."] [--json]
patchbay item watch [item-id] [--since-version N] [--json]
patchbay label list [item-id] [--json]
patchbay label add [item-id] --key "..." [--value "..."] [--expect-version N] [--json]
patchbay label update [item-id] <label-id> [--key "..."] [--value "..."] [--clear-value] [--expect-version N] [--json]
patchbay label delete [item-id] <label-id> [--expect-version N] [--json]
patchbay label suggestions [--json]
patchbay comment list [item-id] [--json]
patchbay comment add [item-id] --body "..." [--author "..."] [--author-type user|agent|system] [--json]
```

Project memory commands:

```text
patchbay memory show [--json]
patchbay memory history [--json]
patchbay memory append --body "..." [--json]
patchbay memory set --body "..." [--json]
```

Project memory is tracked through Patchbay, not through Codex internal memory or any other assistant memory feature. The generated prompt includes the full project memory snapshot for this run in its Project Memory section. Use `memory append` for important facts future agents should receive. Use `memory set` only for intentional full rewrites. Memory writes create attributed `MemoryChanged` events.

Automation visibility:

```text
patchbay automation runs [--limit N] [--json]
patchbay automation log <run-id> [--json]
```

## Workflow

You MUST perform these calls when: progress is made, the task is finished or cannot be finished.

```sh
patchbay item progress --body "Short progress note."
patchbay item finish --report "Done. Summary of changes and verification."
patchbay item release --comment "Why work is being stopped or handed back."
```

## Rules

- Treat the claimed Patchbay item as the current work contract.
- Re-read the item and comments before finishing because humans may edit work while you run.
- You may add, update, and delete work item labels yourself when doing so clarifies status, routing, priority, environment, or follow-up needs.
- Work item swim-lanes are driven by the `state=<state-label>` label. Use `patchbay item update --state <state-label>` to move an item.
- Patchbay records the state label value that was active before claim. `patchbay item release` restores that value and adds `patchbay:automation-blocked`; leave that label in place when the item needs human triage before another automated attempt.
- Keep progress comments concise and specific.
- Do not finish unless the requested work is complete or the final report explains why no code change was needed.
- If verification could not be run, say so in the finish report or release comment.
- If another worker owns an item or the server rejects a transition, do not bypass it with generic updates.
