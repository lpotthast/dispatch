# Backend architecture implementation report

The backend-wide migration is implemented across the domains listed below. Standard verification and hydration checks
pass; browser execution is blocked by incompatible, concurrently edited sibling dependencies.
`knowledge/architecture.md` records the accepted design, while this report retains the implementation checkpoints.

Implemented:

- One application composition root for database access, attribution, shared execution, sessions, the automation
  supervisor, project deletion, event delivery, CrudKit contexts, and owned background workers.
- A thin binary calling the SSR library entry point. Initial SSR and server-function requests receive their own
  application context instead of reading process-global application state.
- Instance-owned event channels and sequence numbers, with explicit delivery dependencies propagated through existing
  producers and websocket subscriptions. Shared execution receives its server API URL through its constructor.
- Attribution transport, service, model, and repository boundaries. Repositories return decoded run records; services
  validate project/run/agent relationships. An opaque transaction handle supports nested validation without another
  pooled connection.
- Shared process execution outside item and knowledge-job workflows. Prepared inputs specify optional process identity
  and incremental-log destinations. Process identity recording and cleanup belong to shared execution; knowledge jobs
  own recovery of their run lists. Existing environment and artifact formats are preserved.
- Explicit worker ownership and shutdown joins, including knowledge passes. Both normal shutdown and server-error
  cleanup stop runtime work. Dropped application owners cancel sessions and abort owned worker handles.
- Shared page query response records with no backend dependency on frontend rendering. Board projections and batching
  retain their existing query implementation.
- `AutomationSupervisor` terminology, typed label mutation inputs, and removal of the superseded global lookup and
  attribution entry points.
- Regressions for independent application state/events/URLs, request context isolation, failed mutations without success
  notifications or partial prompt history, uncommitted scope resolution with one pooled connection, and worker joins.
  Structural tests protect the extracted services and execution boundary and forbid global application lookups and
  backend rendering dependencies.

- Projects now own constructor-injected configuration and deletion services, typed repository reads and writes,
  persistence encoding, runtime adapters, path-status worker, and JSON/form/CrudKit transport modules. Project data
  reuses the shared typed views; deletion uses a small lifecycle scope and run-artifact projection.
- Project creation commits the project, required catalogs/defaults, and initial prompt history together. CrudKit create
  and update call the same project service as the other adapters. Project hooks perform validation only. Notifications
  follow commit, and the superseded project function APIs, initialization wrappers, and top-level deletion module are
  removed.
- Settings, configuration, prompt writes, and prompt-history clearing use the opaque transaction handle. Settings edits
  merge against the project loaded inside the mutation transaction; filesystem knowledge-directory checks run before
  it and validate their observed scope again inside it. Path health cannot record a result for a different stored path.
- Project CrudKit edits retain their immutable project identity, nullable-path behavior, and validation error status.
  Typed settings updates can repair existing policy violations. Project deletion keeps runtime and filesystem cleanup
  outside its final database transaction, and invalid configuration does not prevent deletion.
- Page queries receive the project service explicitly, preserving summary loading without Git inspection and the
  workspace bar's separate Git inspection. Leptos project mutations resolve their request's service instance.
- Added rollback failures after initial default writes and during prompt-history insertion, stale admin identity,
  corrective settings updates, deletion cleanup failure/retry, invalid-settings deletion, live HTTP form/JSON/CrudKit
  equivalence, and Leptos mutation/context isolation checks. Project boundary tests prohibit ORM dependencies in its
  services and workflow writes in its lifecycle hooks.

- Comments and relationships own injected services, typed repositories, and JSON transports. Relationship policy uses
  shared typed views, and project/run attribution, item scope, writes, version changes, and durable events use the same
  opaque transaction. Relationship reads retain batch loading. Notifications follow successful commit.
- Comment CrudKit creation, update, and deletion call the comment service. Creation now shares version/history behavior
  with JSON and Leptos additions; administrative edit/delete preserve their existing maintenance behavior and publish
  committed comment changes. Hooks only parse and validate typed authors and nonempty bodies.
- Removed the superseded comment/relationship function APIs and top-level persistence/policy modules. Existing claim
  persistence and item-deletion callers access the owning domain's persistence adapters pending their own conversion.
- Added comment history failure, second-endpoint rollback for all relationship mutations, single-connection attributed
  operations, live HTTP/CrudKit equivalence and failure coverage, Leptos request isolation, and structural boundary checks.
  `just check` and `just test` pass at this checkpoint: 442 server tests passed, with one ignored provider smoke test.
  `just clippy` and `just check-hydrate` also pass for that checkpoint. The browser dependency blocker has not yet been
  rechecked for this slice.

