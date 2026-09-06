---
id: dispatch.knowledge.documents
refines:
  - dispatch.knowledge
---

# Knowledge Documents and Relationships

A knowledge document is an ordinary UTF-8 Markdown file under the project's configured knowledge directory, normally
`knowledge/`. Its content and authored relationships travel together. Dispatch reads those files to derive navigation,
search, and diagnostics; no separate index owns their meaning.

## Location and discovery

The directory is relative to the configured project workspace. The first version supports one directory per project and
one project root, always the directory's `README.md`. Location is a project setting; relocation is a user-requested file
move with destination, collision, exclusion, and reference checks, never an implicit response to a missing folder. It
preserves document IDs and unrelated files and updates the setting only after the move succeeds. Background knowledge
workers cannot relocate the directory. [Knowledge UI](knowledge-ui.md#settings-and-degraded-operation) owns the preview;
file writes use the [publication and recovery rules](knowledge-automation.md#publication-cancellation-and-recovery).
Copying the complete directory preserves its text and relationships. Deleting a project from Dispatch preserves the
directory.

Every non-ignored `.md` file beneath the directory is discoverable. A file without Dispatch metadata remains readable
and searchable by path. It is shown as unorganized until it has a unique identity and a refinement route to the root.
Absence of a root is an uninitialized knowledge structure, not permission to discard existing documents or to prevent
ordinary code work.

The root's content follows [Pyramid and authoring](knowledge-pyramid.md#layers-and-useful-summaries). Directory nesting
is useful organization for humans; it does not define refinement. Relative Markdown links work in ordinary editors and
repository viewers. Document discovery reads at most 4 MiB per file; larger files receive a local size diagnostic.

## Minimal frontmatter

An organized document uses the following fields:

```yaml
---
id: claim-lifecycle
refines:
  - workflows
depends_on:
  - permissions
related_to:
  - agent-lifecycle
sources:
  - dispatch-server/src/backend/work_items/**
---
```

| Field        | Meaning                                                                                          |
|--------------|--------------------------------------------------------------------------------------------------|
| `id`         | A project-local, stable concept identifier. Required for organized documents.                    |
| `refines`    | IDs of immediate broader documents. Required and non-empty except at the root.                   |
| `depends_on` | IDs whose contracts this document relies on. Optional.                                           |
| `related_to` | IDs useful as contextual reading without implying hierarchy or dependency. Optional.             |
| `sources`    | Project-relative file paths or globs identifying relevant implementation and evidence. Optional. |

IDs use lowercase ASCII letters, digits, dots, underscores, and hyphens, start with a letter, and remain stable when a
concept is renamed or moved. A document identity has no global significance outside the project. References in
frontmatter use IDs, not filenames or line numbers. Empty optional lists may be omitted. The root has an ID and no
refinement parent.

The title comes from the first level-one heading. The opening paragraph after that heading is a short routing summary,
normally one to three sentences. It states the document's subject and usefulness. Missing headings or summaries produce
actionable diagnostics; path-based reading still works. Title, summary, children, hashes, token counts, timestamps, and
job history are not duplicated in frontmatter.

Dispatch preserves unrelated frontmatter fields. Its editor changes only fields the user edited and preserves other
content and formatting where possible. The parser rejects duplicate YAML keys and malformed Dispatch fields; it does not
interpret custom tags or execute anything. Metadata has no ability to grant permissions, change instructions, or expand
filesystem access.

## Relation semantics

`refines` points from detail to a broader explanation. Its reverse is a derived child list. One child may support
multiple parents. Every organized non-root document must have a path to the root. Refinement cycles, self-links,
duplicate IDs, and missing targets are errors. Redundant shortcuts are a diagnostic for review, not a reason to reject
otherwise usable documents.

`depends_on` is directed from the dependent document to the contract it relies on. Impact mapping consults dependents
when their dependency changes. Dependencies can be cyclic because mutually interacting subjects are possible; dependency
traversal must track visited documents.

`related_to` is symmetric contextual navigation. Authors declare it once at either endpoint; the derived graph displays
one relation and both backlink directions. Exact duplicate declarations are diagnosed and deduplicated for display.
Contextual relations do not imply transitive impact or required reading.

Ordinary Markdown links are readable citations and routes, not implicit refinement edges. Their destinations are indexed
as backlinks and checked when local files move. Heading links use ordinary Markdown anchors, or explicit HTML anchors
where a stable human-facing citation is needed. There are no separately registered, hashed section records in the first
version.

`sources` link to evidence, not to proof of correctness or exclusive source ownership. Several documents can refer to a
shared source file. Globs support `/`, `*`, `?`, and `**` relative to the project root; absolute paths, traversal
outside the project, and symlink traversal are not allowed. Fine-grained symbols may be named in prose. Unmatched
selectors are visible diagnostics. A change to a source selector affects future impact scope even when prose is
unchanged.

## Ignore rules

Knowledge participation respects `.gitignore` and `.dispatchignore` files from the configured project root down to the
file's directory. Rules outside the project, Git's global excludes, and `.git/info/exclude` do not participate. The same
behavior applies in Git and non-Git projects.

Both families use Git-compatible pattern semantics: patterns are relative to their control file, later matching rules
override earlier ones within a file, deeper controls override broader controls within a family, and `!` negates an
exclusion within that family. An excluded parent directory must be re-included before any descendant can be re-included;
controls inside a pruned directory are never read. Evaluate the two families independently and exclude a path if either
excludes it. A negation in one family cannot override exclusion in the other.

Patterns apply regardless of whether Git tracks a file. This is intentionally a file-participation rule rather than
Git's tracked/untracked behavior. [Git's pattern documentation](https://git-scm.com/docs/gitignore) defines the syntax.
Control files and reserved Dispatch runtime directories are never knowledge documents. Traversal does not follow
symlinks.

Apply exclusions before opening document bodies, indexing, source-selector expansion, creating agent input, or reading
through a knowledge API. Knowledge-source inventory uses the same project-root controls and also excludes generated
outputs, dependencies, binaries, and likely secret-bearing content by default. Explicitly expanding that inventory
requires a user-selected scope that still cannot override ignore exclusions. Ordinary coding-agent filesystem
permissions are a separate concern; ignore files are not a universal sandbox for all project tools.

When an indexed document becomes ignored, stop serving its body and derived content, remove it from active graph/search
results, and leave its bytes untouched. References from visible documents receive unavailable-target diagnostics without
opening the ignored target. Do not invalidate the whole knowledge base or delete unique statements from visible
summaries merely because their evidence became unavailable. Removing an ignore rule makes the file discoverable again
without an import ceremony.

In-flight jobs recheck exclusions before reading additional input and before publishing. A changed exclusion affecting
their inputs makes them stale. Discard future-use caches or drafts containing newly excluded material and rerun with
permitted inputs. Exclusion cannot undo a past model request or erase text already copied into other documents or
historical logs.

Unreadable controls or traversal errors must not silently broaden access. Stop discovery in the affected scope and
report the problem. Independent valid scopes remain accessible.
[Knowledge UI](knowledge-ui.md#settings-and-degraded-operation) owns exclusion explanations and degraded-operation
messages.

## Editing and document lifecycle

People and authorized coding agents can create and edit files with normal tools. There is no registration command,
signing requirement, or reconciliation ceremony. New files become discoverable on the next refresh. A missing ID or
parent produces an organization task, not fabricated metadata or an unreadable file.

Dispatch editor saves compare the opened content fingerprint with current content before replacing the file; concurrent
edits reject the write. Markdown and unknown frontmatter fields survive unchanged unless edited. File access cannot
escape the configured knowledge directory through absolute paths, traversal, symlinks, or ignore-rule changes.
[Knowledge UI](knowledge-ui.md#document-and-graph-workspace) owns draft retention, conflict comparison, and navigation
guards.

A rename or move preserves the ID. Dispatch-assisted moves update incoming Markdown links and affected relative links in
the same reviewed change. Ordinary external moves are rediscovered by ID; unresolved path links are diagnosed. Changing
an ID is an explicit reference migration, never a side effect of changing the heading.

Splits keep the existing ID for the concept that continues and give new concepts new IDs. Merges choose one surviving
identity and migrate inbound references. [Authoring rules](knowledge-pyramid.md#splits-merges-and-removals) determine
when splits, merges, and deletion are justified. A short, ordinary redirect document can temporarily preserve a useful
old route; it is not mandatory permanent history.

## Indexing and local failure

The derived index contains current visible paths, IDs, titles, opening summaries, relationships, backlinks, source
selectors, content fingerprints, and searchable text. It is scoped to one working copy. Filesystem events prompt
refresh, but a missed event cannot make stale content authoritative: document reads compare current bytes, and impact
and validation reconcile the relevant inventory.

Malformed frontmatter leaves the raw file readable by path with a diagnostic. Duplicate IDs do not select an arbitrary
winner: both files remain available by path and ID-based lookup reports ambiguity. Invalid relations are displayed as
diagnostics and excluded from valid hierarchy traversal. Cycles are reported with their path; graph exploration remains
bounded. Previously cached valid metadata must not hide a current error.

Structural checks establish parseability, identity uniqueness, reachability, and link health. They cannot establish
semantic truth. Broken external web links are best-effort diagnostics, never a requirement for network access during
ordinary reading. The graph and prose remain useful while a localized problem is being repaired.
