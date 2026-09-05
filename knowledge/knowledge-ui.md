---
id: dispatch.knowledge.ui
refines:
  - dispatch.knowledge
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.automation
---

# Knowledge User Interface

The project's Knowledge page lets users read, navigate, edit, and maintain knowledge, and inspect every knowledge job
and agent run. It remains useful as a document workspace when AI is unavailable. Work-item creation is an optional
follow-up action, never a prerequisite for knowledge work.

## User actions

| Action                             | Expected result                                                                                                 |
|------------------------------------|-----------------------------------------------------------------------------------------------------------------|
| Initialize knowledge               | Schedule one bounded analysis of the existing project and show progress, the candidate documents, and coverage. |
| Ask a free-form question           | Start a visible answer job and present a cited answer with uncertainty and scope limits.                        |
| Search                             | Return immediate lexical matches without launching AI.                                                          |
| Browse documents and graph         | Open relevant explanations and follow broader, narrower, dependent, and related subjects.                       |
| Create or edit a document          | Save ordinary Markdown and optional frontmatter, with preview and local diagnostics.                            |
| Request an AI edit                 | Describe the desired change and start a scoped update or reorganization job.                                    |
| Edit relationships                 | Change authored frontmatter relationships through understandable document selectors.                            |
| Move, rename, or delete a document | Inspect affected references and perform the requested file change.                                              |
| Update knowledge now               | Analyze accepted changes and refresh affected explanations and summaries.                                       |
| Scan for drift now                 | Report contradictions and unexamined areas without silently repairing either side.                              |
| Reorganize now                     | Propose or apply justified splits, merges, moves, and summary improvements.                                     |
| Review a proposal                  | Inspect its diff and reasoning, edit it, apply it, or reject it.                                                |
| Resolve a finding                  | Record the correction evidence or the explanation for resolving it.                                             |
| Create a work item from a finding  | Optionally transfer needed implementation work, retaining a link to its origin.                                 |
| Inspect jobs and runs              | View status, live progress, logs, usage when available, failures, results, and changed documents.               |
| Cancel or retry a job              | Stop pending/active work or start a new attempt with current inputs.                                            |
| Configure automatic work           | Set schedules, change triggers, model/effort, budgets, and review or automatic application.                     |
| Pause or resume automation         | Control recurring knowledge admission independently of work-item automation.                                    |
| Configure exclusions               | Edit project `.gitignore` and `.dispatchignore` rules and inspect their participation effects.                  |

Where meaningful, actions accept the whole project or selected documents/subsystems. The UI shows the selected scope
before starting a job. An agent may need related context within the permitted scope and budget; selecting one document
is not a promise that no other document will be read.

## Document and graph workspace

Provide a document list and a navigable graph alongside the selected document. The graph starts around the root or
selected subject and can expand to immediate neighbors or the full project. It is not necessary to fetch every document
body to draw it. Layout follows refinement where possible and uses stable ordering so unrelated updates do not
constantly rearrange it.

Show titles, clear relation types and direction, and distinguish unorganized documents and invalid relationships.
Refinement connects detailed documents to broader ones; contextual relationships are visibly different. Source
references live in document details rather than turning the graph into a drawing of every source file. Missing targets
remain visible as diagnostics without exposing excluded bodies.

Users can pan, zoom, fit, select, and navigate by keyboard. A list provides equivalent navigation for users who cannot
or do not want to use the graph. Search results open and locate documents. On wide screens, graph/list and document can
appear side by side; smaller screens retain a usable document-first layout. Do not require graph manipulation to read or
edit prose.

The editor offers raw Markdown and rendered preview. Relationship controls edit the same frontmatter as the raw editor;
they do not create a second metadata store. Preserve unknown frontmatter fields. A plain Markdown file is editable
immediately, with a clear route to adding identity and refinement parents.

Save compares the content originally opened with current disk content. An intervening edit preserves the user's draft
and offers comparison/reload rather than overwriting it. Same-project refresh preserves selection, scroll position,
graph viewport, and dirty drafts. Switching project or navigating away uses the existing dirty-edit guard. A bad
document does not unmount the entire workspace.

## Answers

Answer and search are distinct actions. Search is immediate; asking AI creates a job with its model, scope, progress,
cancellation, and result visible. The answer distinguishes documented facts, observed implementation, inference, and
gaps. References open the relevant document or source location and identify the version read. Source is consulted only
when needed and permitted; if unavailable, the answer says so.

An old answer remains historical. When its supporting documents or source have changed, label it as potentially outdated
rather than presenting it as a current verified answer. Repeating a question can reuse an answer only when its relevant
inputs and effective permissions still match; the first version need not implement an answer cache. Provider failures
leave document reading and search available.

## Jobs and agent-run visibility

The Knowledge page includes queued, active, awaiting-review, and past jobs. Show kind, trigger, scope, timestamps,
current progress, outcome, and links to the underlying agent runs. Every Dispatch-triggered knowledge agent run appears
here, including failed and cancelled runs. Job detail remains available after the process exits or the server restarts.

Reuse shared agent-run detail and live logs, including model/effort, effective instructions and task, runtime events,
duration, failure reason, and token usage when reported. Run lists distinguish ordinary tasks from knowledge jobs and
can filter by purpose. A knowledge run links back to its job and has no fabricated item link, claim timer, or item
status. Existing run detail must work when the work-item field is absent. Cancelling a knowledge run delegates to its
job's cancellation behavior.

Results show an answer, changed documents with diffs, a proposal, drift findings, explicit no-change outcome, or
incomplete analysis. Display "drift found" separately from execution failure. Applied changes with a refresh warning
remain labeled applied; a warning must not suggest the write was rolled back. Interrupted jobs expose the available
draft and a retry action using current inputs.

## Proposals and findings

A proposal shows the before/after diff, explanation, affected references, scope gaps, validation issues, and source job.
Apply checks freshness again. A stale proposal cannot be applied merely by dismissing a warning; it must be refreshed or
explicitly edited and revalidated against current content. Rejection preserves its history without altering the
documents.

A finding shows the disputed claim, supporting evidence, relevant locations, unresolved questions, and any known
implementation transition. Users can request a knowledge correction, resolve it with evidence or an explanation, or
create a work item for code changes. Creating or finishing that work item does not automatically resolve the finding; a
later check or user resolution does.

## Settings and degraded operation

Knowledge schedules and application mode belong to the Knowledge page or its settings panel. Work-item automation has
its own controls. Show whether recurring work is paused, why a job is waiting, and what the next enabled schedule will
do. Manual initialization, scans, and questions remain available while recurring work is paused.

Exclusion changes show which visible documents will leave or re-enter participation, preserve all file bytes, and
refresh active views. Do not display ignored contents through old graph/search caches. Explain that exclusions do not
retroactively erase prior model inputs or history. Editing an ignore control is an explicit user operation, not an
autonomous reorganization action.

Missing roots, unreadable paths, invalid metadata, absent models, and exhausted budgets produce local, actionable
messages. Reading valid documents remains available whenever their files can be read. No signer-trust panel,
signed-history browser, typed-change editor, or mandatory work-item diagnostics queue is required by the replacement
design.
