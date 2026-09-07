---
id: dispatch.knowledge.automation
refines:
  - dispatch.knowledge
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.pyramid
  - dispatch.knowledge.agents
---

# Knowledge Automation

Knowledge automation runs bounded jobs that initialize, update, inspect, or reorganize a project's knowledge. Jobs have
their own results and lifecycle. They reuse Dispatch's agent runtime and remain independent of work-item creation,
claims, labels, comments, and completion.

[Initialization and continued discovery](knowledge-initialization.md) owns discovery admission, passes, budgets, and
candidate acceptance. [Knowledge UI](knowledge-ui.md) owns job controls, settings presentation, and result visibility.

## Job kinds and ownership

| Kind         | Trigger and output                                                                                                                             |
|--------------|------------------------------------------------------------------------------------------------------------------------------------------------|
| `initialize` | A user requests [initialization or continued discovery](knowledge-initialization.md); produces a knowledge candidate and coverage explanation. |
| `update`     | Accepted design changes, source/document changes, or a user request; refreshes affected knowledge from the bottom upward.                      |
| `drift`      | A schedule or user request; reports contradictions and uncovered areas without changing source or knowledge.                                   |
| `reorganize` | A schedule, diagnostics, or user request; improves placement, summaries, splits, merges, and navigation while preserving meaning.              |
| `answer`     | A user's free-form question; returns a cited answer and any uncertainty.                                                                       |

An answer is a read-only knowledge job using the same execution and visibility facilities. It does not need a second
background-job system. Navigation, indexing, and structural checks are ordinary operations and do not allocate an agent
run.

A job belongs to an immutable project ID and records kind, trigger, requested scope, status, input working copy,
effective settings, underlying agent runs, progress, result, and timestamps. The requested scope may be the project,
selected documents, source paths, or a question. Necessary context can expand within the job's permitted project and
budget; the result explains that expansion. IDs and content fingerprints are practical correlation and freshness data,
not an event-sourced proof system.

## Lifecycle and runtime reuse

Statuses are `queued`, `running`, `awaiting_review`, `completed`, `failed`, and `cancelled`. A job starts queued,
becomes running when admitted, and either completes, fails, is cancelled, or retains a proposed diff awaiting review. A
reviewed proposal completes with an applied or rejected result. Stale proposals are marked as such and require a fresh
attempt before application. Awaiting-review jobs consume no active agent slot.

Admission resolves project settings and allocates the underlying run and its job checkpoint in one transaction. A
failed checkpoint leaves no allocated run or success notification. Continuations inherit retained history in their
allocation transaction. Source preparation and process execution happen outside these short database transactions.

Results distinguish an answer, no change needed, applied changes, proposed changes, drift findings, incomplete coverage,
and an unresolved question. A scan that found drift can complete successfully: the finding remains unresolved. An
operational failure is distinct from a successful scan with findings; completed does not imply consistent. A result
needing a user answer records the question and available findings; a later answer starts a linked follow-up job using
current inputs rather than keeping a provider process suspended indefinitely.

Reuse agent model and reasoning configuration, process launch and cleanup, sandboxing, live and persisted logs, timeout,
cancellation, token usage, and global/project resource accounting. No job targets a work item. Neither launch nor
success/failure cleanup may fall back to claim, release, finish, or comment code. An optional work item created later
from a finding is a user action and never owns the job's state.

One knowledge job runs at a time per project in the first version. Other projects remain independent, subject to shared
runtime limits. A background knowledge job must not publish into a workspace while an admitted coding agent is mutating
it. Periodic jobs wait for that activity to finish; a manual read-only scan may inspect a stable prepared copy during
work but records its result as an in-progress observation. Answers use the same queue and cancellation contract.

The server supervises work and validates output. The agent does not declare that its changes were applied or that its
process should be recorded as successful merely by submitting a report.

## Settings, scheduling, and triggers

New projects start with recurring knowledge automation disabled. Manual jobs do not enable recurring model calls and
remain available while recurring automation is paused.

Project knowledge settings expose enabled schedules for drift and reorganization, change-triggered updates,
model/effort, execution timeout and token budget, and application mode (`review` or `automatic`). Defaults inherit
compatible project model/runtime settings. Review is the default application mode. Timeouts are enforced by the runtime.
A reported-token budget stops further work when observed usage reaches its limit, but an in-flight model turn can exceed
it. If the provider supplies no usage, token metering is unavailable and the runtime uses the time limit. These budgets
provide no exact token or monetary ceiling. Suggested schedules are daily drift checks and weekly reorganization checks,
enabled only by the user. Cheap weekly inventory reconciliation can accompany enabled recurring automation without
requiring an AI call.

Watch source and document changes, coalesce repeated notifications, and wait for a short quiet period before queuing
work. Watcher events are hints; compare current inventory and content fingerprints to the last processed inputs before
deciding scope. Source additions/deletions, document text, relationships, and ignore changes all matter. Do not skip
semantic work solely because source bytes are unchanged when design changed.

