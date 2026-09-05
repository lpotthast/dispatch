---
id: dispatch.knowledge.pyramid
refines:
  - dispatch.knowledge
---

# Knowledge Pyramid and Authoring

Good knowledge preserves the information a reader needs to make a correct decision, at the level where that information
is useful. This document defines what to retain, how to summarize it, and how humans and agents update the pyramid
without changing its meaning accidentally.

## Layers and useful summaries

The project root explains purpose, actors, global boundaries, and essential invariants. Broader documents explain a
domain's responsibilities, interactions, and consequential decisions. Detailed documents explain behavior, interfaces,
data constraints, exceptions, and failure cases. Source code, tests, schemas, configuration, commands, and local
documentation supply the remaining detail.

A refinement hop changes abstraction, not just filenames. A parent must contain a useful explanation as well as routes.
A child supplies detail that would distract or mislead a reader at the parent. There is no fixed depth, minimum child
count, required growth ratio, or stored layer number. Small projects may need only a few documents; large projects can
add levels where they improve navigation.

Keep the root short enough to read at task startup. As initial review signals, aim for roughly 500 words at the root,
one to three sentences in each routing summary, and inspect documents above roughly 1,500 words or parents with more
than about twelve immediate children. These are prompts for judgment, not validity limits or automatic split commands. A
coherent detailed reference can remain long when readers can locate the needed section.

## Evidence, intent, and disagreement

Accepted requirements and decisions constrain the implementation. Code demonstrates what exists; tests demonstrate the
cases they exercise. Neither proves that observed behavior is intended. Comments and README files may be stale. A source
link is a route to evidence, not evidence that the agent actually read it.

Authors distinguish these situations in ordinary prose:

- accepted intent, including the reason for an important requirement;
- observed implementation, with the files or tests supporting the observation;
- an inference, with its reasoning and uncertainty;
- a known disagreement or missing evidence;
- an accepted future change whose implementation is still pending.

Do not add machine-managed authority fields to every claim. Make material uncertainty explicit where readers encounter
it. Historical material belongs in a clearly labeled history section or separate reference document; active summaries
must not present obsolete behavior as current.

For a new behavior change, record the intended contract and the implementation gap together. For example: "The accepted
target is a ten-minute claim timeout; the current implementation still uses thirty minutes. This change is being
implemented in the linked work." When implementation catches up, remove the transition wording after checking evidence.
An unfinished transition must not disappear because a job timed out or an item was closed.

## What to retain

Retain an explanation somewhere when forgetting it could cause a plausible consequential mistake. This includes accepted
requirements, public behavior, compatibility, security and privacy boundaries, data integrity, destructive operations,
ownership, concurrency, cancellation, recovery, and important failure semantics. Preserve non-obvious rationale and
expensive or hard-to-reconstruct decisions.

For other information ask: will it guide future work, remain useful, avoid expensive rediscovery, or route readers to
important evidence? If not, it can remain in code or local documentation. There is no numerical importance score and no
per-claim evidence ledger.

Do not transcribe every function, test, field, or line of control flow. Link to the relevant source and explain only the
contract or rationale that needs prose. Low query frequency, age, length, and textual similarity do not establish that
information is obsolete.

## Placement and information loss

Place a statement at the broadest scope where it is correct, actionable, and understandable. Cross-cutting
responsibilities and consequences belong higher. Local syntax, exhaustive cases, helper names, examples, and changeable
mechanics normally belong lower.

Each summary preserves four things: what is true, why it matters when the reason is consequential, the boundary or
exception needed for a safe decision, and the route to detail. Examples and repeated evidence can be omitted.
Qualifications that change a reader's decision cannot be omitted.

For example, a workflow summary can say: "An active claim gives one agent ownership; expired claims can be recovered by
the server." The detailed document explains who may finish or release, expiry handling, version checks, and races. A
summary saying "only the claimant can change ownership" would be wrong if it concealed stale-claim recovery. It is
acceptable to omit the exact timeout from the project root and route to the workflow contract.

One document owns the full explanation of a durable fact. Other documents may summarize its consequence and link to the
owner. This permits useful repetition at different abstraction levels without creating independent specifications that
must be reconciled sentence by sentence.

## Code comments and README files

