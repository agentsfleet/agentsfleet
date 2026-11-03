# M236 implementation evidence

## Opening dependency verification

Indy approved implementation and latest stable dependencies on November 3, 2026.
The comparison baseline is `2f8c878c09e9c4048da1724385ec58746d348ed4`; opening documentation commit is `cd8245dbf`.
The measured runtime changes are only the Rig 0.46.0 manifest and lockfile upgrade, including transitive rig-http 0.46.0.

Crates.io metadata confirmed rig-core 0.46.0, reqwest 0.13.5, object_store 0.14.2 and aes-gcm 0.11.1 as latest stable.
No additional decision SDK or R2 runtime dependency has been introduced yet.

Run the repository's `make test-unit-rustd` with the pinned Rust 1.99.1 toolchain and all features selected by its existing shards.
The cloud host used two Cargo jobs, disabled debug information/incremental compilation, a child-reaping init wrapper and umask 022.

| Shard | Decisive command output |
|---|---|
| Runner | `unit: runner — 1490 passed` |
| Daemon | `unit: daemon — 139 passed` |
| Daemon libraries | `unit: daemon libraries — 3343 passed` |
| Combined Rust unit | 4972 passed; command exit 0. Datastore-dependent tests remain ignored in this lane. |

Full output remains at `/tmp/m236-rig-rust-tests.log` in this cloud environment.
This proves existing Rust unit compatibility after the dependency upgrade; it does not prove DT functionality or full repository verification.

Initial runs exposed host assumptions: orphaned process reaping and published-file permissions under umask 077.
The completed run used upstream Tini 0.19.0 as a subreaper and umask 022, without source edits or skipped tests.
The downloaded static binary matched upstream SHA-256 `c5b0666b4cb676901f90dfcb37106783c5fe2077b04590973b885950611b30ee`.

## Decision-client component

`afr_providers::decision` contains one configured client behind `DecisionEvaluator`.
The client owns its profile and credential; callers supply a validated request.
Four native decision adapters share guarded transport, a streaming reply bound and physical timeout.

Nine modular scenarios exercise the production client against owned local TLS servers.
They cover native protocols, exact bounds, invalid answers, retained usage, missing results, redirects, cancellation and private diagnostics.
Fixtures use channels, atomics and bounded shutdown without a new mutex or paid provider calls.

The first complete provider/telemetry component run used the existing make target:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afr_providers afr_telemetry'
```

Its deciding output was 144 passed: provider unit 44, decision scenarios 9, existing provider scenarios 49 and telemetry 42.
The log is `/tmp/m236-decision-affected-unit.log`.

This run preceded the final credential-ownership refinement.
The affected decision scenarios then passed nine tests in `/tmp/m236-decision-final-unit.log`.

`make lint-rustd` passed after mechanical source and fixture corrections.
It covered source rules, formatting, workspace Clippy with all targets/features, and separate default-feature builds of both binaries.
Full output is `/tmp/m236-rust-lint.log`; review corrections below require another affected lint check.

Decision metrics record evaluations and elapsed time by bounded provider, purpose and outcome.
Logs and spans carry native call/profile identifiers; the scenarios assert that credentials, evidence and private endpoints remain absent.
Billing settlement and user-visible activity still require native lifecycle integration.

## Astra implementation review and corrections

Requested `gpt-6-astra` with extra-high reasoning reviewed the native code and existing dependency implementations on November 3, 2026.
The review covered the decision client, wire types, scenarios, telemetry, secret ownership and task/schema foundation.
The serving revision was not independently observable; this review supplied no benchmark or live activation evidence.

| Finding | Correction and remaining proof |
|---|---|
| Duplicate JSON keys could hide conflicting decisions. | One parser records ambiguity before replacement. Semantic answers are rejected; independent usage survives, while ambiguous usage remains unknown. |
| Lock waits inflated displayed wall limits. | Native insertion derives creation and expiry from one post-lock clock instant. Full task binding integration proof remains pending. |
| Client cloning duplicated the profile and secret. | Removed the configured client's `Clone` implementation; runtime composition can share the existing evaluator trait. |
| Response decoding copied entire JSON trees. | Deserialize raw protocol records from borrowed values. Owned result strings retain their required allocations. |
| Byte validation allocated rejected serialized buffers. | A bounded counting writer measures complete escaped JSON and stops at the ceiling. The provider question check shares it. |
| Credential validation created an extra plaintext header copy. | Build the actual authenticated request once; preserve builder failures as `NotSent` before executing it. |
| Three tables and a cyclic key shared one oversized slot. | Split unshipped slots 2190–2220 and add supporting foreign-key deletion indexes. Query-plan measurements remain pending. |

After this correction batch, the affected provider/wire gate passed 253 tests:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afr_providers afd_wire'
```