Changes made by a job are accounted for by that job's bottom-up review and do not create an immediate identical
follow-up job. Independently arriving edits remain pending. A periodic inventory comparison catches missed events and
previously unlinked components. Keep one combined pending change-triggered update rather than a job per file event.
User-requested jobs retain their individual identity and requested purpose.

Pause stops new recurring admission; it does not silently cancel a running job or discard queued manual requests.
Cancellation is a separate visible action. On resume, reconcile current inputs rather than replaying every missed timer
occurrence. No configured recurring work means no idle model polling.

## Inputs and freshness

Prepare permitted source and knowledge using current project-root ignore controls. The input contains only needed,
allowed content; source is read-only and knowledge drafts are writable for authoring jobs. Capture fingerprints and
membership of the input scope, including explicit omissions and unreadable areas. Detect files changing while copying
and retry preparation within a bound. A dirty Git checkout is valid input when its actual bytes can be captured
consistently; a clean repository is not an acceptance requirement.

Before publishing, recheck the files and scope facts the result depends on, the destination files, and ignore controls.
New or deleted relevant files also matter. An intervening change makes affected work stale, not automatically mergeable.
Refresh that analysis or offer it as a stale proposal; never overwrite newer work. Unrelated changes do not require
reanalyzing the entire repository when independence can be established.

The normal project working copy remains usable while a draft is prepared. Background agents cannot expand source/network
permissions through their output or frontmatter. The publication step accepts only intended knowledge-directory changes,
excluding control files and runtime metadata. Source changes require an ordinary coding task.

## Updates and drift

An update reconciles accepted design changes and observed implementation using the
[bottom-up authoring rules](knowledge-pyramid.md#updating-from-the-bottom-upward). It can add missing explanation or
repair a source link. A new requirement requires an explicit user/task decision; an automatic update cannot infer
acceptance merely because code changed or an item finished.

A drift scan compares statements with relevant code, tests, schemas, and source documentation. Fingerprints select
candidates and avoid redundant model work; they do not establish semantic consistency. Periodic broader inventory review
finds unlinked new areas and accumulated blind spots. A finding names the claim, observed contradiction, supporting
references, likely affected scope, and uncertainty. "Not examined" must not become "no drift."

An accepted contract remains in knowledge even when code does not satisfy it. Jobs record an implementation gap in their
findings, using task reports or run evidence to identify active design-first work. Outside that scope, ordinary drift
reporting continues. Once work ends, recheck the transition; completion or failure alone cannot erase it. Reuse an
unresolved finding for the same contradiction instead of opening a duplicate on each scan. Resolve it only with
correction evidence or a user explanation, preserving the reason in the job/finding history.

## Reorganization and automatic application

Reorganization reads the affected documents and necessary evidence, identifies a concrete reading or maintenance
problem, and applies the [authoring rules for reorganization](knowledge-pyramid.md#splits-merges-and-removals).

In review mode, document-changing jobs produce inspectable proposals. In automatic mode, Dispatch may apply supported
updates within existing accepted intent and reorganizations that preserve meaning. Changes to requirements, disputed
meaning, compatibility promises, security boundaries, or other consequential contracts remain proposals requiring a user
decision. The worker explains why a change is eligible; deterministic validation checks files, scope, and structure but
cannot prove the semantic explanation. Uncertainty falls back to review, not model-confidence thresholds.

A proposal is an ordinary diff with input fingerprints, explanation, findings, and its job/run link. Editing a proposal
requires renewed structural and freshness checks; application follows the lifecycle and freshness contract above.
Before/after content is retained with the result so an applied change is reviewable; later restoration is another
checked edit, not an unconditional reset of the project.

## Publication, cancellation, and recovery

Serialize publication per target workspace and coordinate with coding-agent admission. Check the complete intended diff
before writing. Use temporary files, atomic single-file replacements, and a small recovery record for multi-file changes
so interruption is visible and repairable. Recheck destination bytes before replacement to detect external edits. An
ordinary editor that ignores Dispatch's lock cannot be promised a perfectly atomic multi-file view; recovery must
preserve conflicting bytes and require resolution instead of deleting them.

After files have been written, a failed index refresh or notification is recorded as applied with a warning and retried.
It must not be reported as though no change occurred. A lost response can be resolved by inspecting the existing job
result; retrying reporting must not apply the diff twice.

Cancellation is persisted before stopping execution and preserves drafts, assessments, findings, evaluations, and
incremental logs. Cancel before publication to leave target files untouched. Once publication starts, finish or recover
that bounded write before recording the outcome; cancellation cannot imply that already written files vanished. Shutdown
interrupts execution without recording user cancellation. Startup recovers publication, cleans up verified surviving
processes, and resumes valid checkpoints within the job's recovery limit and original budget; user cancellation never
auto-resumes. At most three automatic recovery attempts are allowed. An explicit retry starts a linked job with current
inputs. Publication writes the root last and preserves conflicting external edits.

Deleting the project stops jobs and processes before removing project-owned operational artifacts, while preserving
workspace knowledge. Deleting and recreating the same project name does not inherit old jobs or write permissions.
