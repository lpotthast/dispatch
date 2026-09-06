---
id: dispatch.knowledge.integrity
refines:
  - dispatch.knowledge
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.automation
---

# Knowledge Integrity and Preservation

Knowledge remains readable and editable through ordinary files. Dispatch protects those files, reports local defects,
and preserves operational history independently of its derived indexes. These guarantees apply to document editing,
automatic publication, project lifecycle operations, and changes in supported storage representation.

## File and history ownership

Markdown and frontmatter own document content and relationships under
[Documents and relationships](knowledge-documents.md). Derived indexes, cached views, and compatibility metadata cannot
override them or modify excluded files.

Job history, logs, findings, proposals, and publication results are durable operational records. Rebuilding the index
does not reconstruct or erase them. History identifies the inputs and outcomes of its run rather than presenting past
results as current facts. Historical records cannot authorize new writes or reactivate completed work.

The [project-deletion lifecycle](workflows.md#project-deletion) preserves workspace files while removing operational
state. Historical records remain bound to the immutable project ID, never a replacement with the same name.

## Structural validity and local failure

Document defects remain local under the [indexing and validation contract](knowledge-documents.md#indexing-and-local-failure).
Discovery resource limits do not redefine the [authoring retention rules](knowledge-pyramid.md#what-to-retain).

Reading, graph navigation, lexical search, and structural checks require no AI provider. Their
[degraded UI behavior](knowledge-ui.md#settings-and-degraded-operation) preserves ordinary document access when a model
job fails. Structural validity does not prove semantic consistency.
[Source coverage and reading evaluation](knowledge-evaluation.md) records aspect-specific evidence and checks whether
independent readers can recover consequential behavior and exceptions; document size remains advisory.

## Checked changes

Direct saves follow [document lifecycle preconditions](knowledge-documents.md#editing-and-document-lifecycle);
[the editor](knowledge-ui.md#document-and-graph-workspace) preserves drafts when those checks reject a concurrent write.

Background publication follows the input freshness, write-scope, interruption, and recovery rules in
[Knowledge automation](knowledge-automation.md#publication-cancellation-and-recovery). Its durable outcome distinguishes
process execution from actual application, including interrupted writes and failures after application.

## Compatibility and preservation

A change in storage representation preserves useful document content, supported relationships, unknown metadata, and
meaningful historical records. Identity conversion is explicit when a source representation is ambiguous. Existing
`README.md` content is never overwritten merely to establish a root. Unsupported relationships are surfaced for a user
decision instead of silently discarded. Backups and archived metadata remain outside active discovery.

There is one active knowledge authority and one scheduler per project. Compatibility records do not require a second
active writer or public mutation protocol. Rules for historical reserved knowledge work remain inactive, and interrupted
legacy runs cannot resume as ordinary item work. Recorded completed runs remain readable. User-configured label data is
preserved independently of built-in routing behavior.

Format conversion preserves the explicit schedule and application choices defined by
[Knowledge automation](knowledge-automation.md#settings-scheduling-and-triggers); it never enables recurring model calls.
Compatibility handling cannot silently enable retired automation or overwrite independent edits. Future schema changes
preserve frozen identifiers, ordering, and meaningful history; removal of obsolete records requires an explicit
preservation policy.

## Observable guarantees

| Situation                                       | Required outcome                                                                                                          |
|-------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| Existing project with substantial code          | [Initialization](knowledge-initialization.md) preserves existing content and records coverage gaps in bounded work. |
| Plain Markdown added in an editor               | [Discovery](knowledge-documents.md#location-and-discovery) exposes it; missing identity or refinement remains local. |
| Implementation contradicts an accepted contract | [Drift reporting](knowledge-automation.md#updates-and-drift) records evidence while knowledge retains accepted intent. |
| Accepted design precedes implementation         | [Authoring](knowledge-pyramid.md#evidence-intent-and-disagreement) keeps implementation gaps outside knowledge. |
| A detail has several parents                    | [Bottom-up updates](knowledge-pyramid.md#updating-from-the-bottom-upward) inspect all affected parents. |
| Reorganization repeats on unchanged inputs      | [Stable maintenance](knowledge-pyramid.md#stable-repeated-maintenance) preserves satisfactory wording and meaning. |
| An ignore rule excludes an open document        | [Exclusion handling](knowledge-ui.md#settings-and-degraded-operation) removes active saved content and preserves bytes. |
| A coding agent uses a worktree                  | [Working-copy binding](knowledge-agents.md#working-copy-correctness) makes retrieval reflect that worktree's edits. |
| A user edits while an editor or job is open     | [Editor conflict handling](knowledge-ui.md#document-and-graph-workspace) and [job freshness](knowledge-automation.md#inputs-and-freshness) preserve newer edits and drafts. |
| A subsystem has no source links                 | [Discovery](knowledge-initialization.md#discovery-and-evidence) reports uncovered scope. |
| An AI provider is unavailable                   | [Degraded operation](knowledge-ui.md#settings-and-degraded-operation) preserves document access and explains job failures. |
| A knowledge job has no work item                | [Shared execution](knowledge-automation.md#lifecycle-and-runtime-reuse) operates without item transitions. |
| The server restarts during publication          | [Publication recovery](knowledge-automation.md#publication-cancellation-and-recovery) preserves one inspectable outcome. |

The [reading evaluation contract](knowledge-evaluation.md#representative-reading-tasks) checks preserved decision value
through representative questions; structural checks alone cannot establish that meaning survived.
