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

Markdown and frontmatter own document content and relationships. Derived indexes, cached views, and compatibility
metadata cannot override them. Refresh reflects external edits and current ignore rules. Ignored files are excluded from
active reading, graph results, search, and model inputs without being modified or deleted.

Job history, logs, findings, proposals, and publication results are durable operational records. Rebuilding the index
does not reconstruct or erase them. History identifies the inputs and outcomes of its run rather than presenting past
results as current facts. Historical records cannot authorize new writes or reactivate completed work.

Project deletion removes project-owned operational data and runtime artifacts, while preserving workspace files. The
immutable project ID prevents history or queued work from attaching to a new project with the same name.

## Structural validity and local failure

Visible Markdown stays readable by path when metadata is missing or invalid. Duplicate identities are ambiguous;
Dispatch never selects an arbitrary winner. Missing relation targets and cycles produce local diagnostics, while valid
parts of the graph remain navigable. Source and link validation respect participation and filesystem boundaries.

Document discovery reads at most 4 MiB per file. Larger files receive a local size diagnostic. This resource limit does
not redefine semantic quality: length, age, or low query frequency alone cannot justify deleting information.

Reading, graph navigation, lexical search, and structural checks require no AI provider. A model failure affects its job
and leaves ordinary document operations available. Structural validity does not prove semantic consistency.

## Checked changes

Editor saves compare the opened content fingerprint with current content before replacing the file. Concurrent edits
produce a conflict; the user's draft remains available for comparison and deliberate reload. Markdown and unknown
frontmatter fields survive unchanged unless the user edits them. File access cannot escape the configured knowledge
directory through absolute paths, traversal, symlinks, or ignore-rule changes.

Background publication follows the input freshness, write-scope, interruption, and recovery rules in
[Knowledge automation](knowledge-automation.md). Process success alone cannot establish application success. Retries
never apply a result twice or overwrite unrelated edits. Applied changes remain identified as applied even if a later
notification or index refresh fails.

## Compatibility and preservation

A change in storage representation preserves useful document content, supported relationships, unknown metadata, and
meaningful historical records. Identity conversion is explicit when a source representation is ambiguous. Existing
`README.md` content is never overwritten merely to establish a root. Unsupported relationships are surfaced for a user
decision instead of silently discarded. Backups and archived metadata remain outside active discovery.

There is one active knowledge authority and one scheduler per project. Compatibility records do not require a second
active writer or public mutation protocol. Rules for historical reserved knowledge work remain inactive, and interrupted
legacy runs cannot resume as ordinary item work. Recorded completed runs remain readable. User-configured label data is
preserved independently of built-in routing behavior.

Neither initialization nor format conversion enables recurring model calls. Schedule and application policy remain
explicit user choices. A rollback cannot silently enable retired automation or overwrite independent edits. Schema
migrations preserve frozen identifiers, ordering, and meaningful history; removal of obsolete records requires an
explicit preservation policy.

## Observable guarantees

| Situation                                       | Required outcome                                                                                                          |
|-------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| Existing project with substantial code          | Initialize schedules visible, bounded work; detail precedes summaries; existing content and coverage gaps remain visible. |
| Plain Markdown added in an editor               | Refresh exposes it immediately; missing identity or refinement is a local organization diagnostic.                        |
| Implementation contradicts an accepted contract | A finding records the evidence; the owning knowledge document retains the intended contract.                              |
| Accepted design precedes implementation         | Knowledge describes the accepted behavior; task reports and findings outside knowledge retain the implementation gap.     |
| A detail has several parents                    | Updates inspect every affected parent and preserve summaries that remain correct.                                         |
| Reorganization repeats on unchanged inputs      | No gratuitous rewriting, oscillating splits, or progressive loss of exceptions occurs.                                    |
| An ignore rule excludes an open document        | Active views stop exposing its saved content; file bytes remain intact.                                                   |
| A coding agent uses a worktree                  | Retrieval and impact bind to that worktree and reflect its current edits.                                                 |
| A user edits while an editor or job is open     | A stale write is rejected, with the draft retained for comparison.                                                        |
| A subsystem has no source links                 | Inventory review reports uncovered scope instead of declaring a complete clean scan.                                      |
| An AI provider is unavailable                   | Documents and graph remain usable; affected jobs expose an actionable failure.                                            |
| A knowledge job has no work item                | Launch, logs, results, cancellation, retry, and recovery work without synthetic items or claim operations.                |
| The server restarts during publication          | Recovery exposes incomplete application and preserves a single inspectable outcome.                                       |

Semantic quality is evaluated through representative questions, shared-parent documents, conflicting evidence, important
exceptions, and repeated maintenance. Useful reading and preserved decision value determine success; structural checks
alone cannot establish that meaning survived.
