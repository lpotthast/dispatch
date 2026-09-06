---
id: dispatch.knowledge.initialization
refines:
  - dispatch.knowledge.automation
  - dispatch.knowledge.pyramid
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.agents
---

# Iterative Knowledge Discovery

Initialization turns project evidence into a compact, navigable explanation of important behavior, boundaries, and
decisions. Continue discovery extends that understanding from current evidence and retained investigation scope,
including when the project already has a root. Each user request authorizes one bounded job, not recurring automation.

## Admission and bounded execution

One unresolved initialization job exists per project, including a pending proposal. Persisted admission and stable
request idempotency prevent duplicates across tabs, response loss, reloads, and restarts. Reusing a request ID for
different settings is rejected. Pending proposals are applied or rejected before another job starts.

Initialization uses sequential bounded discovery, synthesis, reader, and reviewer passes with a shared agent-run record
for each pass and durable checkpoints. One hour of active agent work is the default budget shared by all passes and
automatic recovery. The final quarter is reserved for synthesis and validation. Continue discovery and Retry authorize
another bounded linked job using current evidence and retained investigation scope. Existing Markdown remains input with
preserved content and identities; continuation never rebuilds accepted knowledge from scratch.

Request identity, predecessor links, active-work accounting, pass/run references, and recovery attempts remain durable
across reloads and restarts.

[Knowledge automation](knowledge-automation.md) owns the shared runtime, scheduling, publication, cancellation, and
recovery contract. [Knowledge UI](knowledge-ui.md#jobs-and-agent-run-visibility) owns initialization controls and
progress presentation. The [API](api.md#iterative-knowledge-jobs) and [CLI](cli.md#bounded-discovery-commands) define
request syntax.

## Discovery and evidence

The service inventories eligible source, tests, manifests, configuration, instructions, and documentation under the
[document participation rules](knowledge-documents.md#ignore-rules). Inventory is orientation, not analysis. The agent
reads orientation material and entry points, then records a compact discovery map in operational job artifacts. Areas
follow behavior and responsibility rather than folders. Each records concrete questions, likely evidence, applicable
aspects, existing knowledge owners, and remaining investigation. The map covers purpose, actors, observable
capabilities, subsystem boundaries, cross-subsystem workflows, and applicable concerns such as persistence, security,
ownership, concurrency, compatibility, cancellation, and recovery.

Continuation reconciles this map with current source and knowledge. Useful discoveries survive; new responsibilities and
aspects enter remaining scope; changed or excluded evidence invalidates affected assessments. The investigation order
is:

1. Consequential contradictions and changed evidence affecting existing knowledge.
2. Unexamined public boundaries and cross-cutting responsibilities.
3. Incomplete workflows and missing exceptions in discovered areas.
4. Remaining source regions and less consequential details.

Major project areas receive attention before the budget is exhausted in one subsystem. Each investigation asks what
behavior or constraint the evidence establishes, who owns it and depends on it, what initiates it, what observable
outcomes follow, and which conditions, exceptions, failures, or recovery paths change the answer. Entry points are
traced through implementation and supporting tests or documentation. Names, imports, search hits, and comments alone do
not establish a contract. Supplied documentation sites and relevant documentation links are read when they inform the
area. Missing evidence remains an explicit gap.

## Synthesis and evaluation

Synthesis applies [Pyramid and authoring](knowledge-pyramid.md) to retained discoveries: accepted intent constrains the
descriptive baseline, each explanation has one owner, and detail precedes broader summaries. Candidates include the
necessary ancestor changes so continuation leaves new detail connected to the overview. Authored links follow
[relation semantics](knowledge-documents.md#relation-semantics); imports and similar words alone do not justify them.

[Source coverage and reading evaluation](knowledge-evaluation.md) owns aspect assessments, representative questions,
independent reader/reviewer passes, and compression measures. A justified source-only disposition is a successful
analysis outcome. Inventory, supplied ranges, assessments, questions, checkpoints, findings, drafts, and publication
records remain durable job artifacts outside canonical knowledge.

## Candidate acceptance

Initialize authorizes additive, validated initial candidates with adequate coverage and no unresolved material
contradiction or unsupported consequential claim. Changes to accepted documents follow the
[application mode](knowledge-automation.md#reorganization-and-automatic-application). Partial candidates always require
explicit review, with gaps recorded in the job result.