The deciding output was `unit: runner — 253 passed`; full output is `/tmp/m236-astra-fixes-unit.log`.
This includes the nine modular decision scenarios with duplicate-key and exact escaped-byte-boundary coverage.

The correction batch passed `make lint-rustd`; full output is `/tmp/m236-astra-fixes-lint.log`.
The requested Astra extra-high follow-up verified the original findings closed, with no material residual issue in this batch.
It inspected the corrections without running tests, builds or git mutations.

`SecretString` wipes its current owned allocation on normal drop.
An abort can skip destructors; request headers and transport buffers remain outside the owned-buffer guarantee.
No production latency, allocation-count or large-repository transfer benchmark is claimed.

## Catalogue configuration component

The existing catalogue now stores immutable reviewed decision definitions and separate coding/decision price schedules.
Decision-only entries need no fabricated coding rates; omission during an update preserves saved decision settings and prices.
List reads use one database snapshot, and current or historical revisions cannot acquire different definitions.

Astra's requested extra-high review found two validation defects: metadata-only PATCH rejection and PostgreSQL NUL rejection after shape validation.
The corrections allow shape-valid empty PATCH schedules and reject NUL in decision strings before saving.
Native validation still checks whether the saved coding settings require coding rates.

The first affected-package unit command passed 293 tests:

```text
make test-unit-rustd-daemon-libs _RUSTD_UNIT_DAEMON_LIBS='afd_admin afd_api_wire afd_api_operator afd_inference afd_billing afd_tenant'
```

The log is `/tmp/m236-profile-unit.log`; this run preceded the review corrections.
The corrected wire/inference selection passed 141 tests in `/tmp/m236-profile-wire-unit.log`.
The corrected operator/credential selection passed 172 tests in `/tmp/m236-secrets-unit.log`.

`make _migrate-test-db` rebuilt the owned local schemas through slot 2250 and established the `api_runtime` login.
The log is `/tmp/m236-profile-migrate.log`.
All eleven `afd_admin` database scenarios passed, including both new multi-invariant decision scenarios.

Those scenarios prove revision preservation, both rate protocols, rollback, decision-only entries and invalid-definition refusal through production owners.
They ran through the existing make lane with `cargo test --all-features -p afd_admin -- --ignored`.
The log is `/tmp/m236-profile-integration.log`; this scoped run does not satisfy the full repository integration gate.

Astra found no remaining production blocker in the catalogue/pricing component after the corrections.
It identified the existing PATCH test expectation that also needed updating; that correction and static decision-specific refusals are now written.
No activation, latency or allocation benchmark follows from this source review.

The broader native database run exposed an old three-column price conflict key in shared `ModelSeed` SQL.
Seventeen credential scenarios failed during fixture setup; all three new decision-key scenarios and eleven admin scenarios passed.
The log is `/tmp/m236-catalogue-secrets-integration.log`; this run is a failure, not a completed integration gate.

An intermediate correction updated that conflict key, but the full upgrade test exposed its wider incompatibility.
The old coding daemon's writers and readers also needed to remain valid while a release migrated.
That shared-table design is superseded by the additive decision-price table below.

