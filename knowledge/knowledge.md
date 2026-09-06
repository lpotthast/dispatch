---
id: dispatch.knowledge
refines:
  - dispatch.architecture
---

# Knowledge System

Dispatch helps a project maintain a readable, connected explanation of its intent and design. Humans and agents write
ordinary Markdown. Agents use a short overview to find the detail relevant to their task. Background knowledge jobs
initialize, update, inspect, and reorganize that explanation as the project changes.

## Purpose and authority

Knowledge is the project's accepted understanding and intent. It states what the project does, the constraints it must
respect, and why consequential decisions were made. Implementation supplies the remaining executable detail and must
satisfy those statements. Tests, schemas, configuration, code comments, and README files provide further evidence and
explanation.

When implementation and accepted design disagree, there is a contradiction to resolve. An agent must not silently
rewrite a requirement to match a bug. An intentional design change can update knowledge before implementation;
knowledge states that accepted contract in present tense. Implementation gaps, progress, and unresolved contradictions
are recorded in task reports or findings outside the knowledge documents. Evidence and rationale support
the design without turning its owning explanation into a record of the implementation effort.

Knowledge is living documentation. It is neither an append-only activity log nor an exhaustive prose translation of
code. Its value is helping a reader make a correct decision with the least necessary reading.

## Shape and ownership

One short project root routes to broader subjects, which route to increasingly specific documents and finally to source
evidence. Every parent remains useful without opening its children. A detailed document may refine several parents; the
refinement structure has no cycles. Other relations connect dependencies and related subjects across the hierarchy.

Each durable fact has one owning explanation. Broader documents summarize the consequences needed at their scope and
link to detail. A changed detail requires checking affected broader summaries, including shared parents; it does not
require rewriting a summary that remains correct.

## First-version boundaries

- Markdown and small YAML frontmatter are authoritative. Documents remain readable outside Dispatch and ordinary file
  edits are supported.
- Dispatch derives its graph, search index, backlinks, and diagnostics from visible files. These indexes can be rebuilt.
  Job history and retained run artifacts are separate durable operational data.
- Immediate navigation, search, impact mapping, and structural checks use ordinary software. AI performs semantic
  reading, drafting, drift analysis, and reorganization.
- Coding agents edit knowledge alongside source in their assigned working copy. Background knowledge agents edit
  prepared copies and publish checked differences according to the job's application setting.
- Knowledge jobs are independent of work items and item automation. They reuse agent execution, logs, cancellation, and
  resource limits. Every knowledge agent run is visible in the UI.
- `.gitignore` and `.dispatchignore` determine participation. Excluded files remain untouched and unavailable to
  knowledge retrieval and knowledge-job inputs.
- Automation must preserve intent, expose uncertainty, avoid stale overwrites, and produce inspectable results. A
  successful process exit is insufficient evidence of a successful knowledge job.

The first version does not require cryptographic signatures, signer trust, authoritative sidecars, a custom
version-control system, per-query receipts, per-claim scores or lineage records, a separate distributed execution
framework, embeddings, or cross-project graph federation. Git remains optional; knowledge jobs do not stage, commit,
reset, or push the project.

## Read progressively

| Document                                              | Owns                                                                                                          |
|-------------------------------------------------------|---------------------------------------------------------------------------------------------------------------|
| [Documents and relationships](knowledge-documents.md) | File layout, frontmatter, root selection, relation semantics, ignore rules, indexing, and document lifecycle. |
| [Pyramid and authoring](knowledge-pyramid.md)         | Retention, placement, summarization, source documentation, bottom-up updates, and stable reorganization.      |
| [Agent interface](knowledge-agents.md)                | Launch context, working-copy reads, CLI operations, permissions, editing, and job reporting.                  |
| [Automation](knowledge-automation.md)                 | Initialization, updates, drift, reorganization, scheduling, publication, and recovery.                        |
| [User interface](knowledge-ui.md)                     | User actions, document and graph views, answers, proposals, findings, and visible jobs and runs.              |
| [Integrity and preservation](knowledge-integrity.md)  | Validation, content preservation, compatibility guarantees, and observable acceptance criteria.               |

Detailed rules live with the document that owns them. The general architecture, API, CLI, data-model, workflow, and UI
documents link into this branch rather than maintaining a second specification of knowledge behavior.
