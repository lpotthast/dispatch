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

## Job kinds and ownership

| Kind         | Trigger and output                                                                                                                       |
|--------------|------------------------------------------------------------------------------------------------------------------------------------------|
| `initialize` | A user schedules initial knowledge construction from an existing project; produces documents, relationships, and a coverage explanation. |
| `update`     | Accepted design changes, source/document changes, or a user request; refreshes affected knowledge from the bottom upward.                |
| `drift`      | A schedule or user request; reports contradictions and uncovered areas without changing source or knowledge.                             |
| `reorganize` | A schedule, diagnostics, or user request; improves placement, summaries, splits, merges, and navigation while preserving meaning.        |
| `answer`     | A user's free-form question; returns a cited answer and any uncertainty.                                                                 |

An answer is a read-only knowledge job using the same execution and visibility facilities. It does not need a second
background-job system. Navigation, indexing, and structural checks are ordinary operations and do not allocate an agent
run.

A job belongs to an immutable project ID and records kind, trigger, requested scope, status, input working copy,
effective settings, underlying agent runs, progress, result, and timestamps. The requested scope may be the project,
selected documents, source paths, or a question. Necessary context can expand within the job's permitted project and
budget; the result explains that expansion. IDs and content fingerprints are practical correlation and freshness data,
not an event-sourced proof system.

The first implementation can run one agent per job. Large initialization can use sequential bounded passes and retained
intermediate notes. Parallel scouts, separate reducers, reusable computation identities, and distributed leases are not
prerequisites. The same authoring rules apply regardless of how many passes are needed.

## Lifecycle and runtime reuse

Statuses are `queued`, `running`, `awaiting_review`, `completed`, `failed`, and `cancelled`. A job starts queued,
becomes running when admitted, and either completes, fails, is cancelled, or retains a proposed diff awaiting review. A
reviewed proposal completes with an applied or rejected result. Stale proposals are marked as such and require a fresh
attempt before application. Awaiting-review jobs consume no active agent slot.

Results distinguish an answer, no change needed, applied changes, proposed changes, drift findings, incomplete coverage,
and an unresolved question. A scan that found drift can complete successfully: the finding remains unresolved. An
operational failure is distinct from a successful scan with findings. The UI must not equate completed with consistent.
A result needing a user answer records the question and available findings; a later answer starts a linked follow-up job
using current inputs rather than keeping a provider process suspended indefinitely.

Reuse agent model and reasoning configuration, process launch and cleanup, sandboxing, live and persisted logs, timeout,
cancellation, token usage, and global/project resource accounting. No job targets a work item. Neither launch nor
success/failure cleanup may fall back to claim, release, finish, or comment code. An optional work item created later
from a finding is a user action and never owns the job's state.

One knowledge job runs at a time per project in the first version. Other projects remain independent, subject to shared
runtime limits. A background knowledge job must not publish into a workspace while an admitted coding agent is mutating
it. Periodic jobs wait for that activity to finish; a manual read-only scan may inspect a stable prepared copy during
work but labels its result as an in-progress observation. Answers use the same visible queue, and the user can cancel a
long job when another needs to run.

The server supervises work and validates output. The agent does not declare that its changes were applied or that its
process should be recorded as successful merely by submitting a report.

## Settings, scheduling, and triggers

New projects start with recurring knowledge automation disabled. The Initialize action schedules exactly one bounded
initialization and does not enable later recurring model calls. A manual action also works while recurring automation is
paused.

Project knowledge settings expose enabled schedules for drift and reorganization, change-triggered updates,
model/effort, execution timeout and token budget, and application mode (`review` or `automatic`). Defaults inherit
compatible project model/runtime settings; the UI shows effective values. Review is the default application mode.
Timeouts are enforced by the runtime. A reported-token budget stops further work when observed usage reaches its limit,
but an in-flight model turn can exceed it. If the provider supplies no usage, show that token metering is unavailable
and use the time limit; do not advertise an exact token or monetary ceiling. Suggested schedules are daily drift checks
and weekly reorganization checks, enabled only by the user. Cheap weekly inventory reconciliation can accompany enabled
recurring automation without requiring an AI call.

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

## Initialization from existing code

One click queues an initialization job and exposes its progress immediately. Existing Markdown is input, not disposable
scaffolding. If a root already exists, offer update/reorganization instead of overwriting the store as a new
initialization. An existing non-root document retains its content and identity unless an explicit reviewed change
replaces it.