The importer check passed TypeScript compilation, 76 unit tests and its curated-data check through `make _model_allowlist_check`.
Twenty-six database tests were skipped there; the Rust-owned live suite must grade them.
The log is `/tmp/m236-catalogue-import-unit.log`.

The first clean full-workspace gate failed in the historical authoring-upgrade fixture: 262 passed and one failed in that suite.
Its current reader/seed referred to a price column absent from the older schema; the log is `/tmp/m236-current-workspace-integration.log`.
Pinning a test-only writer would not repair deployed coding writers or prevent old readers from mixing price protocols.

Unshipped slot 2250 now adds `model.decision_prices` while preserving the original coding table, uniqueness and statements.
Both sources share price values, row decoding, catalogue transactions and the existing biller; `PriceReplacement` borrows its values and selects fixed statements.
The imported model-identity protections remain, including clearing removed coding settings and excluding decision-only rows from coding diffs.

Astra's extra-high source review verified the additive correction without another material finding.
Decision writes use 15/4 parameters for upsert/prune; coding writes retain their original 14/3.
Creating decision profiles still requires upgraded catalogue writers/importers.

The next clean full-workspace run passed the historical-upgrade scenario, live catalogue importer and Secrets scenarios, then failed seven database schema checks (40 passed).
The log is `/tmp/m236-additive-workspace-integration.log`.

The correction uses UUIDv7 revision identities, model-scoped current selection, immutable revision triggers and scoped decision-price references.
New price instants use `_at` names; original coding SQL remains unchanged.
Explicit table, role, immutability and migration inventories now include the five new tables; no checker logic or exception list changed.

The clean rerun passed all 47 database checks and reached the later daemon HTTP suite.
Repeated lease-based scenarios failed there; the remaining run was stopped by its scoped process group rather than continuing the same failures.
The log is `/tmp/m236-schema-correction-integration.log`; exit 130 is not a completed repository integration gate.

Astra's extra-high source recheck found no material residual defect in the corrected schema/catalogue batch.

## Platform-admin decision keys

The existing `Providers` resolver opens fixed API bags in the trusted platform-admin workspace for each read.
It keeps no key cache and supplies no tenant or environment fallback.
The typed parser owns one guarded key string, and the existing vault reader guards decrypted bytes.

Three native scenarios cover four APIs, workspace isolation, rotation/deletion, malformed fields, damaged ciphertext and unavailable storage.
They drive real encrypted rows and the production Secrets write, replace and delete owners without a new mutex.
Astra found no material issue in this helper slice; reservation and lease-handler composition remain pending.

Parser scratch/error buffers and HTTP buffers remain outside the owned-allocation wiping guarantee.
The resolver's tracing span records its API without exposing plaintext; no production timing benchmark has been measured.

## Fleet question ownership refinement

Indy selected editable fleet questions and criteria through setup chat or CLI, without platform ownership of their wording.
He confirmed that each Apply must create another immutable revision rather than edit an old one.
The proposal now includes separate question/criteria views, revision history and the version each DT has accepted.

`instructions_md` supplies business context; structured `decision_policy` supplies the runnable questions.
Existing DTs require explicit revision adoption, which invalidates older assessments without erasing history or resetting spend.
The native parser now owns typed routing/completion policy under `config["x-agentsfleet"]` and the proposal tool exposes its editable criteria.

UI and CLI review projections are written, including the primary fleet setup screen and proposed/saved revision labels.
Task snapshots retain named questions with full criteria; malformed task JSON uses the existing value-free `Unreadable` cause.

Astra found and guided corrections for the validation-leaf dependency, UTF-8 bounds, primary-screen wiring, excess CLI fields, published criterion shapes and private JSON causes.
The first native component check failed at compile time on a helper-name collision and a generic serde bound; both are corrected.

