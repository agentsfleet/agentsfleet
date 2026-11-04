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
That review ran no gates. Real publisher-to-browser delivery and forced client-transport deadline behavior still require live proof.

Changed executable TS/TSX lines require 100% coverage; Rust patch coverage must strictly exceed 99% against latest main, per Indy's instruction in the active spec.
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

The [native coverage attempt on `bf0684c97`](https://github.com/agentsfleet/agentsfleet/actions/runs/41723739970) passed runner, daemon and kernel shards.
The substrate shard failed on a concurrent catalogue seed primary-key collision; the complete Rust patch percentage remains unverified.
Raising the floor is not a measured result; only a complete report at the tested revision can satisfy it.

## Mixed-model settlement recovery

The existing billing sweeper now reconstructs provider/model/posture groups under each original work identity.
It selects the oldest bounded batch first, sorts borrowed keys in place, and moves owned headers without allocating cloned map keys.
Recovery counters count settlement groups; their existing names and closed work-type labels remain unchanged.

One isolated live scenario verifies complete headers, six unique usage lines, cumulative per-work amounts, native wallet settlement and unchanged repeated recovery.
It uses four model/posture groups under one work and another work with the same model.
Stored charges are synthetic inputs; the scenario does not measure provider rates.

The same scenario failed against the previous implementation at its native reconstruction assertion: two groups appeared instead of five.
The fixed source was restored before its verification batch; `/tmp/m236-billing-group-parent-red.log` records the assertion failure.
Astra's extra-high review found no material product defect and verified the grace-period and independent sequence-ordering corrections in the test.

| Component check | Result and retained output |
|---|---|
| `make lint-rustd` | Passed; `/tmp/m236-billing-groups-checks-lint.log`. |
| Affected native units | 331 passed through `make test-unit-rustd-runner` for `afd_billing afd_fleet afd_observability`; `/tmp/m236-billing-groups-unit.log`. |
| Clean native integration | 1,472 regular tests and all three exclusive tests passed; `/tmp/m236-billing-groups-integration.log`. The original Make process completed with exit zero. |

The new scenario passed in that clean integration run.
No new mutex, billing queue or metrics family was added.
Physical decision reservations, frozen call pricing, receipt accounting and hold-release guards remain pending; Rust patch coverage remains unmeasured.

## Usage sequence component

Coding, runtime and native decisions use disjoint sequence ranges under the same lease work identity.
Runtime allocation rejects negative, exhausted and overflowing counters; renewal and expiry SQL exclude decision sequences from their runtime counts.
The cohesive native scenarios compare complete boundary settlements and exercise decision usage, renewal, abandonment, expiry billing and replay together.

The failed remote coverage attempt exposed a fixed catalogue seed identifier shared by concurrent tests.
Each seed now mints a candidate and lets the existing provider/model uniqueness rule choose the logical row.
A bounded isolated scenario releases 100 callers across two models, then checks their complete identities, inference settings and quotes together.

It proves concurrent callers over the configured pool, not 100 simultaneous SQL statements.
Exercise and teardown deadlines begin after fixture construction; they do not prove bounded migration startup.

Astra reviewed this component at extra-high effort and found no material source or test defect.
Its documentation correction is applied in the billing architecture page; the review ran no gates.

`make lint-rustd` passed after correcting two fixture string assignments to reuse their allocation.
The log is `/tmp/m236-usage-ranges-lint-final.log`.

`make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afd_fleet afd_wire'` passed 291 tests; `/tmp/m236-usage-ranges-unit.log` records the scoped result.
The initial clean integration run caught a prepared-plan fixture missing the new twelfth metering argument; both its declaration and invocation now match production.

The clean `make test-integration-rustd` rerun passed 1,473 regular tests and all three exclusive tests.
The log is `/tmp/m236-usage-ranges-integration-final.log`; the complete Rust patch coverage measurement remains pending.

`orly gate work` and `orly gate verify` passed, including `make lint-all` and version agreement at 0.72.0.
The verification log is `/tmp/m236-usage-ranges-verify.log`; these checkpoint checks do not close the full milestone.

## Complete native coverage measurement

The [next remote measurement](https://github.com/agentsfleet/agentsfleet/actions/runs/41732479873) tested `6c5392015`.
Runner, daemon and kernel reports completed; the substrate worker exhausted disk before uploading its report.
The failed check annotation identifies `System.IO.IOException: No space left on device`.

Indy approved the three-line resource proposal on Nov 04, 2026: "Approve the resource settings".
The shard worker now disables development/test debug information and limits Cargo to two jobs, matching the local measurement settings.
No coverage floor, source exclusion, test selection or required check changed.

A clean verification checkout at exactly `6c5392015` completed `make test-coverage-rustd RUSTD_SHARD=substrate` with the documented cloud Cargo settings.
Its deciding output was 5,108 regular tests and three exclusive tests passed.
The log is `/tmp/m236-coverage-6c5392015-substrate-local.log`.

The original remote reports and all four original revision stamps were retained.
Only the local report's workspace `SF:` prefix was aligned with the remote reports; a byte comparison verified every non-path counter record remained unchanged.

The unchanged `make test-coverage-rustd-merge` gate then measured 99.0137% whole-workspace coverage and 79.6834% patch coverage: 2,114 of 2,653 changed executable lines.
Thirty-eight of 111 touched files were below the separate 99% file floor.
The log is `/tmp/m236-coverage-6c5392015-merged-canonical.log`.

This fails the required floors; no coverage requirement or exclusion was relaxed.
The largest gap is the native task binding owner, followed by negative paths in decision adapters and reviewed configuration.

The next component adds cohesive scenarios through `TaskLifecycle` for concurrent whole-plan acceptance/replay, source/revision qualification, capacity rollback, cumulative limits, controls and unreadable saved state.
A real blocked Postgres query tests the client deadline and pool-permit recovery; it does not prove literal packet loss.
The fixture retains database cleanup ownership outside guarded setup and checks.

Astra's extra-high review identified and checked the revision, wall-duration and connection-lifetime corrections, then closed the guarded-setup cleanup finding with no remaining material finding.
`make lint-rustd` passed after two empty-page assertions were changed to retain the returned values in failure diagnostics.
The log is `/tmp/m236-task-lifecycle-lint-final.log`.

The first integration run found invalid fixture names; all three definitions now use valid slugs and include the required budget.
After a clean schema reset, the scoped `cargo test --all-features -p afd_fleet --test fleet_suite integration_tasks:: -- --ignored` batch passed all ten scenarios.
The log is `/tmp/m236-task-lifecycle-focus.log`; this scoped pass does not replace the repository gate.

The clean `make test-integration-rustd` rerun then passed 1,483 regular tests and all three exclusive tests.
The log is `/tmp/m236-task-lifecycle-integration-final.log`; its recorded exit status is zero.

## Native lifecycle coverage result

The [four-shard measurement](https://github.com/agentsfleet/agentsfleet/actions/runs/41751320421) completed successfully at `b5db2b455` with the approved worker resources.
Its unchanged merge gate measured 99.5583% workspace coverage: 78,893 of 79,243 executable lines, with 350 missed.
All four raw reports and revision stamps were verified against that commit.

Manual workflow dispatch leaves the patch base empty, so that green run did not grade patch or touched-file coverage.
The unchanged merge gate was rerun with explicit base `fde5859717c73ea52d02d84d2408b8f00efcae82` in a clean checkout of the same commit.
It measured 96.0133% patch coverage: 2,601 of 2,709 changed lines; 30 of 113 touched files missed the separate 99% floor.

The log is `/tmp/m236-coverage-b5db2b455-vs-fde585971.log`.
Those results fail the required patch/file floors; they precede the subsequent main integration and cannot grade that integrated source.
No coverage floor, assertion, exclusion or report counter was changed.

A temporary source mutation inverted the native machine-source guard in the isolated verification checkout.
The qualification scenario failed at its authority assertion when the mutated owner allocated bindings; the original source was restored byte-for-byte.
The deciding output is retained in `/tmp/m236-source-authority-mutant.log`; the verification checkout is clean.

Main advanced to `11883fdab` while coverage ran; merge `205addaa2` integrates it before the next feature push.
Astra found no material merge defect in the combined decision/memory lease payload, parallel context reads or gate registration.
The integrated provider-component gates below subsequently passed.

## Provider validation with integrated main

Decision encoding now checks each question before collection, removing an unnecessary temporary reference vector and unreachable representation errors.
Score validation walks borrowed, ordered entries instead of repeatedly looking them up.
The duplicate-key parser retains ambiguity detection while removing JSON visitor paths its decoder cannot call.

Three cohesive HTTP scenarios exercise 54 exchanges across the four protocols, alongside the existing transport, deadline and preflight cases.
They compare complete outcomes, reported identity, retained usage, credential placement, physical-send counts and scoped measurements together.
The scenarios add no mutex or process-wide capture lock.

Astra found masking errors in the first duplicate fixtures and an incomplete model-identity assertion.
The corrected fixtures append duplicates to complete valid distributions and compare the entire `DecisionResult`.
The shifted-score fixture also preserves a matching weighted score, so removing contiguity validation would incorrectly accept it.

The first lint pass requested a borrowed measurement slice; that mechanical correction is applied.
`orly doctor`, `orly gate work` and `orly gate verify` passed; verification includes `make lint-all` and version agreement at 0.73.0.
The final verification log is `/tmp/m236-main-provider-verify-final.log`.

`make test-unit-all` passed 1,593 runner, 141 daemon and 3,377 library tests, with all four TypeScript coverage gates at 100%.
The app passed 4,653 tests; website 133, CLI 1,997 and design system 522; 16 CLI live-acceptance tests remained explicitly skipped.
The log is `/tmp/m236-main-provider-unit-all.log`; these results do not establish live acceptance.

Clean `make test-integration-rustd` passed 1,494 regular tests and all three exclusive tests.
The log is `/tmp/m236-main-provider-integration.log`; the enclosing batch recorded exit zero.
A fresh fetch confirmed main remained `11883fdab`, an ancestor of the integrated branch.

The companion documentation branch also integrates its latest main in `9a70538` and is pushed.
Its work gate passed 25 checks; full documentation verification and the branch secret scan passed.
The logs are `/tmp/m236-docs-main-work.log`, `/tmp/m236-docs-main-verify.log` and `/tmp/m236-docs-main-secrets.log`.

Current-main Rust coverage remains below the required patch floor; the next section records the complete measurement.
Physical decision reservations, runtime integration and R2 recovery remain unfinished.

## Current-main native coverage

The [four-shard measurement at `37c177bb8`](https://github.com/agentsfleet/agentsfleet/actions/runs/41761357273) passed every shard and the workspace floor.
All four downloaded report archives, checksums and revision stamps matched that exact commit.
Manual dispatch left the patch base empty; its green result did not grade changed lines.

The unchanged merge command ran in a clean checkout with explicit base `11883fdab`.
It measured 99.6178% workspace coverage, covering 79,498 of 79,803 lines.
Patch coverage was 97.3743%, covering 2,633 of 2,704 changed lines; 26 of 114 touched files missed their separate 99% floor.

`make test-coverage-rustd-merge RUSTD_PATCH_BASE=11883fdab` therefore failed the required patch/file checks.
The log is `/tmp/m236-coverage-37c177bb8-vs-11883fdab.log`; no floor, report counter or exclusion changed.

## Editable pull-request starter

Indy clarified that New fleet's Review pull requests button must load the full customer-editable starter, including proposed completion questions.
The canonical public JSON supplies all three existing buttons through the existing suggestion component with `send={false}`.
It adds no loader, request route or dependency.

The review starter asks for repository, timing and destination instead of assuming them.
It includes two completion questions with yes/no criteria; owner edits travel in the ordinary setup message.
Selection makes no write or save request, and submitting sends exactly one edited message.

The Rust source audit found no production PR-review canned reply to delete.
Authoring response text comes from the model response; accepted revisions supply customer instructions and questions.
Generic authoring guidance remains separate; the earlier claim that a PR-review canned reply remained in Rust was incorrect.

The replacement regression failed on the parent UI's one-line starter at its completion-question assertion.
The log is `/tmp/m236-starter-parent-red.log`; the fixed flow passes in the full app suite.
Astra's source review found no material issue and ran no builds or tests.

`make lint-app` passed, including type checking and source rules.
`make test-coverage-all` passed all four 100% coverage checks: app 4,653, website 133, CLI 1,997 and design system 522 tests.
Sixteen CLI live tests remained explicitly skipped; `/tmp/m236-starter-coverage.log` records the complete results.

The production-component browser suite passed ten desktop/mobile cases, including keyboard selection, retained edits, zero writes, 14px text and usable composer geometry.
The log is `/tmp/m236-starter-browser.log`; the batch recorded exit zero.
This controlled browser proof does not establish live model authoring or full DT acceptance.

The companion setup guide is pushed in `0b2bc39`, after its work gate, complete site validation and secret scans passed.

## Native decision accounting checkpoint

The native component stores one physical-call intent, obtains confirmed whole-work credit, resolves fresh platform Secrets and commits one credential issuance.
Replays contain saved request/results without the key.
Typed receipts retain unknown reservations or record normalized usage at the frozen price under the original lease and payer, including after fleet deletion.

The shared biller and SQL helpers remain the owners; no second task queue or executor is introduced.

The requested `gpt-6-astra`, extra-high source review found an intent-receipt/issuance race.
Receipts now take the existing optional fleet-slot lock before task/call locks, serializing actual overages with further physical issuance.
Astra verified the correction and reported no residual material component findings; it ran no tests.

Its serving revision and effective reasoning setting were not independently observable.

`make lint-all` passed after merging main `bf3baa126`, including workspace/all-target/all-feature Clippy with warnings denied.
`orly gate verify` then passed in `/tmp/m236-decisions-post-main-verify-2.log`, including changed prose and product-version checks.
The earlier enclosing gate reported documentation paragraph and wording findings; their correction changed no runtime behavior.

The new scenarios cover 100 concurrent replays, uncertainty/retry accounting, source/task budget ownership, deletion, funding recovery, cancellation and slot ordering.
On integrated main `bf3baa126`, all eight pass within clean `make test-integration-rustd`: 1,502 regular tests and all three exclusive tests passed.
The enclosing batch exited zero; `/tmp/m236-decisions-post-main-integration.log` retains its results.

The funding scenario now also proves missing coding prices and unconfirmed credit preserve the same intent before fresh-key recovery.
It checks the distinct bounded funding reasons alongside retained rows, one later charge and revoked-model disposition.
The tests add no mutex; existing trace capture is acquired once per scenario before creating a lease.

After merging main `bf3baa126`, `make test-unit-all` passed 1,594 runner, 141 daemon and 3,378 library tests, totaling 5,113 Rust tests.
All four package coverage gates remain at 100%: app 4,653, website 133, CLI 1,997 and design system 522 tests.
Sixteen CLI live tests remain explicitly skipped; `/tmp/m236-decisions-post-main-unit.log` records the complete results.

The provider fixture now accepts an intact eighteen-option compiled question above 1 KiB, then rejects individually valid fields whose escaped representation exceeds 16 KiB.
The authored-question limit remains separate and unchanged.

The first integration build exhausted worker disk before tests ran.
Targeted `cargo clean` reclaimed 7.8 GiB, then another 2.7 GiB after the complete unit run.
No source, coverage report, floor or test exclusion was removed to recover space.

Two further targeted cleanups reclaimed 1.3 GiB each around post-merge verification; `/tmp/m236-decisions-post-main-clean.log` records the second cleanup.

Subsequent failures exposed missing migration expectations, nonstandard private column names and unregistered assembled SQL.
Their fixes preserve every on-disk statement check and add both assembled queries to the existing positive preparation inventory.
The native fixture also now prices its coding model and uses the actual steer `message` field.

The Resume adjustment passed ten desktop/mobile production-component browser cases in `/tmp/m236-ui-polish-browser.log`.
The same Resume geometry check fails against the previous component in `/tmp/m236-resume-parent-red.log`.
These checks use controlled data and do not establish live DT acceptance.

### Component file responsibilities

| Owner | Change |
|---|---|
| `schema/2280_runner_decision_calls.sql`, `2290_runner_decision_payloads.sql`; migration, role and upgrade inventories | Retain financial call identity after live-owner deletion; cascade private request/results with their owners; enforce operation, retry and sequence uniqueness. |
| `afd_fleet/src/task/decision/{ownership,prepare,compile,request,context}.rs` | Resolve trusted lease/source/task state and accepted criteria; compile complete bounded questions and native assessment identity. |
| `afd_fleet/src/task/decision/{allocate,attempt,intent,fund,dispatch,row,replay,sql}.rs` and statements | Commit intent, bound retries, confirm whole-work credit, resolve fresh keys, issue once and return credential-free replays. |
| `afd_fleet/src/task/decision/{account,journal,record,semantic}.rs` | Normalize usage, price the frozen quote, atomically record the original payer's charge and separately judge current answer usability. |
| `afd_fleet/src/task/{transaction,fetch,observe}.rs`, error shell and existing lease owners | Bound database waits, release cancelled sessions and pool permits, record closed outcomes, include unresolved calls in holds and preserve those holds until reconciliation. |
| `afd_billing/src/{rates,budget}.rs`, `{price,sql}/`; `afd_credential/src/{provider/decision,vault/mod}.rs` | Reuse existing rate, budget, biller and encrypted-vault facilities on the caller's bounded connection. No second ledger or secret store. |
| `afd_wire/src/{decision,task}/`, `paths.rs`; `afd_api_runner` decision handlers; `afd_http` runner routes; `agentsfleetd` plane | Share typed answer validation, expose the two lease-scoped decision routes and supply only server-owned administrative credential context. Regenerated OpenAPI preserves every existing path. |
| `ui/packages/app/components/{layout/DraftSidebarItem,domain/SteerComposer}.tsx`; browser and unit owners | Align Resume and its reachable Delete action; shorten guidance to “Change fleet defaults in Edit setup.” Shared typography and theme remain. |
| Native lifecycle scenarios, HTTP refusal matrix, provider limits and SQL inventories | Prove complete flows and residual state together; retain negative controls and use actual native implementations and API-role databases. |
| Owning architecture pages, metrics census, active spec and this ledger | Record actual boundaries, bounded funding diagnostics and operation cardinality, measured evidence and remaining work. |

## Checkpoint encryption component

`afd_crypto::checkpoint` defines borrowed native object scope, wrapped key metadata, temporary key ownership and encrypted chunks.
The daemon wraps each new object key with the existing envelope; transfer code consumes that temporary key during object encryption or decryption.
The code moves the existing object allocation and checks scope through authenticated associated data.

The byte format separately binds wrapping purpose/version/nonce and data purpose; `sandbox_evaluation.md` records its exact fields.
The AES-GCM `zeroize` feature clears AES and authentication state on drop.
The shared envelope now guards its entropy destination before filling it, preserving its stored format.

The final scoped command `make test-unit-rustd-runner _RUSTD_UNIT_RUNNER=afd_crypto` passed 68 tests after the key-consumption correction.
`/tmp/m236-checkpoint-crypto-final-unit.log` records the result; this is component evidence, not a repository-wide verification result.
Four new scenarios combine an independent format vector, allocation checks, empty/small/8-MiB chunks, ownership changes, corruption, entropy failure and malformed keys.

Node 24.19.0 with OpenSSL 3.5.7 independently produced the fixed ciphertext/tag vector.
The scenario also checks a reused buffer's previously initialized tail, then decrypts the same allocation.
A documented test-only observation reads that initialized range; production crypto adds no unsafe block or mutex.

Astra's requested extra-high source review found the spare-capacity retention issue and the envelope's inherited unguarded entropy buffer.
Both corrections were verified closed against the actual worktree; no material component finding remained.
The reviewer ran no builds or tests.

An inner-loop mutation changed the full spare-capacity wipe to an empty slice.
The complete vector/allocation scenario failed at its retained-plaintext assertion, then passed after byte-exact source restoration.
Logs are `/tmp/m236-checkpoint-crypto-mutant.log` and `/tmp/m236-checkpoint-crypto-restored.log`.

Conformance identified explicit key consumption and two mechanical test-helper findings; their corrections add no lint suppression.
The source now drops the transferred key immediately after constructing the guarded cipher.
`make lint-rustd` passed, including formatting and workspace Clippy over all targets/features; `/tmp/m236-checkpoint-crypto-lint-3.log` retains its results.

No R2 SDK dependency or live transfer is claimed by this component.
Native registration, quotas, manifests, transfer, restore and the live private-bucket proof remain pending.
No complete recovery Dimension is marked done.

## Published-head coverage result

The coverage run for published head `2ace1f3a1` completed all four Rust coverage shards successfully.
The [merge job](https://github.com/agentsfleet/agentsfleet/actions/runs/41815241326/job/126531170305) failed the existing floors.
Its merge checkout is `06156bdfe677d47d752c33a79e378b48c55ba4ae`, combining that head with main `bf3baa126`.

| Measurement | Observed | Required |
|---|---|---|
| Workspace line coverage | 81,517 / 82,006; 99.4037%; 489 missed | 99.5% |
| Changed Rust lines | 4,678 / 4,932; 94.8500%; 254 missed | 99.01% |
| Touched files | 48 / 159 below the per-file floor | Every touched file at least 99% |

This is the measured result for the published head, not the later local maintenance work.
No floor, exclusion or coverage report has been changed to hide the misses.
Missing native decision, HTTP and configuration behavior remains work before readiness.

## Abandoned-decision maintenance component

The same `Tasks` owner now supplies `TaskLifecycle::maintain_decisions` to the existing billing loop.
The pass bounds candidates to 100 and total time to ten seconds; individual statements retain their existing three-second bound.
The financial journal records proved-unsent expiry under the shared parent/slot/task/call locks, preserving issued uncertainty and original ownership.

Five complete database scenarios cover released/future/unknown reservations, deletion, cancellation, delayed issuance commit and competing expirers.
The query-plan check uses the exact static production statement and a test-local planner setting; it requires an indexed deadline range.
Native metrics add `decision_sweep`, bringing each operation/outcome family to 88 bounded series.

Astra first found a volatile-clock range scan, an unqualified text tie-breaker and missing stale-candidate race coverage.
The query uses `statement_timestamp()` and orders the qualified source UUID.
Initial transaction races observed blocking; corrected maintenance cases now expect contention skips.

Per-item expiry logs use debug, with aggregate counts and overdue age at info.

`make lint-rustd` passed after the final SQL qualification; `/tmp/m236-maintenance-lint-5.log` records formatting and workspace Clippy success.
The scoped changed-crate unit selection passed 429 tests before that SQL qualification; `/tmp/m236-maintenance-unit.log` retains its output.
The first integration attempt stopped during compilation for the query correction; it supplies no live test result.

The second build failed before tests after exhausting disk space; `/tmp/m236-maintenance-integration-2.log` records the linker and filesystem errors.
`cargo clean` removed 23,775 build files, recovering 19.3 GiB; the following `df` reported 19 GiB available.

The clean `make test-integration-rustd` gate passed: 1,507 regular tests and three exclusive tests; exit zero.
`/tmp/m236-maintenance-integration-3.log` retains that pre-fairness result, including all five initial recovery scenarios.

A focused Astra follow-up confirmed a further liveness defect: one blocked or malformed oldest candidate can starve every later fleet.
The correction keeps a typed keyset cursor in the existing billing loop, advances before attempts, skips contention and isolates row failures.
Expected contention, failed rows and recovery age need separate bounded diagnostics.

The starvation regression failed against the prior implementation at its one-second unrelated-recovery deadline.
`/tmp/m236-maintenance-fairness-red.log` records that assertion failure with zero passing tests and one failing test.

The corrected `make test-integration-rustd` gate passed: 1,511 regular tests and three exclusive tests; exit zero.
`/tmp/m236-maintenance-fair-integration-2.log` records the full rerun after the fixture correction and exact SQL restoration.

`orly gate verify` passed after resolving the local UI fixture path and evidence paragraph formatting.
The changed-crate unit selection passed 430 tests, including the capturing-exporter scenario.

The loop owns `DecisionMaintenanceCursor`; the `TaskLifecycle` trait borrows it mutably for each bounded pass.
Shared `locks::Mode` retains ordinary waiting for receipts and uses `NOWAIT` for maintenance without changing lock order.
No mutex or new worker is added; the cursor moves each candidate identity without copying it.

Astra also identified indefinite postponement when new arrivals keep every continuation nonempty.
Each scan cycle now retains its initial database-time cutoff across pages and cancellation; short or exhausted pages reset the cycle.
Arrival-between-passes cases cover both short and full pages, without backdating new work into the earlier cycle.

The aggregate failure warning retains the first registered error code, alongside its counts.

Astra's correction review reported no remaining material finding in this maintenance component.
This was source review under the requested extra-high configuration; the reviewer ran no tests.

Nine recovery scenarios cover mixed financial state, erasure, deadlines, competing passes, pending issuance, a blocked earlier fleet, malformed state and cancellation.
The backlog case creates 101 native unissued calls from distinct admitted sources; another fleet must recover beyond that entire busy page.
The exact production query's planner check covers both initial and continuation bounds.

The busy-issuer case proves contention safety; it does not independently mutation-test the fresh issuance/accounting recheck.

The initial live backlog case failed because earlier fixture inputs remained unsettled and correctly blocked later claims.
The fixture now completes each preceding claimed source; production input-receipt settlement remains pending.
The scoped recovery selection passed all nine scenarios after this correction; `/tmp/m236-maintenance-fair-focused-2.log` records nine passed and zero failed.

Astra's final fixture review found no material issue and confirmed that native FIFO guards remain unchanged.
The seeded source-completion state proves financial recovery; it does not establish production input settlement or the runner loop.

Removing the cycle cutoff made the complete backlog scenario fail at `(4, 3, 1, 0)` versus `(2, 1, 1, 0)`.
The mutation driver restored the exact original SQL bytes; `/tmp/m236-maintenance-cutoff-proof.log` records the result and restored digest.
`/tmp/m236-maintenance-cutoff-mutant.log` retains the assertion failure, not a compilation failure.

`agentsfleet_task_decision_recovery_attempts_total` has four bounded outcomes; the overdue histogram records candidate age beyond its original deadline.
A capturing-exporter scenario checks counts, age totals and operation latency under exact label sets.
Structured log checks verify aggregate severity/counts, expected contention and absence of fixture credentials or question text.

The component is verified locally; combined-main verification and pushed-head checks remain pending.

## Runner ending deadline and additional activity scope

Merge `0e31bc918` integrates main `1920ff345` and PR 802 without rewriting the feature branch.
The merge retains both provider dependencies and restores the tenant plane's required native task dependency.
The latest freshness check found main unchanged.

Shell and Git now bound their ending wait by command time plus shared kill/drain grace and named reply slack.
Compile-time assertions compare the default wait, maximum command and ending allowance with the existing lease/runtime/shutdown bounds.
Spawn retains its separate native reply deadline; no independent whole-turn deadline is claimed.

Missing endings retain partial output and return `interrupted`, with no exit code or claim of remote death.
Expiry cancels the existing executor link, interrupts pending calls and processes, and refuses later calls immediately.
Confirmed endings and ordinary caller cancellation preserve connection reuse.

Owned output snapshots make each read finite without copying buffered bytes or holding the channel lock during consumption.
Ready notifications consume Tokio cooperative budget; empty frames add no buffer entries.
The existing `Executor` trait and admission/lease path remain unchanged.

The warning `process_ending_deadline_exceeded` records safe identities and timeout durations.
Computed fields are bound before logging; existing tool-duration accounting records the returned failure.
No command, output, credential or unbounded metric label is added.

Astra's extra-high source review found indefinite draining and notification-loop starvation; both were corrected.
Its final native-link review found no material production issue and requested a one-poll test for immediate refusal.
That proof now uses `now_or_never`; source review ran no tests or kernel probes.

The initial silence regression failed before the guard; the cooperative-budget regression also failed at its expected assertion before budgeting.
Final native scenarios cover shell/Git silence, retained output, confirmed cleanup endings, cancellation, sibling processes, pending RPCs and post-loss refusal.
Snapshot scenarios cover concurrent later writes, missing endings, empty frames and retained terminal output.

`orly gate verify` passed again on the final runner checkpoint: formatting, workspace Clippy, dependency checks, prose and version synchronization.
Its final log is `/tmp/m236-runner-checkpoint-verify.log`; the earlier component log is `/tmp/m236-runner-native-verify.log`.

The three Rust shards passed 1,602 runner, 141 daemon and 3,400 library tests, totaling 5,143.
`/tmp/m236-runner-native-unit-all-3.log` records those results before the logging-field cleanup.
The aggregate command then failed on 29 analytics tests because the inherited environment disabled analytics.

`make test-coverage-all` then passed after unsetting only that inherited switch; no test or application source changed for this correction.
App 4,653, website 133, CLI 1,997 and design system 522 passed; the CLI retains 16 explicit live skips.
All four coverage gates reported 100%; `/tmp/m236-runner-native-ts-coverage.log` retains the result.

The final logging-field cleanup also passed the complete runner shard: 1,602 tests, exit zero.
`/tmp/m236-runner-final-unit.log` records that fresh result.

The clean `make test-integration-rustd` gate passed 1,513 regular and three exclusive tests; exit zero.
`/tmp/m236-runner-final-integration.log` records fresh schema/cluster reset, migration and both passing selections on the integrated source.

Disk pressure during integration was relieved with `cargo clean` in the separate, inactive coverage checkout.
It removed 3,658 cached build files and recovered 1.2 GiB; the active target and saved coverage reports were preserved.

Earlier attempts identified container preconditions: PID 1 left killed children unreaped, and the shell inherited a restrictive file-creation mask.
The unchanged process-group tests passed with `docker-init -s`; the complete Rust rerun used that reaper and `umask 022`.
No assertion, timeout, coverage floor or production sandbox policy was weakened.

Indy superseded the separate-PR request with “I want all this in this PR” and “no new PR”.
The runner correction and schedule activity remain in implementation PR 803; public pages remain in existing docs PR 241.
The separate signal-isolation audit remains pending, with no measured exploit or reviewed runtime mechanism.

The schedule activity specification records atomic ordinary/setup mutations, sync outcomes, queued fires, native run details and replay limits.
Astra found and closed missing recipe writes, deletion/expiry distinctions, offline visibility and run-link assumptions in the design.
It reported no remaining material specification finding; schedule implementation and its storage/browser proof remain pending.

The existing unsigned schedule-header boundary remains unresolved and separate from activity.
Architecture now states acceptance-time database provenance without claiming signed schedule/revision identity.
Other missing activity types remain inventory only, as Indy requested.

## Owner work-item status

| Requested work | Current state and evidence |
|---|---|
| Integrate merged main before shipping | PR 798/799 ancestry is retained. Merge `9641c3cb2` adds PR 800/main `bf3baa126`; merge `0e31bc918` adds PR 802/main `1920ff345`. Combined source passes final verification, unit selections and clean integration. Repeat the freshness check before each push. |
| Complete DT runtime | Schema, decision adapters/catalogue/assignment, owner-controlled questions, task reads and mixed-model settlement recovery are foundations. Native physical-call accounting passes unit/integration gates; full agent/lease integration, waits, publication and recovery remain incomplete. |
| Keep approved UI and communicate work | Inline task rows and saved criteria use native reads. Ten production-component browser checks passed, including the editable Review pull requests starter, typography, disclosure and short/mobile viewports; progress/controls and full runtime demonstration remain pending. |
| Align unfinished setup actions | `components/layout/DraftSidebarItem.tsx` uses shared grid columns: title, reachable Delete, then Resume. Desktop/mobile measurements, app lint and 100% coverage pass; the same geometry assertion fails against the previous component. Logs: `/tmp/m236-ui-polish-browser.log`, `/tmp/m236-resume-parent-red.log`. |
| Shorten fleet chat guidance | `SteerComposer.tsx` now reads “Change fleet defaults in Edit setup.” The existing action, hint and theme remain; controlled browser cases, app lint and 100% coverage pass. |
| Private R2 bucket and secure keys | Non-secret bucket metadata resolves to `agentsfleet-checkpoints-dev` in the accessible `ZMB_CD_DEV/cloudflare-r2` item. No key value is recorded here. The scoped encryption component passes 68 tests and Astra review; native storage, transfer, recovery and live bucket checks remain pending. |
| Performance, concurrency and Grafana | Task reads are bounded and observable; cursor paging avoids per-task queries. Financial recovery retains progress across busy pages and cancellation. Exporter and structured-log scenarios check recovery attempts, age, latency and bounded labels. No benchmark result or complete DT performance trace is claimed. |
| Architecture and companion docs | Astra reviewed all 39 architecture Markdown pages and 39 public Markdown/MDX pages. Owning corrections and setup/Current work guides are applied; merge `d97b373` adds docs main `920933d`. Command-recovery guides are pushed as `ed1e1ea`; local validation and both published CI workflows passed. [Docs draft #241](https://github.com/agentsfleet/docs/pull/241) stays open; new API pages follow the lead merge. |
| Runner hang and same-PR scope | Native ending guard, finite snapshots, cooperative waits and shared-link invalidation are implemented and reviewed. The final runner shard passed 1,602 tests; all Rust shards and all four 100% TypeScript gates have passing evidence. Clean integration passed 1,513 regular and three exclusive tests; final checkpoint verification passed. All changes stay in PR 803. |
| Schedule activity and other event inventory | The reviewed M236 workstream 003 covers readable changes/sync/fire/outcome rows and the saved-summary bug. Runtime implementation and storage/browser checks remain pending. Other event types are inventoried without implementation. |
| Orly lifecycle and PR merge | Local Orly 0.28.0 matches the pin. Indy requested the early review [implementation draft #803](https://github.com/agentsfleet/agentsfleet/pull/803). Native decision, crypto and bounded financial recovery have component evidence. Remaining implementation and full readiness gates stay pending; leave both PRs in draft. |