- Run admission now owns a concrete service, typed count repository, policy, and instance-owned runtime permit. The
  composition root shares it with automation launch/scheduling/routing, knowledge publication/recovery, project deletion,
  and board/run queries. `Store` no longer owns the runtime-admission mutex. Admission revalidates the observed project
  settings and counts runs through the supplied opaque transaction. Allocation itself still needs migration into the
  same transaction with the remaining run workflow.
- Added regressions for uncommitted run visibility through one pooled connection and cancellation/instance isolation of
  admission permits. `just check`, `just check-hydrate`, and all standard tests pass at this checkpoint (444 server tests,
  one ignored). The first `just verify` reached clippy and reported signature/style lints; after correction, `just clippy`
  passed. The item persistence/service migration is now in progress to unify producer creation and remove its Store lock.

- Work-item creation, updates, deletion, list/search/history, and compact board item reads now use injected services and
  typed repository records. Creation, attribution, version checks, effective agent settings, labels/origin initialization,
  mutations, and history run through the supplied opaque transaction. Query enrichment uses its caller's connection.
  JSON, form, and CrudKit item adapters live in the item domain, and CrudKit mutations use the same services. Item hooks
  only adapt validation. The duplicate update input and superseded item function/persistence modules are removed.
- Automation production owns its instance mutex and coordinates deduplication, evaluation, item creation, provenance,
  and evaluation linkage in one transaction. It calls item creation with that transaction and publishes after commit.
  The producer mutex is removed from `Store`. Run item summaries decode their typed results in a batch instead of
  reloading every item's project and complete item view separately.
- This work-item/production checkpoint passes `just check` and `just test`: 447 server tests passed, one ignored.
  Added HTTP equivalence and history-failure rollback regressions pass after correcting their CrudKit selection and
  storage fixtures. `just clippy` and `just check-hydrate` also pass after signature/style fixes.

- Item labels now own a concrete service, typed repository, normalization/mutation/workflow policies, and JSON
  transport. Generic label changes validate project/run attribution and expected versions inside their transaction;
  mutations, catalog discovery/forgetting, item versions, and durable events commit together. Workflow policy contains
  no SQL; its application lives in the label repository. Superseded label function APIs and top-level modules are removed.
- Added label catalog/history rollback with a single pooled connection, Leptos item-state/label mutation and instance
  isolation coverage, and rollback when production's final evaluation linkage fails. This checkpoint passes `just check`,
  `just test` (450 server tests, one ignored), `just clippy`, and `just check-hydrate`.

- Work groups now own an injected service, typed membership projection, repository, key/name policy, and JSON
  transport. Creation and assignment validate attribution and project scope inside their transaction; assignment checks
  every existing membership before its batch update and records each changed item's history before commit. Group
  summaries retain batch loading for item views. This checkpoint passes `just check`, `just test` (451 server tests,
  one ignored), `just clippy`, and `just check-hydrate`, including second-item history failure rollback.

- Durable item events now own their attribution model and persistence adapter. `ItemService` receives the concrete
  event repository for history queries. Event writes and project-history decoding expose typed event/actor enums;
  project-history wire fields retain their existing encoding. The superseded top-level event module is removed.
  `just check` and all standard tests pass (451 server tests, one ignored). Consolidated verification reached clippy; after correcting two redundant result wrappers, `just clippy` and `just check-hydrate` pass.

- Authored item states and board swim lanes now own concrete services, typed records, repositories, policies, and CrudKit
  adapters. Mutations validate scope and commit before notifications; page queries receive these services explicitly.
  Their four new service/CrudKit/rollback regressions pass. The structural check correctly recognizes both
  directory-based and single-file persistence adapters.
- Label catalog configuration now uses an injected service. Clearing persistence and forgetting an unused key are one
  atomic mutation. CrudKit hooks retain validation and permitted-operation behavior, while service-owned writes publish
  only after commit. The superseded catalog/state/lane modules and central resource implementations are removed.
  This checkpoint passes `just check`, `just test` (459 server tests, one ignored), `just clippy`, and
  `just check-hydrate`, including catalog equivalence and atomic forgetting-failure rollback.

- Launch contracts own a typed model, pure validation/digest policy, and persistence/encoding repository under runs.
  The top-level launch module is removed. Its extraction passes compilation and the standard tests.