The native unit shards passed before the latest dependency updates: runner 1499, daemon 139 and daemon libraries 3352.
Across the initial and resumed Make invocations, all 4990 tests passed.
Question rendering/dispatch, explicit task revision adoption and end-to-end DT integration remain pending.

Indy rejected displaying internal routing rules: "What is the value the user gets by seeing that?"
UI and CLI setup now show `What counts as done?`, owner-editable yes/no criteria and the saved revision; routing categories, model terminology and internal statement IDs are omitted.

The validated routing policy and machine-readable setup remain intact. The HTML preview follows the same display rule.
Frozen installs passed at the root and all three UI packages, preserving the existing assistant-ui patches and overrides.

`AGENTSFLEET_UI_ENV_FILE=/tmp/m236-ui-unit.env NEXT_PUBLIC_POSTHOG_ENABLED= make test-unit-app` passed all 4640 tests across 464 files on Vitest 5.1.0.
The 162.86-second run used an explicit public test API origin; it proves the app suite rather than the repository or DT performance.

The first app lint passed Oxlint and type checking, then refused the missing configured environment-file path during source checks.
Using the explicit public test environment, all app, design-system, website and CLI lint targets passed.

The remaining Make unit targets passed: CLI 1989 with 16 skips, design-system 522 and website 133; the combined invocation exited zero.
The four CLI question scenarios cover readable success criteria, unchanged JSON, malformed policy, ordinary fleets, saved revisions and UTF-8 bounds.
These package gates do not complete the repository's remaining native integration, runtime or live acceptance criteria.

The first scrollable HTML walkthrough combined ten setup messages and a work request/reply; its transcript, PR number and task activity were illustrative.
Browser checks verified section navigation, copy/composer controls, omitted routing internals, desktop/mobile layout and zero page errors.

Indy identified that the combined example placed PR work beside a setup composer.
The corrected setup mock contains ten onboarding messages, inline success criteria and the saved revision's `Open fleet` link.
The separate fleet-chat mock contains ten work messages and three DTs, with an optional CI event and four steering/pause follow-up messages.

Browser checks passed for both mocks on desktop and mobile: separate composers, saved-setup transition/spinner, three work items, one running item and selected-PR replies.
The CI follow-up preserves the same work items and fleet conversation; page errors and HTTP requests were both zero.
The original walkthrough now contains setup only; the DT panel remains a proposed runtime UI addition.

Indy approved both mocks and specifically accepted the centered `Save done v1 → Open fleet` pill and work-panel typography.
The implementation reference retains bundled Instrument Sans, current design-system tokens, 14px conversation text, 13px work labels and 12px secondary text.
Native work-panel implementation and its browser comparison remain pending; the mock approval does not complete runtime acceptance.

Indy explicitly signed off both chat visuals.
The approved mock places `Current work` inline above the composer, with details expanding within the same fleet chat on desktop and mobile.

Conditional fleet handoffs were inspected separately: caller steering already supports authorized targets, while native message and schedule tools stay thread/fleet scoped.
Indy authorized a separate-session prompt if this was not a small change; no cross-fleet runtime code is added to M236.

## Integration probe and ship sequence

One clean runner-message reproducer returned 403 before reaching the daemon: its local `[::]:port` address was routed through the inherited HTTP proxy.
Extending `NO_PROXY` only for local fixture addresses made the same clean case pass: one passed, zero failed, with no source changes.
The diagnostic uses the existing Make reset/migrate environment and preserved proxy/CA settings; `/tmp/m236-lease-probe{,-local}.log` holds both results.

Indy: "And if everything is green (CI jobs green, greptile resolved) merge the PR".
Before ship, verify PR #798 is merged, integrate current main, finish all declared gates and push the resulting revision.
Review all architecture pages and all docs-repository Markdown/MDX pages, publish both PRs, and check CI/review status against the actual pushed commit.

## Durable Task acceptance

