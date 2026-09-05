---
id: dispatch.knowledge.transition
refines:
  - dispatch.knowledge
---

# Knowledge Replacement and Acceptance

The knowledge branch specifies a new first version, not the behavior of the old implementation. This document identifies
the replacement boundary and the scenarios that must work before the implementation is considered complete. It is the
sole transition note; the other documents specify the target behavior.

## Present boundary

The documentation rewrite is accepted design work. The running implementation still uses legacy signed Markdown,
sidecars, graph records, CLI mutation commands, review gates, and partially built knowledge-cycle infrastructure. Those
mechanisms are not requirements of the new design merely because they exist in code. Existing source explains current
mechanics, not the desired replacement.

The rewrite edits Markdown directly and does not update legacy managed files under `knowledge/.dispatch/`. That old
index/history may therefore reject or disagree with these files. Read the Markdown as the design authority during
implementation of the replacement; do not restore old prose or invoke signing/reconciliation simply to satisfy the
obsolete store. No claim of current legacy store validity is made.

The new frontmatter in this branch demonstrates the target format. Broader Dispatch documents have concise links to the
new owning contracts. Retired knowledge-system Markdown, including the old core-library documentation and
cycle-storage/workflow supplements, is removed rather than maintained as a competing specification. Version-control
history and the unchanged implementation remain available for deliberate historical investigation.

Until runtime cutover, existing launched runs may still encounter legacy instructions or completion gates. Report that
mismatch. Repository editing of the replacement is not proof that those runtime paths are already updated.
`AGENT_INSTRUCTIONS.md` remains a legacy launch template until it is changed with the runtime that consumes it; it is
not the target knowledge-agent interface.

This transition concerns the knowledge subsystem only. It does not waive unrelated work-item ownership, API, storage,
project-deletion, or security requirements.

## Keep the useful foundations

Reuse ordinary agent execution and run visibility, server-owned operational persistence, typed transport, project
scoping, filesystem traversal, lexical search, content comparison, process cleanup, and focused tests where they
implement the new contract. Deterministic parsing and graph helpers can remain in a small library if that is useful;
preserving the current crate or module decomposition is not a requirement.

Keep job history, logs, proposals, findings, and publication results as operational data. Rebuildable indexes remain
separate from that history. The new design does not require reconstructing historical execution from the Markdown files.

## Remove obsolete obligations

Remove requirements and implementation used only for installation signing keys, signer trust, signed canonical
transactions, multi-tip store history, authoritative per-document sidecars, portable claim-lineage records, per-query
navigation receipts, mandatory signed source attestations, and knowledge-review gates on item completion.

Replace the knowledge-specific phase/event/lease framework with the small job lifecycle and shared execution facilities.
Replace knowledge rules that create and consume reserved work items with direct knowledge-job scheduling. Do not
preserve all legacy endpoints or schemas as permanent parallel APIs. Preserve genuinely needed historical data and offer
a deliberate cutover rather than maintaining two active authorities.

Remove obsolete prompts, tests, settings, UI panels, and task-plan assumptions along with the code they enforced. In
particular, the previous `.plan/knowledge_automation.md` describes superseded implementation work; it does not add
requirements to this branch. Do not treat passing old signature or cycle tests as proof that the replacement is
complete.

## Implementation sequence

1. Implement document discovery, the small frontmatter schema, ignore handling, graph derivation, and local diagnostics.
   Demonstrate ordinary editor changes and rebuildable indexes without AI.
2. Implement working-copy-bound navigation, search, impact mapping, and checks. Update launch context and ordinary
   coding-agent instructions together; retire incompatible completion gates at the same boundary.
3. Add knowledge jobs using shared agent execution, filtered prepared inputs, progress/results, bounded scheduling, and
   visible job/run detail.
4. Implement initialization, update, drift, reorganization, and answer procedures using the shared authoring rules. Add
   checked proposal/application and interruption recovery.
5. Complete document/graph editing, user actions, findings, settings, and live-refresh behavior. Run the acceptance
   scenarios and remove obsolete public surfaces.

These steps are an implementation order, not a second job state machine. Each step should remove replaced paths instead
of leaving an indefinitely growing compatibility layer. Exact Rust types, database migrations, and HTTP DTO fields are
implementation details chosen to support the documented behavior.

## Existing-project cutover

