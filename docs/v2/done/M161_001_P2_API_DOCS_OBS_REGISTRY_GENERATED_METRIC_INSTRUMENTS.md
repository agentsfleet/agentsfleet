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

# M161_001: Metric instruments generate from the family registry; samples shrink and the aggregator goes constant-time

**Prototype:** v2.0.0
**Milestone:** M161
**Workstream:** 001
**Date:** Aug 11, 2026
**Status:** DONE
**Priority:** P2 — internal quality: same wire output, less code, less memory, fewer places a new family can drift
**Categories:** API, DOCS, OBS
**Batch:** B1 — §1→§2 sequential; §3/§4 follow §2 and run concurrently; §5→§6 follow §2; §7 last
**Branch:** feat/m159-otlp-runtime-metrics (folds into open PR #597 — continuation of M159/M160)
**Test Baseline:** unit=3530 integration=588
**Depends on:** M161 continues M159_001 (the closed family registry this milestone generates from; in `docs/v2/done/`)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 11, 2026) from a post-REVIEW refactor assessment of the M159 diff, grounded in source on this branch
**Canonical architecture:** `docs/architecture/observability.md` §Metric family census

---

## Overview

**Goal (testable):** every metric family keeps a byte-identical wire shape while its storage, writer, snapshot, and flush-time collection are generated at comptime from the family registry — with `@sizeOf(Sample)` at most 128 bytes and aggregator insertion via a hash probe instead of a linear scan.

**Problem:** one metric family is hand-spelled up to five times on its way to the wire — a named global atomic, a `Snapshot` field, a `snapshot()` load line, a hand-written collect mapping, and its registry row. Two collect mappings bind values to labels by array position, protected only by a comment ("field order below must match") and an after-the-fact census test. Each sample carries ~490 bytes of inline label buffers, so the 1024-slot ring holds ~0.5 MB and every flush builds a several-hundred-KB accumulator array on the flush thread's stack, then matches samples against it by linear scan with per-label memcmp.

**Solution summary:** the family registry (`otel_metrics_families.zig`) grows a per-family label-dimension declaration, and a new comptime instrument layer generates the atomic storage cells, a typed writer, the snapshot reads, and the flush-time collection loop from it. The named-atomic source files collapse to one-line writer wrappers over the layer (public signatures unchanged, so nothing outside `observability/` moves). Sources that read live state instead of module atomics (Redis pool, resident-set-size probe, streamed per-runner slot table) become explicit collect hooks. Labels intern their keys and closed-enum values to comptime indices — only one dynamic value per family remains inline — shrinking `Sample` at least 4×, and the aggregator replaces its linear scan with a fixed open-addressed hash. Every family name, unit, label set, per-metric attribute order, value spelling, and drop semantics stay byte-identical, and the census and egress guard tests pass without a single edit. (Amended at REVIEW: the metric OBJECTS inside one envelope now serialize in registry declaration order with live-read hooks last, instead of the retired collector's hand-written call order — the OTLP metrics array is unordered, so no consumer distinguishes the two; each object's bytes are unchanged.)

## PR Intent & comprehension handshake

- **PR title (eventual):** folds into open PR #597; fold commit: `refactor(obs): generate the metric instrument layer from the family registry`
- **Intent (one sentence):** the exporter keeps exactly the wire behaviour M159 shipped while a new family becomes one registry row instead of five hand-synced spellings, and the hot path costs 4× less memory and O(1) aggregation.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.
  - **Restatement (Orly, PLAN):** the wire — every family name, unit, kind, label set, temporality, envelope byte, and drop rule M159 shipped — does not move at all (census + egress suites pass unedited as proof); underneath it, one comptime instrument layer generated from the family registry replaces the five hand-synced spellings a family needs today, so adding a family becomes one registry row plus one typed writer call, samples carry interned labels at ≤128 bytes instead of ~490, and the aggregator finds a series by hash probe instead of linear scan.
  - **ASSUMPTIONS I'M MAKING:** (1) module `Snapshot`/`snapshot()`/`resetForTest` surfaces in `metrics_counters/memory/trace/otel/sensitive_memory` are *regenerated* over the cells, not deleted — fleet integration tests and sibling test files outside Files Changed consume them, and the diff must stay inside the table; (2) the enum-typed dimension declaration is comptime-only (runtime `MetricMeta` cannot carry `type` fields), `MetricMeta` gaining only derived `max_series` plus a live-read marker; (3) `worker_running`, `redis_pool_*`, and `process_resident_memory_bytes` are live-read hook families with no generated cells, preserving absence semantics and the constant-1 liveness gauge; (4) hooks pass to `collect` as an explicit comptime slice owned by `otel_metrics_runtime.zig` — no mutable registration; (5) model (cost) and runner_id (streamed) are the only dynamic label values — every other value is a closed set, confirmed from source; (6) the collision test discovers colliding identities by computing buckets deterministically.
  - **FILE SHAPE DECISION:** `otel_instruments.zig` — conventional layout; operations-over-value: free functions over comptime-generated process-global atomic cells, no primary struct with identity (same shape as `metrics_otel.zig` / `library_stages.zig`).