- Claim selection, ownership, progress, finish, release, feedback, run finalization, and stale recovery now use an
  injected claim service and typed repository plans. JSON attribution is validated within the mutation transaction;
  claimed-item response enrichment happens before commit. Run-target resolution also accepts an explicit caller-owned
  transaction. Automation, shutdown, scheduler, and test callers are migrated, and old claim/recovery function APIs are
  removed. `just check`, `just clippy`, and `just check-hydrate` pass for the claim extraction;
  the standard tests pass with 462 server tests and one ignored after adding second-claim recovery rollback coverage.
  Claim/history failure, uncommitted run-target resolution, and whole recovery-batch rollback use a single connection.

- Agent tools now own an injected service, typed repository, filesystem discovery adapter, and CrudKit transport.
  Discovery prepares filesystem observations before its transaction; configuration and discovery publish committed
  changes from the service. Codex status refresh receives the tool service through its constructor, and readiness/account
  probes no longer receive Store. Shared execution callers and the real knowledge-pass adapter receive this specific
  collaborator. This checkpoint passes `just check`, `just test` (464 server tests, one ignored), `just clippy`, and
  `just check-hydrate`, including service/CrudKit equivalence, discovery metadata preservation, and rollback.

- Codex now owns an injected service, explicit managed home, per-instance operation lock, status cache, and runtime/log
  adapters. Discovery, logout, and every log-purge attempt coordinate status refresh and notifications in the service;
  HTTP forms and Leptos only adapt results. Existing process cancellation and shutdown tests are retained. `just test`
  passes at this checkpoint (466 server tests, one ignored), including isolated locks/caches/events and refresh after
  refused maintenance. Clippy and hydration pass together with the personality migration.

- Automation personalities own concrete services, typed repositories, revision persistence, and JSON/CrudKit transports.
  Creation, editing, restore, and detach commit their immutable revision and current revision pointer in the same scoped
  transaction; deletion enforces Default, bundle ownership, and rule-reference constraints. Notifications follow commit.
  CrudKit hooks only adapt read-only validation. Launch preparation and queries receive the personality collaborator;
  the inspector response lives in shared types and Leptos resolves the request's service. Removed the superseded
  personality function module and central personality revision operations. `just test` passes (471 server tests, one
  ignored), as do `just clippy` and `just check-hydrate`. Added single-connection history failure/rollback, restore scope
  and name-conflict tests, operator/CrudKit equivalence, and Leptos request-instance isolation.

- Automation rule configuration owns concrete service/repository/model/policy modules and form, JSON, and CrudKit
  transports under `automation/rules`. Creation, updates, restore, detach, and manual evaluation queueing resolve scope
  and personality references inside their transaction. Configuration and current revision pointers commit atomically;
  notifications follow commit. CrudKit hooks only adapt validation and retain project-default tools on create and the
  existing tool on update. The former trigger CRUD function API and central workflow hooks are removed; scheduling and
  execution still require their separate service migration. Revision analytics/evaluation queries now own an injected
  query service and typed repository; the rule inspector loads its combined response in one transaction using shared
  response types. Removed the old `automation_revisions` function module. This checkpoint passes `just check`,
  `just verify` (476 server tests, one ignored), and `just check-hydrate`, including revision failure rollback, queue
  concurrency and failure checks, form/JSON/CrudKit equivalence, and Leptos request isolation. The architecture document's
  owning rule-policy path is updated without changing the accepted contract.

- Bundle management now owns a concrete coordinator, typed repository and manifest policy, with JSON and Leptos
  adapters. Managed rule and personality operations accept the caller's opaque transaction and record their revisions
  through their owning services. Apply/removal history commits with those changes; one notification follows commit.
  Leptos deletion confirmation checks the actual diff inside that transaction. The former bundle function module is
  removed. Single-connection rollback and adapter tests pass in consolidated verification.
- Routing explanations now use an injected query service over a single scoped transaction, with pure selector and
  fairness policy. Semantic postconditions own a service, typed persistence adapter, and pure evaluation policy; the
  snapshot contains only project/run-scoped lineage and item state. Removed the superseded routing/postcondition
  function modules and migrated execution, transport, and test callers. `just verify` passes with 479 server tests and
  one ignored provider smoke test, including the bundle and postcondition regressions. A prior run exposed a connection
  timeout in legacy knowledge-job persistence; the full rerun passed without it. `just check-hydrate` also passes.

- Run reads now own an injected query service, typed persistence and encoding adapters, artifact reader, and JSON
  transport. Project scope, run records, launch contracts, and item lineage load within one opaque transaction; log
  files and live session output are read after commit. The board's compact, bounded window-query previews remain
  intact. Page, HTTP, Leptos, and test callers use the service, and the old run-query function API is removed. The
  run CrudKit resource is moved into its domain; its mutations still require lifecycle-service integration.
  `just check` and `just verify` pass with 479 server tests and one ignored provider smoke test, including batching,
  malformed-contract handling, and active-log output. `just check-hydrate` also passes. Typed lifecycle mutation
  and atomic run/claim finalization are the next active slice.

