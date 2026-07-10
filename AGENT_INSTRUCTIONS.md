# Dispatch Agent Instructions

`dispatch` is available on `PATH` and is the source of truth for work state, labels, comments, and project memory. Dispatch prepares `DISPATCH_API_URL`, `DISPATCH_PROJECT`, `DISPATCH_AGENT_ID`, and, for work-consuming runs, `DISPATCH_CLAIMED_ITEM_ID`.

## Live Work Contract

- Treat the claimed item as the current work contract.
- At the start of claimed-item work, run `dispatch item show --json` and `dispatch comment list --json` before making decisions. The prompt contains a launch-time snapshot, but users may edit the item or add comments while the run is starting.
- Commands taking an optional `[item-id]` default to `DISPATCH_CLAIMED_ITEM_ID`. Omit the item id for normal claimed-item work. Use an explicit id only when intentionally addressing another item.
- `item list`, `item create`, and `item claim` never use the claimed-item default.
- Use `--project`, `--agent`, or `--api-url` only when deliberately overriding the prepared context.

## Normal Command Path

```text
dispatch item show [item-id] [--json]
dispatch comment list [item-id] [--json]
dispatch item progress [item-id] --body "..." [--json]
dispatch item finish [item-id] --report "..." [--json]
dispatch item release [item-id] [--comment "..."] [--json]
dispatch item request-feedback [item-id] --body "..." [--json]
dispatch item update [item-id] [--title "..."] [--description "..."] [--state <state-label>] [--expect-version N] [--json]
dispatch label list [item-id] [--json]
dispatch label add [item-id] --key "..." [--value "..."] [--expect-version N] [--json]
dispatch memory show [--json]
dispatch memory append --body "..." [--json]
```

Use `dispatch --help` or `dispatch <command> --help` for less common operations instead of guessing command syntax.

## Progress And Terminal Transitions

- After a meaningful milestone, call `dispatch item progress --body "Short, specific progress note."`.
- Before ending claimed-item work, re-run `dispatch item show --json` and `dispatch comment list --json` so the final decision reflects current user edits.
- Completed work, including a justified no-code outcome: call `dispatch item finish --report "Done. Summary of changes and verification."`.
- A concrete decision or missing information that the user must provide: call `dispatch item request-feedback --body "Concrete question or decision needed."`.
- A technical blocker, failed implementation, or handoff that needs human triage: call `dispatch item release --comment "Exact blocker, verification status, and useful next step."`.
- When the run is responsible for resolving the claimed item, perform exactly one terminal transition: finish, request feedback, or release. A trigger may instead explicitly define a successful non-terminal consumer, such as refinement or verification that should leave the implementation item open; in that case, follow the trigger and let Dispatch restore the claim after a successful run.
- Never bypass a rejected transition with generic item or label updates. If another worker owns the item or the server rejects the operation, report the rejection.

## Runtime Boundaries

Dispatch runs Codex in the project's effective sandbox with network access configured by the run. If a command fails because a required tool home, cache, browser profile, macOS service, or other host path is outside the sandbox, do not work around the restriction. Report the exact failure and the extra writable root or sandbox-mode change that would likely be required.

Dispatch may put a run-specific `git` shim first on `PATH`. Use ordinary `git ...` commands and follow the Effective Run Policy in the developer instructions. If a Git command is blocked, report the exact command and blocker instead of bypassing the shim.

## Dispatch Data Rules

- You may add, update, or delete work-item labels when that clarifies routing, status, priority, environment, or follow-up needs.
- Move an item between swim-lanes with `dispatch item update --state <state-label>`; do not manipulate the reserved `state` label through generic label commands.
- Project memory is Dispatch-owned, not Codex memory. Use `dispatch memory append` for durable discoveries and `dispatch memory set` only for intentional full rewrites. Memory changes create attributed events.
- Keep progress and terminal reports concise, concrete, and explicit about verification that was not run.
