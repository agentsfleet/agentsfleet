<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->
# M143_003: Library performance has privacy-safe causal evidence

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 003
**Date:** Jul 24, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — read improvements need attributable, cardinality-safe evidence and deterministic gates
**Categories:** Observability (OBS)
**Batch:** B3 — instruments M143_001 under M139_004 semantics
**Branch:** `feat/m143-library-performance-evidence`
**Test Baseline:** unit=3172 integration=446
**Depends on:** M143_001 — exact resource maxima. (M139_004 supplied the telemetry names, units, resources, and cardinality rules this workstream binds to; it merged in #555 and sits in `docs/v2/done/`, so it constrains the work without blocking it. M143_001 is the only live dependency.)
**Provenance:** LLM-drafted (Codex, Jul 24, 2026) from Oracle second-pass review
**Canonical architecture:** `docs/architecture/observability.md` §Traces and §Metrics

---

## Overview

**Goal (testable):** Operators attribute authenticated library latency and pool pressure using fixed-cardinality traces and sanitized comparable aggregates without timing-based CI failures.
**Problem:** Stage latency is opaque, pool behavior lacks bounded-progress proof, and evidence can become flaky or identity-bearing.
**Solution summary:** Extend M139_004 stages, exact M143_001 counters, deterministic failure/pool tests, and separate report-validation versus provisioned capture commands.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(observability): prove library read performance safely
- **Intent (one sentence):** Operators explain library latency without high-cardinality labels, secret leakage, scheduler overclaims, or latency gates.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `M143_001_P1_API_CLI_LIBRARY_DATA_SECURITY.md` §3 numeric maxima.
2. `docs/v2/done/M139_004_P1_OBS_TELEMETRY_SEMANTIC_CONVENTIONS.md` — binding semantics (merged; lives under `done/`).
3. `src/agentsfleetd/http/server.zig`, `route_trace.zig`, `observability/metrics.zig` — current ownership.
4. `make/bench.mk`, `make/test-unit.mk`, and `tests/bench/micro.zig` — existing lanes.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/agentsfleetd/http/server.zig`; `route_trace.zig`; trace tests | EDIT | Incoming W3C trace context and stages. |
| `src/agentsfleetd/observability/metrics.zig`; `metrics_render.zig`; tests; `observability/library_stages.zig` | EDIT/CREATE | Closed schema and privacy allow-list. |
| `src/agentsfleetd/db/pool.zig`; `db/pool_test.zig`; `db/test_fixtures.zig`; `http/test_harness.zig` | EDIT if seams change | Deterministic release/timeout/failure fixtures only when required. |
| `ui/packages/app/lib/api/client.ts` | EDIT | Valid traceparent propagation. |
| `tests/bench/micro.zig`; `make/bench.mk`; `make/test-unit.mk` | EDIT | Deterministic resource/pool lane and capture target. |
| `scripts/report-library-performance.ts` | CREATE | Aggregate report validation. |
| `docs/architecture/observability.md`; `docs/architecture/data_flow.md` | EDIT | Stage, privacy, pool, evidence-command truth. |
| `src/agentsfleetd/observability/metrics_otel_test.zig`; `observability/semantic_schema_test.zig`; `observability/otel_traces_test.zig`; `tests/fixtures/telemetry/otlp_metrics.json` | EDIT | One-to-one failure and artifact proof. |

**Scope grading.** Rubric R4 compares `git diff --name-only origin/main` against this table, so every cell is an exact path. A path that turns out to be genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

### Files Changed — amendments

| File | Action | Why |
|---|---|---|
| `src/agentsfleetd/observability/library_read_scope.zig` + its test | CREATE | The per-request telemetry lifecycle. §1 needs one outcome per request on every exit path; the handlers have nine to twelve each, and a call at each is the hand-placed-tally failure mode `library_read_counters.zig` already documents. Split from `library_stages.zig` at the length gate — schema and lifecycle are separate concerns. |
| `src/agentsfleetd/http/handlers/hx.zig` | EDIT | `db()` returned `?DbScope`, so a pool acquire failure reached the caller as a null with its reason erased. §2's pool evidence cannot distinguish a saturated pool from an unreachable datastore through a null. Now returns a named `DbAcquireError`. **Indy-approved in session** (see Discovery). |
| `handlers/fleets/create.zig`; `fleet_bundles/list.zig`; `library/onboard.zig`; `library/catalog_patch.zig`; `library/catalog.zig`; `schedules/api.zig` | EDIT | Mechanical consequence of the line above: `orelse <ret>` → `catch <ret>`, one line each, no behaviour change. Ten call sites that do not care about the distinction. |
| `src/agentsfleetd/http/handlers/model_library.zig` | EDIT | The global catalogue surface — stage instrumentation. Not foreseen because §Files-Changed named `server.zig` and `route_trace.zig` for the trace half, but the stages are per-handler. |
| `src/agentsfleetd/tests.zig` | EDIT | Test-root reachability for the two new modules. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD, FLL, UFS, FLS, CNX, TNM, NDC, NLR, NLG, ORP, VLT.
- **`dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, `dispatch/write_any.md`, `docs/LOGGING_STANDARD.md`, M139_004** — typed lifecycle, fixed schema, privacy.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| ZIG GATE / PUB | yes | focused enums/seams, allocator tests, Linux builds |
| File & Function Length | yes | isolated stage, report, and fixture modules |
| UFS | yes | stage/surface/outcome/unit/limit constants |
| UI Substitution / DESIGN TOKEN | no | no visual UI |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | logging/lifecycle | typed labels; close spans/buffers/connections |

## Prior-Art / Reference Implementations

- **Semantics:** M139_004; **trace:** existing Next/Zig context; **performance:** existing `make/bench.mk` extended, not duplicated.

## Sections (implementation slices)

### §1 — Closed traces and privacy-safe artifacts

Propagate valid W3C `traceparent`; malformed input starts a clean trace. Closed stages are `next_upstream`, `auth_verify`, `pool_wait`, `authorize`, `sql`, `secret_project`, `map`, `serialize`, `cache_revision`, `cache_lookup`. Closed surfaces are `tenant_models`, `global_models`, `fleet_summary` (**amended — §Discovery: `fleet_detail` is retired with the route M143_001 stripped**); outcomes are `ok`, `invalid`, `unauthorized`, `forbidden`, `not_found`, `timeout`, `cancelled`, `dependency_error`, `internal_error`; cache values are `hit`, `miss`, `bypass`, `stale`, `not_applicable`; pool results are `acquired`, `timeout`, `cancelled`, `error`. Permit only these enums and numeric duration/count/bytes. Prohibit authorization material, SQL/raw URL/query, free-form errors, identifiers, all M143_001 response metadata, secret values, API keys, and ciphertext in telemetry, observable cache keys, or evidence artifacts; M143_001's unlogged keyed selector digest remains permitted internally.

- **Dimension 1.1** — context and closed schema are exact → Test `test_library_trace_and_stage_schema`
- **Dimension 1.2** — labels/artifacts obey sink policy on every path → Test `test_library_evidence_is_secret_and_metadata_free`

### §2 — Deterministic resources and bounded pool progress

Consume M143_001's exact measured application-data maxima after middleware auth verbatim — by **importing the constants** `observability/library_read_counters.zig` exports, never by retyping their values here or in a test. That module is the single home for the table, so a ceiling can only move visibly, in one place.

**Amended (§Discovery — the drafted numbers are superseded by the measurement).** This paragraph was drafted before M143_001 measured its own reads and corrected them twice. The live table is: tenant registry ≤6 statements, **zero decryptions**, ≤100 results, 512 KiB, one connection; global model hit/miss ≤1/≤2 statements, zero decryptions, ≤100, 256 KiB, one connection; Fleet summary ≤3 statements, zero decryptions, ≤100, 512 KiB, one connection. Payload overflow is typed, never truncated.

For a controlled occupied slot, releasing it causes at least one queued request to progress. Every waiter either succeeds or receives the configured typed timeout, and all completion/cancellation/failure paths leave zero leaked connections. Do not claim an ordering policy or unbounded scheduler guarantee.

**The `secret_project` stage survives the decryption removal, and its meaning narrows.** M143_001 replaced per-row decryption on read paths with one batch presence query, so this stage now times presence resolution and projection rather than decryption. Keep the stage — it still isolates a real cost — and assert its decryption counter at zero rather than deleting the label, so a regression that reintroduces per-row decryption shows up as a stage that suddenly decrypts instead of as a stage that vanished.

- **Dimension 2.1** — counters enforce every exact maximum, including zero read-path decryptions → Test `test_library_deterministic_resource_gate`
- **Dimension 2.2** — release/timeout/cancel proves bounded progress and zero leaks → Test `test_pool_bounded_progress_and_timeout`

### §3 — Failure matrix

Each failure row in §Failure Modes has a unique deterministic fixture case, and each one proves that the injected fault leaves zero leaked connections, closed spans, and freed buffers.

Changed-backend branch coverage moved out of this workstream to **M143_004**. It is a coverage tool — a pinned Zig parser, an edge-identity scheme, a probe lane, a manifest checker, and a threshold — not a slice of library performance evidence, and leaving it here would give one agent a performance job and a tooling job on the same branch, where the tooling eats the evidence work. This workstream does not depend on it.

- **Dimension 3.1** — every failure injection cleans all owned resources → Test `test_library_failure_matrix_is_complete`

### §4 — Report validation is separate from provisioned capture

Required report check: `bun scripts/report-library-performance.ts --check --baseline test-results/library-performance/baseline.json --candidate test-results/library-performance/candidate.json`. Each file is exactly `{schema_version:1,commit_sha:string,metadata:{fixture_sha256:string,build_profile:string,database_version:string,pool_size:int,replica_count:int,region_class:"local"|"single_region"|"multi_region",warm_state:"cold"|"warm",concurrency:int},aggregates:Aggregate[]}`. `Aggregate={surface,stage,outcome,cache,pool_result,sample_count:int,p50_seconds:number,p95_seconds:number,p99_seconds:number,payload_bytes:int}` using §1 enums. Comparable runs require byte-equal metadata and identical aggregate key tuples `(surface,stage,outcome,cache,pool_result)`; commit differs. Counts are positive, values nonnegative, and `p50≤p95≤p99`; timing/payload values never decide pass/fail.

Add the distinct documented provisioned-environment command `make capture-library-performance BASELINE_REF=origin/main CANDIDATE_REF=HEAD` outside universal CI. Capture may fail for setup, execution, schema, sanitization, or output correctness, never because p50/p95/p99 changed. The generic benchmark target is not P0 evidence.

- **Dimension 4.1** — explicit aggregates validate comparability without value thresholds → Test `test_library_performance_report_validation`
- **Dimension 4.2** — capture command is provisioned-only and value-neutral → Test `test_library_capture_command_is_not_universal_gate`

## Interfaces

`LibraryObservation={surface:closed,stage:closed,outcome:closed,cache:closed,pool_result:closed,duration_seconds,count?,bytes?}`.
`traceparent`: W3C input; invalid ignored. Evidence: sanitized aggregate JSON only. No public Server-Timing or new browser real-user monitoring event.

## Failure Modes

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| Malformed trace | invalid header | malformed fixture | clean root, no echo | `test_library_trace_malformed_case` |
| Metric rejection | recorder rejects | rejecting sink | request unchanged; bounded loss | `test_library_metric_rejection_case` |
| Allocation/serialization | owned stage fails | failing allocator/encoder | typed error; span/buffers close | `test_library_allocation_case` |
| SQL/revision/decrypt | dependency fails | per-stage failpoint | typed outcome; cleanup | `test_library_dependency_failure_case` |
| Pool timeout/cancel | slot remains occupied/waiter aborts | barrier/clock/cancel fixture | success or typed timeout; zero leaks | `test_pool_bounded_progress_and_timeout` |
| Next cancellation | navigation abort | AbortController fixture | cancelled span; no rejection | `test_library_next_cancel_case` |
| Incomparable report | metadata differs | mismatched aggregates | nonzero, names field | `test_library_performance_report_validation` |
| Prohibited artifact | unsafe field/sentinel | sink/report fixture | reject emission/report | `test_library_evidence_is_secret_and_metadata_free` |

## Invariants

1. Typed builders limit cardinality and reject identifiers/metadata.
2. Counter tests consume, not reinterpret, M143_001 maxima.
3. Pool accounting reaches zero and every waiter terminates successfully or by configured timeout.
4. CI checks report structure and comparability, never latency values.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| library stage duration | operations | stage completes | closed enums + duration | no raw strings/IDs/metadata | `test_library_trace_and_stage_schema` |
| pool wait/timeout | operations | acquire completes | closed outcome + duration/count | no tenant/request label | `test_pool_bounded_progress_and_timeout` |
| cache outcome | operations | revision/cache decision | closed outcome | global cache only | `test_library_evidence_is_secret_and_metadata_free` |

## Test Specification (tiered)

This table is the complete set. Every row is mandatory, including the failure rows — an agent that implements only the dimension rows ships an incomplete workstream.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | integration | `test_library_trace_and_stage_schema` | valid/malformed context and enum-only serialization |
| 1.2 | integration | `test_library_evidence_is_secret_and_metadata_free` | all sinks/artifacts reject sentinels and metadata |
| 2.1 | integration | `test_library_deterministic_resource_gate` | every M143_001 maximum and overflow behavior |
| 2.2 | integration | `test_pool_bounded_progress_and_timeout` | one progresses on release; all terminate; zero leaks |
| 3.1 | integration | `test_library_failure_matrix_is_complete` | every named fixture executes and cleans up |
| 4.1 | integration | `test_library_performance_report_validation` | explicit comparable paths; values informational |
| 4.2 | unit | `test_library_capture_command_is_not_universal_gate` | capture absent from universal CI and value-neutral |
| — | integration | `test_library_trace_malformed_case` | an invalid `traceparent` starts a clean root and is never echoed back |
| — | integration | `test_library_metric_rejection_case` | a rejecting recorder leaves the request result unchanged and bounds the loss |
| — | integration | `test_library_allocation_case` | a failing allocator or encoder returns a typed error with span and buffers closed |
| — | integration | `test_library_dependency_failure_case` | per-stage SQL, revision, and decrypt failpoints map to their typed outcome and clean up |
| — | integration | `test_library_next_cancel_case` | a navigation abort produces a `cancelled` span and no unhandled rejection |
| — | unit | `test_library_stage_enum_is_closed` | every stage, surface, outcome, cache, and pool value is one of the §1 enums, and a free-form string fails to compile or is rejected at the builder |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Telemetry/resource/pool tests pass | `make test-unit-all && make test-integration` | exit 0 | P0 | |
| R3 | Explicit aggregate report is valid | `bun scripts/report-library-performance.ts --check --baseline test-results/library-performance/baseline.json --candidate test-results/library-performance/candidate.json` | exit 0 and `comparison=valid`; values do not gate | P0 | |
| R4 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | |
| S1 | Lint/conform/build | `make lint-all && make harness-verify && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S2 | Memory/secrets | `make memleak && gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line. Capture is documented evidence outside universal CI.

## Dead Code Sweep

No file deletion. Replaced stage names get root-wide zero-match checks; no aliases or dual emission.

## Out of Scope

- M143_001 API/data and M143_002 UI/session implementation.
- Public Server-Timing, browser real-user monitoring expansion, authentication/proxy/token redesign, universal latency gates.

---

## Product Clarity (authoring record)

1. **Successful user moment** — one trace/report identifies a bounded slow stage.
2. **Preserved user behaviour** — HTTP, auth, model/Fleet, CLI, and UI behavior.
3. **Optimal-way check** — stages plus deterministic resources separate regressions from noise.
4. **Rebuild-vs-iterate** — extend M139_004 and existing test/bench lanes.
5. **What we build** — trace schema, sink guard, bounded pool proof, aggregate report.
6. **What we do NOT build** — raw requests, IDs, metadata artifacts, timing gates, scheduler guarantees.
7. **Fit with existing features** — M139_004 semantics and M143_001 counters.
8. **Surface order** — operator evidence follows API implementation.
9. **Dashboard restraint** — no panel until fixed-cardinality signal exists.
10. **Confused-user next step** — report names mismatched metadata; trace names coarse stage.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** tracing/privacy, deterministic proof, and capture/report are separate; branch-coverage tooling is its own workstream (M143_004).
- **Alternatives considered:** raw identifiers and percentile gates violate privacy/reproducibility.
- **Patch-vs-refactor verdict:** **refactor** of observability ownership around M139_004.

## Discovery (consult log)

- **Consults** — Oracle second-pass blockers incorporated exactly.
- **Metrics review** — operational signals only; browser analytics unchanged.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.

### §2's maxima were drafted, not measured — three corrections at PLAN

This workstream's whole premise is that it *consumes* M143_001's numbers rather
than restating them (Invariant 2). §2 as drafted restated them, and the restatement
was already stale when it was written: M143_001 measured its own reads during
implementation and corrected the table twice, in its own §Discovery. Verified at
PLAN against `observability/library_read_counters.zig` on `origin/main`:

| §2 as drafted | Shipped constant | Why it moved |
|---|---|---|
| tenant registry ≤4 statements | `TENANT_REGISTRY_MAX_STATEMENTS = 6` | 4→5 when the page's fifth load-bearing statement was counted, 5→6 when the rate batch replaced a resident-cache read that returned null for every row after a restart |
| Fleet summary ≤1 statement | `FLEET_SUMMARY_MAX_STATEMENTS = 3` | `common.authorizeWorkspace` costs two statements and runs *inside* the measured window — the bearer chain authenticates, but only the handler knows which workspace the path names |
| Fleet detail ≤2 statements, 1 result, 1 MiB | *(no constant)* | the route was stripped unconsumed; `FLEET_DETAIL_MAX_*` left the module with it |

The resolution is the one §2 already asks for and the drafted prose undercut: the
tests **import** the constants. No number from this table is spelled in a test or
in this spec's assertions, so a ceiling that moves again moves in one visible
place and this spec cannot go stale against it a third time.

### `fleet_detail` leaves the closed surface enum

M143_001 stripped the Fleet detail route unconsumed — `handlers/library/gallery_detail.zig`
is deleted, its matcher and route registration are gone, and its integration
suite pins the former URL to no-route. A `fleet_detail` value in this
workstream's closed surface enum would therefore be an enum member no producer
can ever emit: dead code at authoring time (RULE NDC), and a label that a
dashboard would render as a permanently empty series.

Dropping it is not a scope reduction — there is no surface left to instrument.
§1's enum and §2's fourth row go together, and §4's `Aggregate` key tuple narrows
with them because it is defined in terms of §1's enums.

### `Hx.db()` lost the acquire reason, and §2 needs it

Raised with Indy during EXECUTE as a quality-ceiling item and approved in
session: *"Yes take that and fix it."*

`Hx.db()` returned `?DbScope` and wrote its own error response, so a failed pool
acquire reached the caller as a bare null. Two very different operator problems
arrive that way — a **saturated pool**, whose answer is capacity, and an
**unreachable datastore**, whose answer is the datastore — and a null cannot
tell them apart. §2's pool evidence is specifically about that distinction, so
the null was load-bearing in the wrong direction.

`db()` now returns `DbAcquireError!DbScope` with `PoolTimeout` and
`PoolUnavailable`. Deliberately a CHANGED signature rather than a second
`dbOrError()` beside it: a second spelling of one operation is exactly the
compatibility shim the operating model forbids, and the call sites that do not
care still cost one line (`orelse return` → `catch return`).

The reason this matters beyond tidiness: the tenant registry handler had already
worked around the missing distinction by acquiring from the pool DIRECTLY, which
meant reimplementing the acquire/release pairing `DbScope` exists to make
unskippable. That workaround is now removed — the handler is back on `hx.db()`
and the release discipline lives in one place again.

Proof is deferred to §2's `test_pool_bounded_progress_and_timeout`, which is the
only tier that can saturate a real pool; a unit test cannot produce
`error.Timeout` from `pg.Pool` without one.

### Stage timings are metrics, not spans — the span budget decides it

`http/route_trace.zig` admits at most **10 generic request spans per monotonic
second** (four runner rejections, four server errors, two sampled successes), and
`docs/architecture/observability.md` §Traces states that budget as the shipped
routing policy. Ten stage spans per library request would consume a whole
second's admission on one request, evicting exactly the server-error spans the
budget exists to protect.

So §1's trace half is the *ingress context* — `traceparent` in,
`http.request` span out — and the stage timings are metric observations under
§Metrics & Observability. `handlers/common.zig:196` `resolveTraceContext` already
parses W3C, takes `.child()` on a valid header, and falls back to `.generate()`
on a malformed one, so Dimension 1.1's context half is **verified**, not built.

### Three metrics with disjoint labels, not one five-label observation

`LibraryObservation` in §Interfaces carries five closed enums. Emitting it as one
metric point with all five labels is a cross-product:
`surface(3) × stage(10) × outcome(9) × cache(5) × pool_result(4) = 5400` series —
against a Prometheus registry whose existing families are fixed in-memory state,
and 21× the OTLP aggregator's `MAX_SERIES = 256` had it gone there instead.

§Metrics & Observability already prescribes the split: three rows, each with its
own *Properties allowed* cell, and the pool row explicitly forbidding a
tenant/request label. Implemented on those lines the budget is fixed at authoring
time: stage duration `surface × stage = 30`, read outcome `surface × outcome = 27`,
pool result `4`, cache outcome `5` — **66 series**, none of which vary with
tenant, request, or content. `LibraryObservation` stays the builder's input
struct and fans out; it is not itself a label set. This is what Invariant 1
("typed builders limit cardinality") requires rather than merely permits.