- Run lifecycle mutations now use a concrete service and typed repository, reusing `AgentRunView` and shared enums.
  Allocation, launch updates, process metadata, commit outcomes, semantic results, cleanup status, and termination no
  longer expose ORM rows to execution workflows. The old run mutation and stop function APIs are removed. Termination
  commits launch-contract state, automatic claim release, and durable item history together; project cancellation is
  atomic across its batch. Late process completion preserves terminal status without another notification. Target
  resolution reloads claim attribution before returning within the same transaction. Workers, shutdown, deletion,
  knowledge passes, and tests call the lifecycle service. Single-connection terminal-history rollback and second-run
  cancellation rollback pass, together with the existing runtime/claim/knowledge suites (480 server tests, one ignored).
  `just clippy` and `just check-hydrate` pass after fixing enum layout and fixture borrowing. `knowledge/workflows.md`
  states the atomic termination and terminal-status guarantees in present tense. Launch coordination, scheduler and
  knowledge persistence, and run CrudKit mutations remain active migration work.

- Workspace opening, editor discovery, and the native folder picker now use an injected service and runtime adapter.
  HTTP and Leptos adapters resolve the request's service; project/run scope is validated before filesystem or process
  operations. The central router only assembles these routes, and the old workspace function module is removed.
  `just check` and `just verify` pass (480 server tests, one ignored), including existing platform command tests.
  Hydration is being verified with the following launch-coordination slice.

- Ordinary launch and worktree cleanup now use a constructor-injected launch service with explicit runtime and session
  collaborators. Admission, project settings, personality revision and prompt snapshot, and run allocation share one
  transaction; workspace preparation and execution remain outside it. Launch contracts and claim resolution retain their
  shared transaction. The old automation function API is removed. `just verify` passes with 481 server tests and one
  ignored provider smoke test, including allocation rollback on a single connection and captured personality inputs.
  `just check-hydrate` passes for the launch and workspace slices. Scheduler persistence is the next active slice.

- Scheduling owns a concrete service, typed repository, shared fairness policy, and explicitly owned worker. The old
  rule execution function module is removed. Project resolution, admission, queue consumption, item matching, and
  schedule updates use explicit transactions. Produced items, evaluation records, and the corresponding schedule
  advance commit together; consuming-run evaluation history and its schedule update are also atomic. The producer
  supports caller-owned transactions. Routing explanations reuse the scheduler's fairness policy. `just check` and
  `just verify` pass with 482 server tests and one ignored, including final schedule-update failure rollback on one
  connection and existing fairness, exclusive routing, queued evaluation, and cancellation cases. Hydration is being
  checked with the following supervisor slice. The atomic production contract is stated in `knowledge/workflows.md`.

- Automation supervision now receives project persistence, run lifecycle, and session coordination through its constructor
  and lives in the automation domain. Stop adapters call one authoritative operation; deletion explicitly closes runtime
  admission before its existing drain and cleanup. Shutdown uses the supervisor's owned session registry. Automation and
  run form adapters are removed from central routing. Current-item launch requests resolve their item/version inside
  allocation rather than in HTTP. `just check`, `just verify` (483 server tests, one ignored), and `just check-hydrate`
  pass, including the added stop/session/terminal-state regression. Dedicated board/operator query conversion follows.

- Board and operator pages now use injected query services, retaining shared response types, compact item projections,
  bounded run previews, and existing batch loading. Leptos adapters call their request's query services, and the central
  page-data function module is removed. Codex page queries preserve the four-minute detailed refresh. `just check` and
  `just verify` pass (484 server tests, one ignored), including page/section equivalence and request-instance isolation.
  `just check-hydrate` also passes. Knowledge documents/jobs and run administrative mutations remain active work.

- Knowledge documents and jobs now use injected services, typed repositories, per-instance job coordination and a shared
  document writer. Pass execution receives prepared settings and artifacts; allocation links the job and run in one
  transaction. Publication retains its root-last recovery journal. Project deletion receives the job service and no
  longer receives Store; Store owns no workflow locks. JSON and Leptos adapters call these services, and ordinary versus
  job-aware run cancellation now shares a coordinator. The old knowledge function APIs and execution module are removed.
  The standard suite passes with 491 server tests and one ignored provider smoke test, including allocation rollback,
  form/JSON/Leptos equivalence, checked document saves, independent coordinators, and publication writer exclusion.
  Consolidated verification reached clippy; after correcting its test-only struct-update lint, clippy and hydration
  pass. The remaining run administration and boundary audit follow.