Before changing a managed project, stop legacy knowledge admission and active writers, inventory its existing documents
and operational history, and prepare a reviewable migration with a recoverable backup. Do not modify unrelated staged or
unstaged work, automatically manipulate Git, or remove ignored files.

Import useful sidecar identities and supported document relationships into frontmatter, preserving existing Markdown and
unrelated metadata. Convert source references where their meaning is unambiguous. Map old stable citations to ordinary
links/anchors. Unsupported or conflicting relationships require an explicit migration decision rather than silent
deletion. Ensure the project root is `README.md` without overwriting another file at that path.

After validating the new file representation and index, retire the active legacy store metadata from the knowledge
directory into a deliberate backup/archive outside active discovery. Keep historical run/proposal data readable where it
is still meaningful. Do not fabricate new-format records for history that lacks the necessary facts. Remove obsolete
tables only through explicit migrations after preservation requirements have been met.

Enable only one knowledge scheduler for a project. Do not translate customized old rules into silently enabled recurring
model work. The user chooses the new schedules and application mode; initialization or migration alone leaves recurring
automation disabled. Rollback restores a known pre-cutover state and cannot overwrite independent edits made since
cutover.

## Acceptance scenarios

| Scenario                                         | Required result                                                                                                                                                            |
|--------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| New project with substantial existing code       | One Initialize action schedules visible work, analyzes bounded areas, writes detail before summaries, preserves existing documents, and exposes coverage gaps.             |
| Plain Markdown added in an editor                | It is readable/searchable immediately after refresh; missing identity/parent is a local organization diagnostic.                                                           |
| Ordinary behavior change                         | The agent reads relevant knowledge, changes intent before or alongside code, updates local documentation, checks affected parents, and leaves correct summaries unchanged. |
| Implementation bug contradicts design            | Drift reports the contradiction with evidence; an update does not redefine the requirement to match the bug.                                                               |
| Design-first work still in progress              | The intended contract and remaining implementation gap are visible; unrelated drift is still checked; unfinished work does not erase the gap.                              |
| README, API comment, and knowledge disagree      | The agent identifies the competing claims, uses intent and evidence to resolve or report them, and preserves useful local documentation.                                   |
| Shared detailed document split                   | Its relevant content survives, both broader parents remain correct, identities and incoming links are repaired, and navigation still works.                                |
| Reorganization repeated with unchanged inputs    | No gratuitous rewrite, oscillating split/merge, or progressive loss of important exceptions occurs.                                                                        |
| Existing document becomes ignored                | It disappears from active retrieval without deletion or a global store failure; visible references are diagnosed; removing the rule restores discovery.                    |
| Nested ignore rules and tracked files            | Both families obey the documented pattern/negation rules, excluded directories are not read, and Git tracking does not bypass exclusion.                                   |
| Duplicate ID, broken link, or refinement cycle   | Affected files remain readable by path, ambiguity is explicit, and unrelated navigation remains usable.                                                                    |
| Coding agent in a worktree                       | CLI reads and impact use that worktree and immediately reflect its document edits, not the main checkout.                                                                  |
| Concurrent edit during a background job          | Freshness checks prevent stale application; the draft remains inspectable and a retry uses current inputs.                                                                 |
| New subsystem has no source links                | Periodic inventory review identifies uncovered scope instead of reporting a complete clean scan based only on old links.                                                   |
| Question requires source evidence                | The visible answer job reads only permitted relevant source or states that it cannot; citations and uncertainty are explicit.                                              |
| AI provider unavailable                          | Reading, graph navigation, search, and structural checks still work; jobs show an actionable failure.                                                                      |
| Cancel, timeout, lost response, or restart       | Processes are cleaned up, useful output remains visible, incomplete publication is recovered, and changes are never applied twice.                                         |
| Knowledge job with no work item                  | Launch, progress, reporting, failure, retry, cancellation, and all run-detail views work without item operations or synthetic items.                                       |
| Automatic application                            | Supported intent-preserving changes can apply; disputed or consequential contract changes remain for review; the actual diff and outcome are visible.                      |
| Project deleted and recreated with the same name | Old jobs cannot act on the new project; workspace knowledge survives deletion.                                                                                             |

Validate structural behavior with focused automated tests and UI flows with browser coverage. Evaluate semantic
retention, summary quality, drift interpretation, and repeated rewriting with representative fixtures and human/AI
review. Tests that merely reproduce implementation mechanics do not substitute for these user outcomes.