## Implementing agent — read these first

1. `src/agentsfleetd/observability/otel_metrics_families.zig` — the closed registry and its comptime ceiling arithmetic; the label-dimension declaration lands beside `MetricMeta`, and every generated artifact derives from this table.
2. `src/agentsfleetd/observability/metrics_otel.zig` and `library_stages.zig` — the repo's existing enum-indexed atomic-array pattern (`labelsOf`, flat cell arrays); the instrument layer generalizes exactly this, not a foreign design.
3. `src/agentsfleetd/observability/otel_metrics_runtime.zig` — the hand-written collect mappings and order-pairing arrays this milestone deletes; the streamed per-runner appender at the bottom stays.
4. `src/agentsfleetd/observability/otel_metrics_payload.zig` + `otel_metrics_aggregate.zig` — the `Sample`/`Label` layout being compacted and the linear-scan `add` being hashed; the serializers below them must not change observable output.
5. `src/agentsfleetd/observability/otel_metrics_census_test.zig` — the double-entry census pinning family literals in both directions; this file is deliberately NOT in Files Changed — it passing unedited is the proof the wire didn't move.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/observability/otel_instruments.zig` | CREATE | comptime-generated storage cells, typed writer, snapshot reads, and registry-driven collect loop |
| `src/agentsfleetd/observability/otel_metrics_dims.zig` | CREATE | (amended at EXECUTE) the registry's label-dimension declarations + interned key/value tables, split from families/payload under the 350-line file gate |
| `src/agentsfleetd/observability/otel_instruments_test.zig` | CREATE | typed-writer cell binding, generated collect coverage, threaded no-lost-increment hammer |
| `src/agentsfleetd/observability/otel_metrics_families.zig` | EDIT | `MetricMeta` gains the label-dimension declaration; operator help prose migrates here as doc comments |
| `src/agentsfleetd/observability/otel_metrics_runtime.zig` | EDIT | hand-written collect mappings and order-pairing arrays replaced by the generated loop + explicit source hooks; streamed appender stays |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | writers become one-line wrappers; named atomics, `Snapshot`, `snapshot()`, `_HELP` consts, stale Prometheus prose deleted |
| `src/agentsfleetd/observability/metrics_memory.zig` | EDIT | same collapse |
| `src/agentsfleetd/observability/metrics_trace.zig` | EDIT | same collapse; suppression-reason enum stays the label source |
| `src/agentsfleetd/observability/metrics_sensitive_memory.zig` | EDIT | same collapse; resident-set-size probe stays a live-read hook |
| `src/agentsfleetd/observability/metrics_otel.zig` | EDIT | signal/discard/omission enums stay; its array storage migrates onto the generated cells |
| `src/agentsfleetd/observability/metrics_fleet.zig` | DELETE | 54-line single counter; its writer wrapper moves to `metrics_counters.zig`, which already re-exports it |
| `src/agentsfleetd/observability/library_stages.zig` | EDIT | 2-D atomic arrays migrate onto generated cells; `observe*` public API and label enums stay |
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | `Label` interns key + closed-enum value to indices; one inline dynamic value; `Sample` size comptime-bounded |
| `src/agentsfleetd/observability/otel_metrics_aggregate.zig` | EDIT | fixed open-addressed hash over sample identity replaces the linear scan; drop semantics unchanged |
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | attribution helpers adapt to the compact label form; record API signatures unchanged |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | EDIT | serialization assertions unchanged; construction sites adapt to compact labels |
| `src/agentsfleetd/observability/otel_metrics_aggregate_test.zig` | EDIT | adds hash-collision and table-full negative cases |
| `src/agentsfleetd/observability/metrics_counters_test.zig` | EDIT | exercises wrappers through the generated snapshot |
| `src/agentsfleetd/observability/library_stages_test.zig` | EDIT | same migration |
| `src/agentsfleetd/observability/otel_metrics_attribution_test.zig` | EDIT | construction sites adapt; omission-counting assertions unchanged |
| `src/agentsfleetd/observability/otel_metrics_window_test.zig` | EDIT | construction sites adapt |
| `src/agentsfleetd/observability/library_failure_matrix_test.zig` | EDIT | construction sites adapt |
| `src/agentsfleetd/tests.zig` | EDIT | registers the new instrument test file |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | (amended at EXECUTE) two tests imported `metrics_fleet` directly; the Dead Code Sweep's `metrics_fleet` grep forces the references onto `metrics_counters.snapshot()` |
| `src/agentsfleetd/http/handlers/webhooks/fleet.zig` | EDIT | (amended at EXECUTE) inline test block used `metrics_fleet`; same forced migration |
| `src/agentsfleetd/observability/otel_metrics_flush_test.zig` | EDIT | (amended at EXECUTE) one construction site builds per-index model labels — adapts to `setDynamicLabel`, semantics unchanged |
| `src/agentsfleetd/observability/metrics_trace_test.zig` | EDIT | (amended at REVIEW) adds the literal wire pin for the five suppression reason labels — the deleted `SUPPRESSION_REASON_LABELS` list was the only literal freeze |
| `docs/architecture/concurrency.md` | EDIT | (amended at REVIEW) drops the thread-inventory row pointing at deleted `metrics_fleet` — the row was stale before this fold (no such thread ever existed there) |
| `src/agentsfleetd/observability/metrics_runner.zig` | EDIT | (amended at REVIEW) `ID_LEN` goes pub for the new comptime tie `ID_LEN <= MAX_LABEL_VAL` in the streamed appender |
| `docs/architecture/observability.md` | EDIT | documents the instrument layer as the single storage/collection shape behind the census |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (delete every orphaned `Snapshot` struct, `snapshot()`, `_HELP` const, `collect*` fn, `push*` helper in the same diff), NLR (stale "Prometheus text format" module docs fixed on touch), ORP (deleted `metrics_fleet.zig` + removed pub symbols swept repo-wide), UFS (bucket/size bounds and probe caps are named constants), FLL (the instrument layer stays ≤350 lines; comptime generation is the mechanism), PUB (new pub surface limited to the typed writer + snapshot + collect entry points), FSD (instrument layer file-shape verdict recorded at PLAN), TST-NAM (milestone-free test names), MSID (no M161 tokens in source), DEINIT/DIDEM (no new init/deinit pairs — storage is static; test resets stay idempotent).
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — comptime assertion discipline, atomics ordering rationale comments, cross-compile both linux targets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all edits are `*.zig` | memory-safety pass per file; cross-compile x86_64-linux + aarch64-linux |
| PUB / Struct-Shape | yes — new `otel_instruments.zig` pub surface | FILE SHAPE DECISION at PLAN; writer/snapshot/collect only, storage stays private |
| File & Function Length (≤350/≤50/≤70) | yes | generation replaces enumeration; if the layer nears 350, split storage vs collect |
| UFS (repeated/semantic literals) | yes | `SAMPLE_SIZE_BOUND`, probe-limit, and table-size constants named in one place |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no log lines, no lifecycle stages, no error registry rows, no schema files touched |
| UI Substitution / DESIGN TOKEN | no | no UI files touched |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/observability/metrics_otel.zig` + `library_stages.zig` — enum-indexed flat atomic cell arrays with comptime-derived label tables; the instrument layer is this pattern promoted to registry-wide generation. Aggregator hashing mirrors the fixed open-addressed tables already used in `metrics_runner.zig`'s slot table. Not greenfield.

## Sections (implementation slices)

### §1 — The registry declares label dimensions

`MetricMeta` gains a declaration of the closed label enum(s) that dimension each fixed-label family (key string + enum type per dimension; empty for unlabelled families; a marker for the at-most-one dynamic dimension carried by cost and per-runner families). `max_series` for fixed-label families becomes derived from the declared dimension product instead of hand-multiplied at each call site. Operator help prose stranded in dead `_HELP` constants moves into the registry as doc comments on the family rows, so the operator knowledge survives the sweep.

- **Dimension 1.1** — every fixed-label family's `max_series` equals its declared dimension product, comptime-asserted → Test `test_registry_dimension_product_matches_max_series` — **DONE**
- **Dimension 1.2** — a family declaring more than one dynamic dimension fails the build → Test `test_registry_refuses_second_dynamic_dimension` (comptime negative fixture) — **DONE**

### §2 — The instrument layer generates storage, writer, snapshot, collect

New `otel_instruments.zig`: a flat atomic cell table sized at comptime as the sum of every fixed-label family's dimension product, with per-family comptime offsets; a typed writer (`inc`/`add`/`set` taking the family id comptime and a typed label struct, so a wrong or missing dimension is a compile error); snapshot reads; and a collect loop that walks the registry emitting one sample per cell into the aggregator. The order-pairing arrays and every hand-written `collect*` function in `otel_metrics_runtime.zig` are deleted — value-to-label binding becomes typed and comptime, impossible to misbind. Atomics keep today's orderings (`monotonic` add, `release` store / `acquire` load).

- **Dimension 2.1** — a typed write to any (family, labelset) is read back by snapshot and emitted by collect under exactly that family and label values → Test `test_instrument_cell_binding_roundtrip` — **DONE**
- **Dimension 2.2** — concurrent writers lose no increments: N threads × M adds per cell snapshot to exactly N×M → Test `test_instrument_hammer_no_lost_increments` — **DONE**
- **Dimension 2.3** — generated collect emits zero-valued cells for fixed-label families (dashboards stay live between increments), preserving today's behaviour → Test `test_collect_emits_zero_cells` — **DONE**

### §3 — Named-atomic sources collapse to wrappers

`metrics_counters.zig`, `metrics_memory.zig`, `metrics_trace.zig`, `metrics_sensitive_memory.zig`, `metrics_otel.zig`, and `library_stages.zig` keep their public writer functions as one-line wrappers over the typed writer; their named atomics, `Snapshot` structs, `snapshot()` functions, and test-reset bodies are deleted or regenerated. `metrics_fleet.zig` is deleted outright; its single wrapper lands in `metrics_counters.zig`. No call site outside `observability/` changes.

- **Dimension 3.1** — every pre-existing writer signature compiles unchanged at its call sites → Test: the full unit suite passes with zero edits outside `observability/` and the listed test files — **DONE**
- **Dimension 3.2** — signup-failure and trace-suppression reason cells bind by enum field, with per-reason writes landing on the declared reason label → Test `test_reason_labels_bind_by_enum_not_order` — **DONE**

### §4 — Live-read sources become explicit collect hooks

Sources that cannot be module atomics — the Redis pool snapshot (absent until a pool registers), the resident-set-size probe (absent when the platform cannot report), and the streamed per-runner slot table — register as explicit hooks the collect loop invokes after the generated cells. Their absence semantics are preserved exactly (no fake zeros).

- **Dimension 4.1** — no registered pool → no `redis_pool_*` series that window; registered pool → all eight series present → Test `test_pool_hook_absent_until_registered` — **DONE**
- **Dimension 4.2** — streamed per-runner families keep their shed/rollback budget behaviour byte-for-byte (existing streaming tests pass unedited) → Test: existing `otel_metrics_runtime` streaming tests green without modification — **DONE**

### §5 — Samples intern their labels

`Label` stores a comptime key index and, for closed-enum values, a comptime value index; the single dynamic value a family may carry (model, runner identifier) stays an inline buffer. `@sizeOf(Sample)` is comptime-asserted ≤ `SAMPLE_SIZE_BOUND` = 128 bytes. Serialization resolves indices back to the same strings, so wire bytes are unchanged. Attribution omission counting (unmappable provider, over-long model) is untouched.

- **Dimension 5.1** — `@sizeOf(Sample) <= 128` enforced at comptime → Test: build fails if violated; `test_sample_size_bound` pins the constant — **DONE**
- **Dimension 5.2** — a serialized envelope from interned labels is byte-identical to the pre-refactor expectation for the same inputs → Test: existing serialization assertions in `otel_metrics_test.zig` pass with unchanged expected strings — **DONE**
- **Dimension 5.3** — over-long dynamic values are dropped and counted exactly as today → Test: existing `otel_metrics_attribution_test.zig` assertions pass unchanged — **DONE**

### §6 — The aggregator goes constant-time

`Aggregator.add` locates a series by fixed open-addressed hash over the sample's identity (family id + interned indices + dynamic bytes) with a bounded probe; table capacity stays derived from `MAX_SERIES`. A full table drops and counts the sample exactly as the linear version did — same observable, new cost profile.

- **Dimension 6.1** — two identity-distinct samples that collide in hash bucket both aggregate correctly → Test `test_aggregator_collision_probe` — **DONE**
- **Dimension 6.2** — table saturation drops + counts, surfacing through `samples_dropped`, identical to pre-refactor behaviour → Test `test_aggregator_full_drops_and_counts` — **DONE**
- **Dimension 6.3** — same-identity samples still coalesce into one series per window across all three kinds (sum adds, gauge last-wins, histogram buckets) → Test: existing aggregate tests pass with construction-site-only edits — **DONE**

### §7 — Dead-code sweep and documentation

Every `_HELP` constant, orphaned `Snapshot` struct, `snapshot()` body, `collect*` function, `push*` helper, and stale "Prometheus text format" module doc goes in the same diff (RULE NDC/NLR). `docs/architecture/observability.md` documents the instrument layer as the single storage/collection shape behind the census.

- **Dimension 7.1** — zero `_HELP` symbols and zero order-pairing comments remain under `src/agentsfleetd/` → Test: Dead Code Sweep greps below return 0 matches — **DONE**
- **Dimension 7.2** — architecture doc names the instrument layer in the same commit as the code → Test: census test's architecture-doc cross-check stays green — **DONE**

## Interfaces

```
Registry (otel_metrics_families.zig):
  MetricMeta gains a label-dimension declaration (key + closed enum per
  dimension; at most one dynamic dimension). Existing fields, ceiling
  arithmetic, and comptime asserts keep their meaning.