- Following the user's naming clarification, every custom REST/form domain is being wired to a constructor-injected
  `*Controller`. Axum handlers extract inputs and forward to those controllers; `*Service` and `*Repository` retain
  workflow and database responsibilities. Startblock's explicit composition is used as a reference, while Dispatch's
  modules remain grouped by domain/use case. The architecture document now states this naming and dependency direction.
  `just check` and `just verify` pass for controller extraction (491 server tests, one ignored), including the existing
  HTTP/Leptos/CrudKit equivalence cases. Event websocket routing now belongs to its domain, and session scope belongs to
  the run query service. A syntax-aware, backend-wide boundary regression is being added to the standard test suite.

- Run CrudKit mutations now use run services. Metadata checks retain launch attribution and project scope. Deletion closes
  run registration, drains execution, shares guarded artifact cleanup with project deletion, and commits final run removal
  with claim-release history. Added rollback, retry, process-drain, late-registration, and live CrudKit regressions pass.
  The server suite passed with 496 tests and one ignored provider smoke test. Runtime relocation followed before the
  remaining checks completed; the next consolidated verification covers both changes.
- The syntax-aware boundary check now covers every backend source and all custom controller handlers; it also rejects
  generic SeaORM mutation repositories for Dispatch CrudKit resources. CLI resolution, Git preparation, run workspaces,
  and commit inspection now receive concrete injected collaborators. Their modules, sessions, output, prompt helpers,
  and label conditions live under their owning domains. The last unused pooled launch-contract lookup is removed.

- Final runtime wiring separates shared execution service, prepared models, and the process adapter. Sessions, CLI
  resolution, Git setup, workspaces, and commit inspection are constructor dependencies. Project and run deletion
  revalidate the inspected working-copy scope in their final transaction. Stale scope, independent CLI/PATH inputs, and
  HTTP SSR/server-function regressions pass. The standard suite now has 500 passing server tests and one ignored provider
  smoke test; it also retains the other Dispatch crate suites. No superseded production function API or forwarding
  module remains for the migrated workflows.

Verification status:

- `just check` passes for all five Dispatch crates.
- `just check-hydrate` passes with the browser-only feature configuration.
- `just verify` passes: formatting, all standard Dispatch crate tests, and clippy with warnings denied. The server suite
  has 500 passing tests and one ignored provider smoke test, including the final opaque-transaction boundary regression.
- The boundary suite parses production Rust and enforces repository-only ORM dependencies, no frontend or application
  state in domain services, injected REST controllers with forwarding Axum handlers, explicit opaque transactions for
  scoped methods, and service-owned CrudKit mutations without workflow writes in hooks.
- `just browser-test` and `just browser-test-signals` were rerun. Both fail before Dispatch starts because the sibling
  `/Users/lukaspotthast/dev/browser-test` imports APIs absent from its current `chrome-for-testing-manager` checkout:
  `DriverOutputListener`, `ChromeForTestingManagerError`, `Chromedriver`, `ChromedriverRunConfig`, and `OperationOutcome`.
  It also expects `DriverOutputLine.sequence` and `From<u16>` for `Port`. Both sibling checkouts contain concurrent edits;
  those edits were preserved. No Chrome/browser assertions ran. These suites still require verification once the
  sibling dependency APIs agree.
- A real HTTP regression verifies initial SSR shell rendering and Leptos server-function reads and mutations using two
  separately constructed applications, including committed prompt history and events. It complements the hydration
  compilation check; it does not replace the blocked browser suites.
- `git diff --check` passes. Schema and migration files are unchanged by this migration.

Documentation impact: `knowledge/architecture.md` describes domain ownership, constructor composition, naming,
controller/service/repository direction, persistence types, runtime adapters, and opaque transactions in present tense.
`knowledge/api.md` specifies service-owned CrudKit mutations and the run administration boundary. `knowledge/workflows.md`
records atomic production and termination, guarded deletion, and final working-copy scope validation.
`knowledge/data-model.md` clarifies administrative comment history, and `knowledge/knowledge-automation.md` specifies
atomic job/run allocation and continuation history. Owning summaries were checked for contradictions; knowledge contains
no task status, verification report, TODO, or migration checklist.

No database schema, migration identity, HTTP route, CLI command, server-function signature, event payload, or runtime
artifact format is intentionally changed. Concurrent manifest and CrudKit edits remain in the checkout.