| Location                            | Primary responsibility                                                                                                           |
|-------------------------------------|----------------------------------------------------------------------------------------------------------------------------------|
| Project knowledge                   | Project and subsystem intent, architecture, cross-component behavior, important invariants and rationale, and routes to detail.  |
| README files                        | Orientation, installation, quick start, supported usage, and routes into knowledge or API documentation.                         |
| Module and API documentation        | Local responsibility, interface contracts, preconditions, results, errors, side effects, concurrency, and examples callers need. |
| Inline comments                     | Non-obvious reasoning, assumptions, constraints, and explanations of surprising implementation choices.                          |
| Code, tests, schemas, configuration | Exact implementation and concrete executable or declarative evidence.                                                            |

Document public and internal APIs, modules, and routines at a level appropriate to their responsibilities and contracts.
A simple routine may need only a brief explanation; callers of a complex API need its preconditions, failures, effects,
and examples. Comments explain contracts and reasoning rather than paraphrasing each statement. Knowledge does not
replace code documentation.

Choose one full owning explanation and summarize or link from other locations. A detailed API contract can live in API
documentation; knowledge explains its architectural consequence and links to it. A README should link to detailed design
instead of maintaining a second architecture chapter. Important intent found only in a comment may deserve a broader
summary, but the local comment remains where future source readers need it.

When an API changes, the coding agent updates its local documentation and examples along with relevant knowledge. A
knowledge-only job reports necessary source-comment or README changes outside its write scope; it does not silently edit
the implementation. Conflicting prose is resolved using evidence and accepted intent, not a blanket rule that one file
extension always wins.

## Updating from the bottom upward

Before changing a contract, read the root and relevant ancestors to understand constraints. Then inspect the affected
detailed documents and source. For an intentional change, draft the new owning contract before or alongside the
implementation.

After the change:

1. Update the lowest affected explanations and their source references.
2. Check every refinement parent of those documents, continuing to the project root and including shared parents.
3. Check dependents whose contracts may change and inspect relevant contextual links. Do not recursively expand every
   related document.
4. Rewrite only explanations whose meaning, completeness, or routing needs to change.
5. Check new, moved, and deleted source files that have no known document owner. Existing links cannot discover
   everything.
6. Check references and record remaining gaps or contradictions in the task or job result.

An unchanged parent is a valid outcome after review. A refactoring that preserves behavior may need only a source-link
update. A broad contract change can require looking downward into multiple implementations even when only a parent
document was edited. Impact mapping proposes scope; the agent expands it when evidence requires.

## Splits, merges, and removals

A horizontal split separates independent subjects at a similar level. For example, an oversized persistence document may
split into backup/recovery and schema migration documents, keeping a brief persistence overview.

A vertical split separates abstraction levels. A workflow document overloaded with request fields and race cases can
retain the workflow explanation and link to a narrower claim protocol document. The parent keeps meaningful constraints
and exceptions; it does not become only a directory listing.

Choose a split because a reader can use a smaller coherent part or because mixed abstraction makes the text hard to
follow. Preserve the continuing concept's identity, update incoming routes, and verify that each new document has a
useful summary. Do not split at arbitrary token or line boundaries.

Merge documents when their scopes overlap enough that readers repeatedly need both. Preserve their unique supported
content, choose a clear surviving owner, and repair incoming references. Remove a statement only when it is demonstrably
false, superseded, duplicated at its owner, or outside the intended scope. Explain consequential removals in the diff
summary. Uncertainty about a mandatory invariant is a finding, not permission to delete it.

## Stable repeated maintenance

A maintenance pass starts by identifying a concrete problem: changed behavior, stale evidence, missing scope,
contradiction, poor routing, duplication, or an overloaded explanation. It preserves satisfactory wording and structure.
Stylistic preference alone is not a reason for repeated automated rewrites.

Before publishing a rewrite, compare the old and new explanations. Check that requirements, exceptions, reasons,
ownership, and routes still have a home. A split followed by a later merge needs new evidence that organization
improved; an agent must not oscillate between equally valid arrangements on successive runs. Retain unresolved disputes
visibly rather than manufacturing agreement.

A job's short change explanation states what was added, corrected, moved, generalized, or removed and why. It need not
classify every sentence. Success is useful reading and preserved decision value, not the smallest word count.

## How we evaluate these rules

Use small representative projects and real task questions. Compare whether readers find the right owner, whether a
summary permits a correct decision, whether bootstrap exposes gaps, and whether repeated maintenance preserves meaning.
Include shared parents, contradictory comments and code, exceptions to broad rules, and horizontal and vertical splits.
Run the same unchanged example twice and expect the second pass to justify any further change. Semantic quality needs AI
or human review; structural validators cannot prove it.
