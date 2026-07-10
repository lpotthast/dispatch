# Dispatch Agent Instructions

`dispatch` (a CLI, available on PATH) is the source of truth for work state, labels, and project memory.

## Prepared Context

Dispatch-launched agents receive:

```sh
DISPATCH_API_URL=<api-url>
DISPATCH_PROJECT=<project-name>
DISPATCH_AGENT_ID=dispatch-run-<run-id>
DISPATCH_CLAIMED_ITEM_ID=<item-id>
DISPATCH_GIT_POLICY_PATH=<run-git-policy-json>
DISPATCH_REAL_GIT=<real-git-executable>
```

When `DISPATCH_CLAIMED_ITEM_ID` is set, commands taking an `[item-id]` default to that id; omit the item id for normal claimed-item work. `item list`, `item create`, and `item claim` do not use the claimed item. Use an explicit item id only when intentionally addressing another item. Use `--project`, `--agent`, or `--api-url` only when deliberately overriding the prepared context.

## Runtime Sandbox

Dispatch-launched agents run through the Codex SDK in the project's configured sandbox. The default is a restricted workspace-write sandbox with network access enabled. Additional writable host paths can be configured on the Dispatch project as extra writable roots, and projects can opt into a less restricted sandbox mode when a tool cannot work inside the workspace sandbox at all.

If a command fails in a way that looks caused by sandbox restrictions, such as denied writes to tool homes, caches, browser profiles, macOS app registration, or other host resources outside the workspace, do not work around it. Report the blocker in progress/final output and tell the user which project extra writable root or sandbox mode change would likely be needed.

Dispatch may put a run-specific `git` shim first on `PATH`. Use ordinary `git ...` commands and follow the generated prompt's Available Git Commands section. The shim enforces the project policy, including `--no-verify` on commits, no force/mirror/prune/delete/empty-source delete-refspec/`+ref` pushes, and hard-reset restrictions for the current workspace mode. If a Git command is blocked, report the exact command and blocker instead of bypassing the shim.

## CLI Quick Reference

Work item and comment commands:

```text
dispatch item list [--state <state-label>] [--json]
dispatch item show [item-id] [--json]
dispatch item create --title "..." --description "..." [--state <state-label>] [--agent-model MODEL] [--agent-reasoning-effort none|minimal|low|medium|high|xhigh] [--json]
dispatch item update [item-id] [--title "..."] [--description "..."] [--state <state-label>] [--agent-model MODEL] [--clear-agent-model] [--agent-reasoning-effort none|minimal|low|medium|high|xhigh] [--clear-agent-reasoning-effort] [--expect-version N] [--json]
dispatch item claim [--state <current-state-label>] [--json]
dispatch item progress [item-id] --body "..." [--json]
dispatch item finish [item-id] --report "..." [--json]
dispatch item release [item-id] [--comment "..."] [--json]
dispatch item request-feedback [item-id] --body "..." [--json]
dispatch item watch [item-id] [--since-version N] [--json]
dispatch label list [item-id] [--json]
dispatch label add [item-id] --key "..." [--value "..."] [--expect-version N] [--json]
dispatch label update [item-id] <label-id> [--key "..."] [--value "..."] [--clear-value] [--expect-version N] [--json]
dispatch label delete [item-id] <label-id> [--expect-version N] [--json]
dispatch label suggestions [--json]
dispatch comment list [item-id] [--json]
dispatch comment add [item-id] --body "..." [--author "..."] [--author-type user|agent|system] [--json]
```

Project memory commands:

```text
dispatch memory show [--json]
dispatch memory history [--json]
dispatch memory append --body "..." [--json]
dispatch memory set --body "..." [--json]
```

Project memory is tracked through Dispatch, not through Codex internal memory or any other assistant memory feature. The generated prompt includes the full project memory snapshot for this run in its Project Memory section. Use `memory append` for important facts future agents should receive. Use `memory set` only for intentional full rewrites. Memory writes create attributed `MemoryChanged` events.

Automation visibility:

```text
dispatch automation runs [--limit N] [--json]
dispatch automation log <run-id> [--json]
```

## Workflow

You MUST perform these calls when: progress is made, the task is finished or cannot be finished.

```sh
dispatch item progress --body "Short progress note."
dispatch item finish --report "Done. Summary of changes and verification."
dispatch item release --comment "Why work is being stopped or handed back."
dispatch item request-feedback --body "Concrete question or decision needed from the user."
```

## Rules

- Treat the claimed Dispatch item as the current work contract.
- Re-read the item and comments before finishing because humans may edit work while you run.
- You may add, update, and delete work item labels yourself when doing so clarifies status, routing, priority, environment, or follow-up needs.
- Work item swim-lanes are driven by the `state=<state-label>` label. Use `dispatch item update --state <state-label>` to move an item.
- Dispatch records the state label value that was active before claim. `dispatch item release` restores that value and adds `dispatch:automation-blocked`; leave that label in place when the item needs human triage before another automated attempt.
- When you need a user answer before continuing, call `dispatch item request-feedback --body "..."`. Dispatch records the request as an agent comment, restores the claimed-from state, clears the claim, and adds `dispatch:feedback-requested` plus `dispatch:automation-blocked` so automation waits for a user response.
- Keep progress comments concise and specific.
- Do not finish unless the requested work is complete or the final report explains why no code change was needed.
- If verification could not be run, say so in the finish report or release comment.
- If another worker owns an item or the server rejects a transition, do not bypass it with generic updates.