Instrument layer (otel_instruments.zig, new):
  inc/add(comptime id, typed labels, delta) · set(comptime id, typed labels, value)
  snapshotCell(comptime id, typed labels) u64          — test/read surface
  collect(*Aggregator) void                            — generated cells + registered hooks
  Wrong family/dimension/enum = compile error.

Unchanged public surfaces (must not move without amending this spec):
  every existing pub writer fn signature in metrics_*.zig / library_stages.zig;
  otel_metrics.zig record API (recordRunSettlement etc.); payload.Series and
  the serializer entry points; the OTLP envelope bytes; every family name,
  unit, kind, temporality, and label set on the wire.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Aggregator table saturated | more distinct identities than `MAX_SERIES` in one window | sample dropped + counted; surfaces as `samples_dropped` (unchanged observable) |
| Hash bucket collision | distinct identities, same bucket | bounded open-address probe finds/creates the right series; no cross-series merge |
| Dynamic value over bound | model or runner id longer than the inline buffer | label omitted + omission counted, never truncated (unchanged) |
| Snapshot during concurrent writes | flush thread reads while writers add | monotonic per-cell reads; totals never torn or lost |
| Live-read hook absent | no Redis pool registered / platform lacks resident-set-size | family absent that window, never a fake zero (unchanged) |
| Writer before exporter install | boot-time increment | cell write lands harmlessly; evented record API keeps its no-op-when-uninstalled guard |

