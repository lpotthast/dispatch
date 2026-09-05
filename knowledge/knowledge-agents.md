---
id: dispatch.knowledge.agents
refines:
  - dispatch.knowledge
depends_on:
  - dispatch.knowledge.documents
  - dispatch.knowledge.pyramid
---

# Knowledge Agent Interface

Agents read and edit ordinary Markdown, using the Dispatch CLI for efficient navigation, impact mapping, checks, and job
reporting. The interface works in the agent's assigned working copy. It does not require a work item, signed change
operations, or an agent call for each retrieval.

## Launch context and instructions

Dispatch provides the task, immutable project and run identity, registered working directory, effective permissions, and
any knowledge-job identity. The CLI resolves explicit project flags before environment defaults, but the service still
rejects an override outside the run's allowed project or working copy. The CLI is an HTTP relay and never opens the
operational database. Arbitrary client-supplied filesystem paths cannot select another workspace.

The launch input includes the short project root and compact summaries of its immediate children, freshly read from the
assigned working copy. This satisfies initial root reading. Relevant path matches or job scope can supply a small
shortlist of useful documents with reasons; it is advisory, not an exclusive reading list. The whole knowledge base is
never injected by default.

If the root is absent or unreadable, report that fact explicitly. An initialization job can start without a root.
Ordinary agents can inspect source and visible documents while reporting the knowledge gap; absence of initialization is
not a global code-work gate.

A small instruction layer explains navigation, authority, permissions, and reporting. Reusable instructions supply the
authoring rules and role-specific job procedure. Keep their actual text with the normal run input so a result can be
understood later. The first version does not require a separate versioned skill registry, prompt-hash protocol, or
per-query receipts.

## Navigate progressively

Read the root and open relevant or uncertain child summaries until the task's required precision is reached. Read source
evidence when the question needs executable detail. Search when a term, path, or subject is not routed clearly. Expand
when source reveals another relevant component or a contradiction. Absence from retrieved knowledge means uncovered
scope, not proof that no requirement exists.

Reuse reading within a task. Do not rerun the root before every command or reopen unchanged documents just to produce an
audit trail. Refresh affected context when files or relationships change, and extend the route when scope expands. A
handoff can summarize what was read, its working copy, and what remains uncertain without a signed receipt.

## Immediate CLI contract

These are target first-version commands, not a statement that today's binary supports them:

```text
dispatch knowledge root --json
dispatch knowledge node show <id-or-path> --json
dispatch knowledge search --text "claim recovery" --json
dispatch knowledge impact --json
dispatch knowledge check [<id-or-path>] --json
```

| Operation   | Result                                                                                                                             |
|-------------|------------------------------------------------------------------------------------------------------------------------------------|
| `root`      | Root document, immediate child summaries, working-copy identity, and relevant diagnostics.                                         |
| `node show` | Current Markdown, authored metadata, parents, children, other relations, path, and content fingerprint.                            |
| `search`    | Ranked lexical matches with short snippets, match reasons, paths/IDs, and pagination.                                              |
| `impact`    | Changed source/document paths, affected documents and ancestors, relevant dependents, reasons for inclusion, and unlinked changes. |
| `check`     | Structural diagnostics for metadata, identity, hierarchy, references, source selectors, and size/routing signals.                  |

Each response identifies its working copy and index generation. Document responses identify exact current content. Lists
have a documented finite default limit and an explicit continuation cursor when more results exist; content must not
silently disappear to meet a budget. Large document responses indicate any truncation and offer section or offset
continuation. Initial routing summaries remain compact.

Navigation and lexical search are deterministic and do not start models. `check` cannot detect all semantic
contradictions. `impact` suggests a scope and never declares that unlinked source changes have no documentation
consequences. Invalid metadata stays visible as a local diagnostic rather than being replaced with cached old content.

In a launched run, `impact` compares the assigned working copy with its captured start baseline. Explicit repeated
`--path` options or `--base <git-base>` override that implicit baseline and are mutually exclusive. Supplied paths are
labeled as caller-selected scope rather than independently detected changes. Without a run baseline or explicit input,
the command requests a baseline rather than inventing one. Source inventory includes visible additions and deletions,
including untracked first-party files. Document changes also cause impact expansion. `check` can run for the project or
a selected document.