First inventory first-party source, manifests, entry points, modules, public interfaces, schemas, tests, accepted
documentation, and instructions. Exclude generated/vendor content, binaries, caches, and likely secrets without
inserting their contents into prompts. Group manageable reading passes by responsibility and behavior; splitting by
arbitrary lines does not preserve context. Cross-cutting persistence, security, lifecycle, and public-interface concerns
must be considered explicitly.

For each area, retain supported contracts, important rationale, and known gaps using the authoring rules. Form detailed
documents first, broader summaries next, and the root last. Observed behavior must be described as observed where intent
is not established. Existing design/code contradictions remain visible; an undocumented behavior is not automatically a
requirement. Do not invent product purpose or historical rationale from suggestive names.

The result accounts for relevant repository areas as considered, excluded with reason, or not yet analyzed. It need not
assign every line or file an authoritative owner. Scope limits, failed reads, and uncertain contracts remain visible.
Missing coverage prevents a claim of complete initialization, not useful partial reporting.

The Initialize action authorizes application of an additive, validated initial candidate when it has adequate coverage
and no unresolved material contradiction or unsupported consequential claim. Otherwise retain the candidate for review
and show why. It never authorizes overwriting existing accepted design or enabling recurring work. A partially analyzed
candidate can be reviewed and applied as partial knowledge with its gaps stated explicitly.

## Updates and drift

An update reconciles accepted design changes and observed implementation, updates the lowest affected explanations, and
checks their ancestors and relevant dependents. It can add missing explanation or repair a source link. A new
requirement requires an explicit user/task decision; an automatic update cannot infer acceptance merely because code
changed or an item finished.

A drift scan compares statements with relevant code, tests, schemas, and source documentation. Fingerprints select
candidates and avoid redundant model work; they do not establish semantic consistency. Periodic broader inventory review
finds unlinked new areas and accumulated blind spots. A finding names the claim, observed contradiction, supporting
references, likely affected scope, and uncertainty. "Not examined" must not become "no drift."

Known design-first transitions are reported as implementation pending when evidence shows they belong to active work.
Outside that scope, ordinary drift reporting continues. Once work ends, recheck the transition; completion or failure
alone cannot erase it. Reuse an unresolved finding for the same contradiction instead of opening a duplicate on each
scan. Resolve it only with correction evidence or a user explanation, preserving the reason in the job/finding history.

## Reorganization and automatic application

Reorganization reads the affected documents and necessary evidence, identifies a concrete reading or maintenance
problem, and applies the authoring rules. It can split horizontally or vertically, merge duplication, move detail,
improve summaries, and repair links. Size, age, similarity, or a preference for new wording alone do not justify
deletion or repeated rewriting.

In review mode, document-changing jobs produce inspectable proposals. In automatic mode, Dispatch may apply supported
updates within existing accepted intent and reorganizations that preserve meaning. Changes to requirements, disputed
meaning, compatibility promises, security boundaries, or other consequential contracts remain proposals requiring a user
decision. The worker explains why a change is eligible; deterministic validation checks files, scope, and structure but
cannot prove the semantic explanation. Uncertainty falls back to review, not model-confidence thresholds.

A proposal is an ordinary diff with input fingerprints, explanation, findings, and its job/run link. Users can inspect,
edit, apply, or reject it. Editing a proposal requires renewed structural and freshness checks. Before/after content is
retained with the result so an applied change is reviewable; later restoration is another checked edit, not an
unconditional reset of the project.

## Publication, cancellation, and recovery

Serialize publication per target workspace and coordinate with coding-agent admission. Check the complete intended diff
before writing. Use temporary files, atomic single-file replacements, and a small recovery record for multi-file changes
so interruption is visible and repairable. Recheck destination bytes before replacement to detect external edits. An
ordinary editor that ignores Dispatch's lock cannot be promised a perfectly atomic multi-file view; recovery must
preserve conflicting bytes and require resolution instead of deleting them.

After files have been written, a failed index refresh or notification is recorded as applied with a warning and retried.
It must not be reported as though no change occurred. A lost response can be resolved by inspecting the existing job
result; retrying reporting must not apply the diff twice.

Cancellation stops the process and preserves useful drafts, reports, and logs. Cancel before publication to leave target
files untouched. Once publication starts, finish or recover that bounded write before recording the outcome;
cancellation cannot imply that already written files vanished. Server restart marks interrupted analysis for retry and
recovers incomplete publication before admitting another writer. Retry captures current inputs and retains a link to the
earlier attempt.

Deleting the project stops jobs and processes before removing project-owned operational artifacts, while preserving
workspace knowledge. Deleting and recreating the same project name does not inherit old jobs or write permissions.