## Invariants

1. Registry↔storage bijectivity — every fixed-label family has exactly its dimension-product cells; enforced by comptime generation from one table (no hand mapping left to drift).
2. At most one dynamic dimension per family — comptime assert over the registry declarations.
3. `@sizeOf(Sample) <= SAMPLE_SIZE_BOUND` (128) — comptime assert in `otel_metrics_payload.zig`.
4. Ceiling arithmetic unchanged — `COST_SERIES_BUDGET = 256`, `MAX_SERIES <= AGGREGATOR_HARD_CAP`, streamed-histogram refusal: existing comptime asserts survive verbatim.
5. Value-to-label binding is typed — a reason/outcome value physically cannot attach to the wrong label (compile error), replacing the comment-enforced order pairing.
6. Wire freeze — `otel_metrics_census_test.zig` and `otel_metrics_egress_test.zig` pass with zero edits; any needed edit to either file means the wire moved and the diff is wrong.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | every family, name, unit, label set, and drop semantics stays byte-identical on the wire; this milestone changes how series are stored and collected, not what is exported | — | — | census + serialization suites unedited |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_registry_dimension_product_matches_max_series` | for each fixed-label family: declared enum product == `metaFor(id).max_series` |
| 1.2 | unit | `test_registry_refuses_second_dynamic_dimension` | negative comptime fixture: two dynamic dims → build refusal (pattern: existing streamed-histogram refusal fixture) |
| 2.1 | unit | `test_instrument_cell_binding_roundtrip` | write (family, labels, 7) → snapshot reads 7 → collect emits one sample with those exact label strings |
| 2.2 | unit | `test_instrument_hammer_no_lost_increments` | 8 threads × 10k adds on the same cell → snapshot == 80k, deterministic |
| 2.3 | unit | `test_collect_emits_zero_cells` | untouched fixed-label family → collect still emits its zero-valued cells |
| 3.1 | integration | full suite | zero edits outside `observability/` + listed tests; `make test-unit-all` green |
| 3.2 | unit | `test_reason_labels_bind_by_enum_not_order` | increment one enum reason → only that reason's cell moves; all others zero |
| 4.1 | unit | `test_pool_hook_absent_until_registered` | no pool → 0 `redis_pool_*` samples; registered → 8 |
| 4.2 | unit | existing streaming tests | shed/rollback + overflow-series behaviour green, unedited |
| 5.1 | unit | `test_sample_size_bound` | `@sizeOf(Sample) <= 128`; constant pinned so shrink regressions surface |
| 5.2 | unit | existing serialization tests | expected envelope strings unchanged in `otel_metrics_test.zig` |
| 5.3 | unit | existing attribution tests | over-long model dropped + omission counted, unchanged |
| 6.1 | unit | `test_aggregator_collision_probe` | crafted colliding identities → two distinct series, correct values |
| 6.2 | unit | `test_aggregator_full_drops_and_counts` | `MAX_SERIES`+1 identities → last dropped, `dropped == 1`, exported via `samples_dropped` |
| 6.3 | unit | existing aggregate tests | sum/gauge/histogram folding semantics green |
| 7.1 | unit | Dead Code Sweep greps | 0 matches (table below) |
| 7.2 | unit | census architecture cross-check | doc section present; census test green unedited |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Wire frozen: census + egress guards pass unedited (§5, §6) | `git diff --name-only origin/main...HEAD \| grep -E 'census_test\|egress_test'` | no output | P0 |  PASS — fold diff vs `84f178390`: no census/egress paths (the verbatim `origin/main...HEAD` base includes M159's own creation of these files — see Session Notes) |
| R2 | Order-pairing eliminated (§2, §3) | `grep -rn -w -E "collectSignup\|collectTrace\|collectCounters\|collectExporterHealth" src/` | 0 matches | P0 |  PASS — `0 matches` |
| R3 | Sample bound enforced (§5) | `grep -rn -w "SAMPLE_SIZE_BOUND" src/agentsfleetd/observability/otel_metrics_payload.zig` | ≥1 match (comptime assert site) | P0 |  PASS — `otel_metrics_payload.zig:44` const + `:88` comptime assert |
| R4 | Concurrency proof (§2) | `zig build test 2>&1 \| grep -c "hammer_no_lost_increments"` | ≥1 (test present + suite exit 0) | P0 |  PASS — test present (`otel_instruments_test.zig:80`) + suite exit 0 (`2133 pass` final); the verbatim grep counts 0 on a silent-pass runner — see Session Notes |
| R5 | Dead code gone (§7) | `grep -rn -w -E "_HELP" src/agentsfleetd/ \| grep -v test` | 0 matches | P0 |  PASS — `0 matches` |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 |  PASS — fold diff = 25 paths, all in this table (including the EXECUTE amendments) |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 |  PASS — Zig lanes green (`unit=3547` final, post-review); app/website/design-system green; CLI lane red locally only — live host credentials break its unauthenticated-environment acceptance tests; lane green in CI on this branch (environment constraint, see Session Notes) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 |  PASS — `All lint checks passed` |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 |  PASS — `[agentsfleetd] All integration tests passed` |
| S5 | No leaks | `make m` | exit 0 | P0 |  PASS — `memleak gate passed (agentsfleetd + runner + lib lanes + boot-drain lifecycle)` (the target's name is `make memleak`) |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 |  PASS — both targets exit 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 |  PASS — `no leaks found` (4250 commits scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 |  PASS — every non-test source is at most 350 lines; the verbatim sweep lists only pre-existing `*_test.zig` files, exempt per the length gate |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 |  PASS — all sweep greps 0 matches; `metrics_fleet.zig` deleted from disk and git |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/observability/metrics_fleet.zig` | `test ! -f src/agentsfleetd/observability/metrics_fleet.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `metrics_fleet` | `grep -rn -w "metrics_fleet" src/ \| head` | 0 matches |
| `snapshotFleetFields` | `grep -rn -w "snapshotFleetFields" src/ \| head` | 0 matches |
| `collectSignup` (and each deleted `collect*`) | `grep -rn -w "collectSignup" src/ \| head` | 0 matches |
| `push1` / `push2` helpers | `grep -rn -w -E "push1\|push2" src/agentsfleetd/observability/ \| head` | 0 matches |
| `LEASE_POLLS_HELP` (sentinel for the `_HELP` family) | `grep -rn -w "LEASE_POLLS_HELP" src/ \| head` | 0 matches |

## Out of Scope

- Any new metric family, label, unit, or alert/dashboard change — this milestone is storage and collection only.
- Runtime string interning of dynamic values (model, runner id) — a runtime intern table is a concurrency and memory liability; dynamic values stay inline by design.
- `library_read_counters.zig` — test-only tallies, deliberately not telemetry (its own module doc), untouched.
- The OTLP exporter substrate (`otlp/`), traces, and logs signals — unchanged.
- Protobuf or gzip OTLP encodings — the JSON envelope stays as-is.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer adds a new metric family by writing one registry row and one writer call, and the build fails loudly if anything about it is inconsistent; dashboards look exactly as they did the day before.
2. **Preserved user behaviour** — every Grafana panel, alert rule, and series name keeps working bit-for-bit; operators observe no change at all.
3. **Optimal-way check** — comptime generation from the already-closed registry is the most direct path; the unconstrained optimum (protobuf wire + runtime meter SDK) changes the wire and is explicitly rejected.
4. **Rebuild-vs-iterate** — iterate: the M159 registry is the right foundation; this removes the hand-synced copies around it. Determinism improves (typed binding replaces order pairing).
5. **What we build** — one instrument layer file + registry dimension declarations + compact labels + hashed aggregator + the collapse of six source files.
6. **What we do NOT build** — a general metrics SDK, runtime interning, new families, exporter changes (each rejected above).
7. **Fit with existing features** — compounds with M159's census guards (they become the frozen proof surface); must not destabilize the exporter flush path hardened in commit `ea1222ed4`.
8. **Surface order** — N/A — no user surface; internal daemon refactor.
9. **Dashboard restraint** — N/A — no dashboard change of any kind.
10. **Confused-user next step** — a developer misusing the writer gets a compile error naming the family and expected label struct; the registry file's doc comments carry the operator prose.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven slices ordered so the registry declaration (§1) and layer (§2) land before the source collapses (§3/§4), and the hot-path changes (§5/§6) ride the same typed identities; the sweep (§7) closes.
- **Alternatives considered:** (a) collapse only `metrics_counters.zig` and leave the siblings — rejected: leaves three copies of the pattern and the order-pairing risk alive; (b) hash the aggregator without interning labels — rejected: hashing 490-byte identities buys little, the wins compound only together; (c) full OTel meter SDK adoption — rejected: changes wire behaviour and adds a dependency for no operator-visible gain.
- **Patch-vs-refactor verdict:** this is a **refactor** because the observable behaviour is frozen by test while the internal shape consolidates; it is deliberately scoped to `observability/` so the PR #597 fold stays reviewable.

## Discovery (consult log)

- **Consults** —
- **Metrics review** — no analytics/funnel playbook update required: no product/operator signal changes; wire frozen by census tests.
- **Skill-chain outcomes** —
- **Deferrals** —