The task schema and lifecycle service are partial, without completed runtime integration.
Task APIs, physical decision reservations, native accounting, R2 recovery and task UI behavior remain unfinished.
Provider activation, storage, browser, performance and combined live-scenario criteria remain ungraded.

Component tests and SDK compatibility are separate from those acceptance criteria.

## Runner decision selection checkpoint

Existing enrollment, edit, heartbeat, self and operator reads now carry an optional reviewed catalogue selection.
The shared operator form preserves the selection across policy edits and loads enabled models from the current catalogue.
Platform Secrets remain the only decision-key source; assignment carries no key material.

Generic setup-authoring guidance lives in Markdown beside its native assembly code.
Customer instructions and completion questions remain accepted revision data; linkable starter copies use existing immutable Library revisions.

Astra reviewed this source slice at extra-high effort on November 3, 2026, without running builds or tests.
Its corrections cover saved enrollment binds, accurate deletion blockers and catalogue revision ownership under row-lock contention.

`make lint-app` passed; `make test-unit-app` passed 4,641 tests across 465 files.
`make lint-rustd` passed Clippy and both production feature checks; the native unit shards passed 1,499 runner, 139 daemon and 3,353 library tests.

An unchanged toolbox permission test first failed under inherited `umask 0077`; process-local `umask 0022` restored its expected read-only mode.
No source or assertion changed for that environment correction; local proxy exclusions remain required for network fixtures.

Clean `make test-integration-rustd` passed 1,465 regular and three exclusive cases, including catalogue ownership, native secret resolution and both assignment races.
The deferred owner check retains immutable-table privileges and satisfies schema naming and literal rules without changing their checks.
This checkpoint does not establish completed task runtime or measured performance.

## Lease decision snapshot component

PR #798 merged and main was integrated in `28bc628aad363d18498c1a15d5c0a9d85b0a9c37` before the foundations checkpoint was pushed.
`orly gate verify` and the branch history secret scan passed for that pushed checkpoint.

Lease issue stores and delivers one reviewed model/profile/threshold snapshot; assembly moves its decoded strings instead of cloning them.
An unconfigured host skips the catalogue read; a disabled selection supplies no decision profile and cannot select a substitute.
Keys remain outside the snapshot and still require a physical-call reservation before dispatch.

Astra's extra-high source review found a requested-model length mismatch and a stale expiry check after slow profile reads.
The shared 256-byte model bound matches catalogue validation; the issue statement checks a fresh clock after acquiring the fleet slot lock.
The new expiry test blocks the real profile read, observes database expiry and checks lease, audit and tally outcomes together.

After both corrections, `make lint-rustd` and clean `make test-integration-rustd` passed: 1,469 regular and three exclusive cases.
Three cohesive cases cover native issue/render/storage, 256-byte model names, revisions, disablement, deletion, lock ordering and expiry without issuance side effects.
The logs are `/tmp/m236-lease-expiry-{lint,integration}.log`; no new mutex or paid provider request was involved.

