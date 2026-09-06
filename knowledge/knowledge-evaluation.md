---
id: dispatch.knowledge.evaluation
refines:
  - dispatch.knowledge.initialization
  - dispatch.knowledge.integrity
depends_on:
  - dispatch.knowledge.agents
---

# Source Coverage and Reading Evaluation

Coverage identifies which source aspects were considered and what remains uncertain. Reading evaluation measures whether
knowledge supports important decisions with less reading. These are operational job artifacts, independent of Markdown
authority and document size.

## Aspect-specific consideration

Each assessment identifies a captured source path and content fingerprint, one or more 1-based inclusive line ranges,
the aspect considered, disposition, relevant document IDs or findings, a short explanation, and its originating job and
run. Dispositions distinguish represented knowledge, justified source-only material, unresolved findings, and further
analysis. Associations are many-to-many: one range can support several aspects and documents; one document can depend on
many ranges. Considering persistence never implies considering recovery.

Aspects are project-specific subjects discovered during investigation. Cross-cutting prompts help identify omissions;
they do not require every conceivable aspect for every line. Source supplied through the scoped listing, search, and
range-reading interface is distinct from considered source. An agent assessment makes a specified aspect considered. A
document, justified source-only disposition, or explicit finding makes it accounted for. Inventory does not count as
analysis. Shell reads are not treated as supplied coverage. Dispatch verifies returned content and report consistency;
it cannot mechanically prove understanding.

Current content is compared with retained evidence. Added and modified hunks become investigation candidates. Deleted
ranges trigger reconsideration of associated explanations. Historical assessments remain available when newer
dispositions supersede the same evidence and aspect; unaffected evidence
is reusable only when its context remains valid. When independence is uncertain, the whole file or aspect is reconsidered.
Newly discovered aspects can require revisiting previously considered ranges. Ignore changes remove content from active
participation and invalidate affected inputs without claiming to erase historical model requests or logs.

Aggregate line consideration means **considered for at least one aspect**; it is not a completeness score.
[Knowledge UI](knowledge-ui.md#coverage-and-reading-results) owns coverage and evaluation presentation.

## Representative reading tasks

Use small representative projects and real task questions to check owner discovery, useful summaries, discovery gaps,
and preservation through maintenance. Include shared parents, contradictory comments and code, exceptions to broad
rules, and horizontal and vertical splits. Run the same unchanged example twice and require justification for any
further change. Semantic quality needs AI or human review; structural validators cannot prove it.

Before drafting an area, derive representative questions and expected answer requirements from supplied evidence. Include
ordinary behavior and a consequential exception or failure. Questions, expected facts, and their retained source-evidence
packages stay outside canonical knowledge.

A separate reader starts with the root and uses progressive knowledge retrieval. It receives the questions but neither
extraction notes nor expected answers. It answers with supporting references or explicitly identifies missing knowledge.
Source fallback is recorded separately. A reviewer compares the answers against the expected requirements and evidence,
and checks the [authoring ownership rules](knowledge-pyramid.md#placement-and-information-loss), meaningful refinement,
unsupported claims, missing exceptions, and unnecessary code transcription. Ownership review compares the candidate's
changed sections with existing owners and affected summaries even when every reading answer is correct. Issues identify
the competing document paths or sections and the owning explanation; they are reported separately from answer
correctness. Relevant failures require repair or an explicit partial/review outcome.

Evaluation distinguishes:

- Answer coverage: supported versus unanswered questions.
- Correctness: required facts, boundaries, and exceptions preserved, with unsupported claims and contradictions separate.
- Reading cost: knowledge, routing/search results, and source fallback consumed to obtain answers.
- Evidence compression: reading cost compared with the retained evidence package supporting the same answers.
- Source consideration: examined ranges and remaining known aspects, independent of document size.

Initial costs use a deterministic estimate: UTF-8 bytes divided by four, rounded up. These are estimated tokens, separate
from provider usage. Compression is reported only alongside correctness and unanswered questions. Historical comparisons
use the same question set and evidence baseline; adding easy questions cannot improve the old comparison. Existing
question IDs, requirements, and evidence baselines remain immutable; changed questions have distinct identities.

A better candidate answers the same questions correctly with less reading, or supports more important questions without
obscuring existing answers. There is no universal required compression ratio. A consequential qualification is never
deleted to improve a number. Repeated discovery on unchanged inputs preserves satisfactory wording and structure.