Reading, searching, and checking work without a launched run when `--project` selects a registered project. A document
path is relative to that project's knowledge directory. An ID/path ambiguity is reported rather than selecting an
arbitrary file. `node show --id <id>` and `node show --path <path>` explicitly disambiguate and are alternatives to its
positional argument. Outside a launched run, the registered project working copy is used; the CLI does not silently
treat its current directory as another registered worktree.

## Working-copy correctness

Every run is registered with one actual working copy: the project checkout, a coding worktree, or a prepared
knowledge-job copy. The service binds requests to that copy; it does not answer a worktree agent from the main
checkout's index. A normal file edit becomes visible to the next relevant CLI read without a signing or import step.
Reads compare current bytes and refresh stale projections.

A content fingerprint detects staleness; it is not a signature or an assertion that prose is true. An index generation
groups derived results, not a new canonical history. Changing an unrelated document does not force the agent to reread
the entire project.

## Permissions and editing

| Role                                    | Reads                                                           | Writes                                                   |
|-----------------------------------------|-----------------------------------------------------------------|----------------------------------------------------------|
| Ordinary coding agent                   | Permitted project source and knowledge in its working copy.     | Source and knowledge within the task's authorized scope. |
| Read-only review agent                  | Permitted project source and knowledge.                         | A report or proposed patch in its output area.           |
| Initialize, update, or reorganize agent | Filtered source evidence and knowledge in prepared inputs.      | Knowledge drafts in its prepared working copy.           |
| Drift-scan agent                        | Filtered source evidence and knowledge.                         | Findings in its output area.                             |
| Answer agent                            | Visible knowledge and permitted source needed for the question. | An answer and citations in its output area.              |

Knowledge workers cannot claim, finish, release, or comment on work items, change automation settings, or expand their
own access. Their environment contains no claimed-item context. The runtime and service enforce scope; instructions
alone do not protect ignored content. Prepared source inputs exclude ignored content and keep source read-only while
allowing the knowledge draft to be edited.

Authorized writers use ordinary file-editing tools for Markdown and frontmatter. They do not update sidecars or derived
indexes. Coding agents edit documentation alongside code in the same working copy, including local API comments and
README examples when needed. Background knowledge jobs cannot fix code as a side effect of documenting it.

Background drafts are applied by Dispatch after validation, freshness checks, and the selected application policy. A
human review mode yields a diff, not a disabled agent editor. Direct edits made by the user or their coding agent remain
normal project edits and need no separate acceptance ceremony.

## Ordinary coding workflow

Read relevant knowledge, inspect source, and record intended contract changes before or alongside implementation. After
editing, use impact mapping, update the lowest affected documents, check all affected broader summaries and relevant
dependents, and run structural checks. Preserve unchanged prose. Report meaningful changes, verification, and unresolved
disagreements through the task's ordinary reporting path.

Work-item completion still follows ordinary item rules. Knowledge quality is part of completing the task, but the new
system does not require signed knowledge reviews or per-query receipts as an additional item transition protocol. Later
checks can detect gaps; a successful item transition alone never resolves a drift finding.

## Knowledge-job reporting

```text
dispatch knowledge job progress --body "Checking persistence contracts."
dispatch knowledge job report --file result.json
```

These commands use the current job/run context and reject missing or mismatched context. They do not select a job by
guessing a work item. Progress is a short replaceable status plus the normal run log. The report contains a summary,
reviewed scope and omissions, findings or answer, supporting document/source references, the explanation of proposed
changes, and unresolved questions. Answer citations identify the actual content read, including whether a conclusion is
inferred.

Dispatch derives changed files, diffs, run IDs, timings, and usage from execution. Agents do not reconstruct those
records or supply a successful application status. Report submission stores a result; the service determines job
completion after the process ends and required checks finish. Repeated identical submissions from the same run are
harmless; replacing a terminal accepted result is rejected.

Agents never recursively launch `knowledge ask` to interpret documents for an existing knowledge job. A user's free
question starts an answer job through the user surface. If source access is unavailable or the answer exceeds its
budget, the agent states the limit and provides supported findings rather than inventing completeness.