The affected unit check passed 655 tests through the existing Make target:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afd_admin afd_api_wire afd_db afd_fleet afd_runner afd_wire afr_providers afr_telemetry'
```

The log is `/tmp/m236-lease-decision-affected-unit.log`; this checks eight affected packages, not the whole repository.
The daemon regenerated OpenAPI with 107 paths and 933,655 bytes, including the lease assignment and its 256-byte requested-model bound.

This component supplies a frozen selection. Physical reservation, key dispatch, task decisions and full DT acceptance remain pending.

## Current work read component

Authenticated collection/detail routes now use the daemon's shared task owner, keyset cursors, immutable accepted criteria and native resource tags.
Collection reads distinguish missing scoped fleets from owned empty fleets; both scope and task SQL run inside one five-second deadline with transaction-local statement cancellation.
Timeouts use a closed metric outcome and bounded reason, alongside the existing task operation counters and duration measurements.

The inline work panel uses the existing fleet chat, fonts, components and stream.
It reads confirmed rows, expands saved criteria lazily and retains explicitly labelled progress during transient refresh failures.
Identity changes and access refusal clear its cached work; the first stream hello closes the initial read/subscription gap.
The native bind/control owner publishes read hints after commit; no extra queue, chat or EventSource was introduced.

Astra reviewed this slice at extra-high effort and verified the subscription-gap, scoped-fleet lookup and SQL cancellation corrections in source.
That review ran no gates. Real publisher-to-browser delivery and forced client-transport deadline behavior still require execution evidence.

Indy's coverage requirement is recorded in the active spec: all changed executable TS/TSX lines reach 100%; Rust patch coverage is strictly above 99%, measured against latest main.
The existing Make and Codecov Rust patch floors now agree at 99.01%; per-file and whole-workspace checks retain their separate floors.
Scenario tests exercise several observable invariants together; the broader test cleanup remains with the other agent.

`make lint-rustd` passed after the final scenario corrections, including workspace Clippy with all targets and features.
The log is `/tmp/m236-task-read-component-lint.log`.
The clean `make test-integration-rustd` run passed 1,471 regular tests and all three exclusive tests; `/tmp/m236-task-read-component-integration.log` records both tallies.

The revised native scenarios prove scoped paging, independent timestamp ordering, accepted revision pinning after a newer save, resource revalidation and connected SQL cancellation.
Their guards dispose owned rows and isolated databases after assertion failure; no new mutex was added.
The daemon regenerated `public/openapi.json`: 109 paths, 957,320 bytes, including both task reads and their response schemas.

`make test-coverage-all` passed on the final UI source; `/tmp/m236-task-component-ts-coverage.log` records the package gates.

| Package | Tests | Measured coverage |
|---|---|---|
| App | 4,653 passed across 468 files | 100% statements, branches, functions and lines; 10,412 executable lines covered. |
| Website | 133 passed | 100% statements, branches, functions and lines. |
| CLI | 1,989 passed, 16 skipped | Existing enforcement passed 100% function and line coverage; Bun does not report branch coverage. |
| Design system | 522 passed | 100% statements, branches, functions and lines. |

The production-component browser recheck passed all eight desktop/mobile cases after the final copy refinement.
It verifies typography, lazy criteria, the shared conversation and usable composer geometry; `/tmp/m236-task-component-browser.log` records the 6.2-second run.
Controlled read fixtures supply its data; this is not full live DT acceptance.

Rust patch coverage has not been measured; raising its floor is not execution evidence.

## Owner work-item status

| Requested work | Current state and evidence |
|---|---|
| Integrate merged main before shipping | PR 798 ancestry is retained by `28bc628aad363d18498c1a15d5c0a9d85b0a9c37`; latest fetch still reports six commits ahead, zero behind. Recheck before push and ship. |
| Complete DT runtime | Schema, decision adapters/catalogue/assignment, owner-controlled questions and task reads are foundations. Physical calls, full agent/lease integration, waits, publication and recovery remain incomplete. |
| Keep approved UI and communicate work | Inline task rows and saved criteria use native reads. Eight final production-component browser checks passed for typography, disclosure and short/mobile viewports; progress/controls and full runtime demonstration remain pending. |
| Private R2 bucket and secure keys | Non-secret bucket metadata resolves to `agentsfleet-checkpoints-dev` in the accessible `ZMB_CD_DEV/cloudflare-r2` item. No key value is recorded here. SDK transfer, encryption, recovery and live bucket checks remain pending. |
| Performance, concurrency and Grafana | Task reads are bounded and observable; cursor paging avoids per-task queries. No benchmark result or complete DT performance trace is claimed. |
| Architecture and companion docs | Owning DT/decision pages track these changes. Full architecture/public-docs corpus review and the companion documentation PR remain pending. |
| Orly lifecycle and PR merge | Local Orly 0.28.0 matches the pin. No feature PR exists; all required gates, actual-head Continuous Integration and Greptile resolution must precede the authorized merge. |
