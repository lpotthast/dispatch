---
id: dispatch.knowledge.ui
refines:
  - dispatch.knowledge
  - dispatch.ui
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.automation
  - dispatch.knowledge.initialization
  - dispatch.knowledge.evaluation
---

# Knowledge User Interface

The project's Knowledge page lets users read, navigate, edit, and maintain knowledge, and inspect every knowledge job
and agent run. It remains useful as a document workspace when AI is unavailable. Work-item creation is an optional
follow-up action, never a prerequisite for knowledge work.

[Knowledge automation](knowledge-automation.md) owns job behavior and application policy;
[initialization](knowledge-initialization.md) owns bounded discovery and continuation. This document owns their user
actions and presentation within the shared [UI shell and navigation contract](ui.md).

## User actions

| Action                             | Expected result                                                                                                 |
|------------------------------------|-----------------------------------------------------------------------------------------------------------------|
| Initialize knowledge               | Schedule one bounded analysis of the existing project and show progress, the candidate documents, and coverage. |
| Continue discovery                 | Start another bounded linked job from current evidence and remaining scope, preserving accepted knowledge.      |
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

The workspace has a graph on the left and the selected document on the right. A tool rail opens actions in a side drawer
over the workspace; actions do not replace the graph or document with a separate page. The split is resizable by pointer
or keyboard. Search, document editing, relationships, diagnostics, AI actions, jobs, and settings use this shared
workspace. A document list is an alternative to the graph. The graph starts around the root or selected subject and can
expand to immediate neighbors or the full project. It is not necessary to fetch every document body to draw it. Layout
follows refinement where possible and uses stable ordering so unrelated updates do not constantly rearrange it.

Show titles, clear relation types and direction, and distinguish unorganized documents and invalid relationships.
Refinement connects detailed documents to broader ones; contextual relationships are visibly different. Source
references live in document details rather than turning the graph into a drawing of every source file. Missing targets
remain visible as diagnostics without exposing excluded bodies.

Users can pan, zoom, fit, select, and navigate by keyboard. A list provides equivalent navigation for users who cannot
or do not want to use the graph. Search results open and locate documents. On wide screens, graph/list and document
appear side by side; smaller screens place the document first and retain graph/list navigation below it. Do not require
graph manipulation to read or edit prose.

The editor offers raw Markdown and rendered preview. Relationship controls edit the same frontmatter as the raw editor
under the [document editing contract](knowledge-documents.md#editing-and-document-lifecycle). A plain Markdown file is
editable immediately, with a clear route to adding identity and refinement parents.

Save compares the content originally opened with current disk content. An intervening edit preserves the user's draft
and offers comparison/reload rather than overwriting it. Same-project refresh preserves selection, scroll position,
graph viewport, and dirty drafts. Closing the editor, selecting a different document, switching project, or navigating
away uses the shared dirty-edit guard. Escape and the drawer close button follow that same guard and restore focus to
the originating action. A bad document does not unmount the entire workspace.

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

A persistent activity strip above the graph/document workspace shows discovery activity and opens the Knowledge Jobs
drawer. Initialize and Continue discovery use this drawer, with optional context and advanced runtime limits. The graph,
selection, viewport, and guarded dirty editor remain in the shared workspace. Starting a job immediately exposes its
queued state and progress.

The Knowledge page includes queued, active, awaiting-review, and past jobs. Show kind, trigger, scope, timestamps,
current stage, area/aspect, remaining scope, coverage, reading-quality results, drafts, outcome, and links to underlying
agent runs. Every Dispatch-triggered knowledge agent run appears here, including failed and cancelled runs. Job detail
remains available after the process exits or the server restarts.

Reuse shared agent-run detail and live logs, including model/effort, effective instructions and task, runtime events,
duration, failure reason, and token usage when reported. Run lists distinguish ordinary tasks from knowledge jobs and
can filter by purpose. A knowledge run links back to its job and has no fabricated item link, claim timer, or item
status. Existing run detail must work when the work-item field is absent. Cancelling a knowledge run delegates to its
job's cancellation behavior.

Results show an answer, changed documents with diffs, a proposal, drift findings, explicit no-change outcome, or
incomplete analysis. Completion must not imply consistency; display "drift found" separately from execution failure.
Applied changes with a refresh warning remain labeled applied; a warning must not suggest the write was rolled back.
Interrupted jobs expose the available draft and a retry action using current inputs.

## Coverage and reading results

The Coverage view shows unexplored and stale regions, an aspect filter, associated knowledge, and outstanding known
aspects. Coverage entries link to their knowledge owners. Aggregate line consideration is labeled **considered for at
least one aspect**, following the [evaluation semantics](knowledge-evaluation.md#aspect-specific-consideration), and is
never presented as a completeness score.

Reading results show correctness and unanswered questions alongside estimated reading costs and compression. Estimated
tokens remain visibly separate from provider usage. Questions, evidence, and evaluations are inspected as job artifacts.

## Proposals and findings

A proposal shows the before/after diff, explanation, affected references, scope gaps, validation issues, and source job.
Inspect, edit, apply, and reject controls use the
[proposal application contract](knowledge-automation.md#reorganization-and-automatic-application). A stale proposal
offers refresh or editing and revalidation; dismissing a warning cannot enable application. Rejected proposals remain
inspectable in job history.

A finding shows the disputed claim, supporting evidence, relevant locations, unresolved questions, and any known
implementation transition. Users can request a knowledge correction, resolve it with evidence or an explanation, or
create a work item for code changes. Its resolution follows
[drift finding semantics](knowledge-automation.md#updates-and-drift), independently of the linked work item's state.

## Settings and degraded operation

Knowledge schedules and application mode belong to the Knowledge page or its settings panel. Work-item automation has
its own controls. Show whether recurring work is paused, why a job is waiting, and what the next enabled schedule will
do. Manual initialization, scans, and questions remain available while recurring work is paused.

Show effective model/effort, runtime limits, and application mode, including inherited values from
[knowledge settings](knowledge-automation.md#settings-scheduling-and-triggers). When provider usage is absent, show that
token metering is unavailable; do not advertise an exact token or monetary ceiling.

Knowledge-directory relocation previews file moves, collisions, exclusions, and affected references before the user
requests the checked move defined by [document lifecycle](knowledge-documents.md#location-and-discovery).

Exclusion changes show which visible documents will leave or re-enter participation, preserve all file bytes, and
refresh active views. Do not display ignored contents through old graph/search caches. Explain that exclusions do not
retroactively erase prior model inputs or history. Editing an ignore control is an explicit user operation, not an
autonomous reorganization action.

For a user-supplied path, explain the matching control and rule without reading the excluded target body. The
[ignore contract](knowledge-documents.md#ignore-rules) owns participation and cache invalidation semantics.

The Knowledge service caches typed document, graph, diagnostic, job, proposal, and settings data per project and working
copy. It checks current content/index generation before treating cached data as current. Local metadata errors
invalidate affected projections, not the whole workspace. Excluded content is removed from active caches. Historical
artifacts load only when requested; refresh follows the document workspace's state-preservation rules above.

Missing roots, unreadable paths, invalid metadata, absent models, and exhausted budgets produce local, actionable
messages. Reading valid documents remains available whenever their files can be read. Reading and document editing
require neither a signer-trust flow nor a work-item diagnostics queue.
