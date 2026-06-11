# M91_001: Memory-loss counters — every silent loss event becomes a Prometheus signal

**Prototype:** v2.0.0
**Milestone:** M91
**Workstream:** 001
**Date:** Jun 11, 2026
**Status:** DONE
**Priority:** P1 — the platform sells durable memory; today a zombie's learned fact can vanish (evicted, windowed out, truncated) with zero signal, and the vector-escape-hatch evidence `direction.md:20` demands is unmeasurable
**Categories:** API, OBS
**Batch:** B1 — first in M91; its counters are the evidence feed every later workstream (and the Bucket-B escalation ladder) reads. Runs **in parallel** with M91_004 (disjoint trees: `src/zombied/` here vs `zombiectl/` there)
**Branch:** feat/m91-001-counters
**Test Baseline:** unit=1966 integration=172
**Depends on:** none (M84_005 shipped the capture/hydrate loop this instruments)
**Provenance:** agent-generated (memory-architecture analysis session, Jun 11, 2026; Indy directive "spec the memory updates") — code-grounded against `metrics_runner.zig`, `runner/memory.zig`, `zombie_memory.zig`, `memory/handler.zig`; re-confirm at PLAN.

**Canonical architecture:** `docs/architecture/observability.md` (metrics shape; scrape path is database-free) + `docs/architecture/runner_fleet.md` §Memory continuity (the loop being instrumented) + `docs/architecture/direction.md:20` (the constant these counters exist to evidence).

---

## Implementing agent — read these first

1. `src/zombied/observability/metrics_runner.zig` — the global, label-free atomic-counter pattern and the three existing `zombie_memory_*` families (captured, push-failures, hydration gauge) these six new families sit beside; the "per-zombie labels would explode" comment is binding.
2. `src/zombied/http/handlers/runner/memory.zig` — `innerRunnerMemoryHydrate` (window applied; drop math lives here) and `innerRunnerMemoryCapture` (truncation branch, per-delta validation skips, `enforceCap` call site).
3. `src/zombied/memory/zombie_memory.zig` — `Compactor.windowByBytes` (the slice the drop math compares against) and `enforceCap` (gains an evicted-count return).
4. `src/zombied/http/handlers/memory/handler.zig` — `innerListMemories` query path (the zero-hit point).
5. `dispatch/write_zig.md` — Zig discipline; cross-compile both linux targets.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m91): memory-loss observability counters`
- **Intent (one sentence):** every event where zombie memory is silently lost or invisible — hydration-window drop, cap eviction, capture truncation/skip, zero-hit search — increments a named Prometheus counter, so operators see loss the day it happens and the no-search-infrastructure bet becomes measurable.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Resolve one mechanism before EXECUTE: how the Postgres driver reports rows-affected for `enforceCap`'s DELETE (the eviction count source). A `[?]` here blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — an operator's dashboard answers "is my zombie losing memory?" the day it starts: the eviction counter moves when entry 1001 lands, instead of a support ticket three weeks later.
2. **Preserved user behaviour** — zero behaviour change on the hydrate/capture loop, API shapes, and tools. One deliberate exposition change: any memory-loss counter movement now un-gates the `/metrics` runner+memory block (previously only captures/push-failures did) — loss is never invisible, even before the first runner is seen.
3. **Optimal-way check** — counters at the four loss points is the minimal evidence engine; the unconstrained-optimal adds per-zombie attribution, rejected for metric cardinality (see Decomposition).
4. **Rebuild-vs-iterate** — iterate: extends the existing `metrics_runner` seam; determinism untouched.
5. **What we build** — six global counters + their increment call sites + render.
6. **What we do NOT build** — per-zombie labels, dashboards, alert rules, child-side metrics (the sandboxed child stays log-only).
7. **Fit with existing features** — gates the Bucket-B ladder (mid-run probe, window bump, vector hatch); rides the existing database-free `/metrics` render.
8. **Surface order** — neither CLI nor UI: Prometheus scrape, operator-facing via existing `/metrics`.
9. **Dashboard restraint** — everything memory-quality-shaped stays hidden until these counters have baselines; this workstream is the "until real" generator.
10. **Confused-user next step** — operator sees a loss counter rise → inspects the zombie's durable set with `zombiectl memory list|search` (M91_004).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE OBS** (observability conventions), **RULE UFS** (metric names + `HELP` strings as named constants, single-sourced), **RULE NDC** (no dead counters: every family rendered and asserted), **RULE FLS** (drain on any touched query path).
- **`dispatch/write_zig.md`** — memory safety, `errdefer`, cross-compile both linux targets.
- **`dispatch/write_any.md`** — LENGTH / LOGGING / MILESTONE-ID umbrella (no memory content in any log line).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` edits | read `dispatch/write_zig.md`; cross-compile x86_64 + aarch64 linux |
| PUB | yes — `enforceCap` return type changes; new pub `inc*` fns | FILE SHAPE verdict at PLAN; mirror existing `incMemoryCaptured` shape |
| LENGTH | watch — `metrics_runner.zig` grows by six families | if the file approaches 350 lines, split a `metrics_memory.zig` sibling |
| UFS | yes — six metric names + `HELP` strings | named constants beside the existing `MEM_*` constants |
| LOGGING | yes — touched warn paths | counts and caps only; never key/content |
| SCHEMA / ERROR REGISTRY / UI / DESIGN TOKEN | no | no schema, no new error codes, no UI |

---

## Overview

**Goal (testable):** each of the four memory-loss event classes — hydration-window drop, cap eviction, capture truncation/skip, zero-hit tenant search — increments a dedicated global `zombie_memory_*` counter visible on `/metrics`; `make test-integration` triggers every class and asserts its counter moved by the exact expected amount.

**Problem:** a zombie's learned fact can vanish three ways today with no signal anywhere: the hydration window silently drops the cold tail (`runner/memory.zig` applies the byte budget and logs only the kept count), `enforceCap` silently deletes the coldest rows past the per-zombie cap, and the capture path silently skips oversized deltas. Operators discover loss as "my zombie forgot" support moments. The architecture's own escalation rule — search infrastructure only on evidence that in-context retrieval is inadequate — cannot fire (or be refuted) because nothing measures recall loss.

**Solution summary:** six global, label-free counters in `metrics_runner.zig` following the existing `zombie_memory_*` family pattern, incremented at the four loss points in the two memory handlers; `enforceCap` returns its evicted-row count so the capture handler can report it. No behaviour change anywhere.

---

## Prior-Art / Reference Implementations

- **Metrics** → `src/zombied/observability/metrics_runner.zig` `incMemoryCaptured` / `incMemoryPushFailure` / `setMemoryHydrationEntries` — the exact pattern (atomic globals, named constants, `renderPrometheus` rows) to extend. No divergence.
- **Counting deleted rows** → mirror how existing zombied write paths read rows-affected from the driver; confirm the call shape at PLAN (the handshake `[?]`).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/observability/metrics_runner.zig` | EDIT | six new counter families: name/`HELP` constants, atomic globals, `inc*` fns, render rows |
| `src/zombied/memory/zombie_memory.zig` | EDIT | `enforceCap` returns the evicted-row count |
| `src/zombied/http/handlers/runner/memory.zig` | EDIT | hydrate computes dropped entries/bytes and increments; capture increments truncation, skip, and eviction counters |
| `src/zombied/http/handlers/memory/handler.zig` | EDIT | query path increments zero-hit counter |
| `src/zombied/http/handlers/memory/helpers.zig` | EDIT | `collectEntries` reports clean-drain so a truncated collect (database blip/OOM) never counts as a zero-hit — added at `/review` (adversarial finding F1) |
| `src/zombied/memory/zombie_memory_integration_test.zig` | EDIT | eviction-count assertions ride the existing cap test |
| `make/`-driven integration test for handlers (existing memory integration suite) | EDIT | per-class counter assertions |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, five Sections — one per loss point plus render. Counters first in M91 because M91_002's selection change needs a before/after baseline.
- **Alternatives considered:** (1) per-zombie labelled metrics — rejected: cardinality explosion, against the explicit comment in `metrics_runner.zig`; per-zombie inspection is M91_004's CLI, not Prometheus. (2) a durable loss-event table — rejected: the scrape path must stay database-free, and a table is infrastructure the constant says to avoid.
- **Patch-vs-refactor verdict:** **patch** — extends an existing seam; no structural change.

---

## Sections (implementation slices)

### §1 — Hydration-window drop counters — DONE

The hydrate handler already holds both the full row set and the compacted slice; the difference is the loss. Two counters: dropped entries, dropped bytes (key+content+category, same arithmetic as `windowByBytes`).

- **Dimension 1.1** — DONE — over-budget durable set increments both counters by the exact dropped entry/byte amounts → `test_hydrate_drop_counters_exact`
- **Dimension 1.2** — DONE — set within budget increments neither → `test_hydrate_no_drop_when_fits`
- **Dimension 1.3** — DONE — tenant list path (passthrough, no window) never touches hydration-drop counters → `test_tenant_list_never_counts_drops`

### §2 — Cap-eviction counter — DONE

`enforceCap` returns how many rows its DELETE removed; the capture handler increments by that count. **Implementation default:** rows-affected from the driver's exec result, because a second counting query would double the write-path cost — verify the driver call shape at PLAN.

- **Dimension 2.1** — DONE — push that lands N entries over the cap increments by exactly N → `test_cap_eviction_counter_exact`
- **Dimension 2.2** — DONE — push under the cap increments zero → `test_under_cap_no_eviction_count`
- **Dimension 2.3** — DONE — eviction failure keeps the existing warn-and-continue behaviour and increments nothing → covered at the adapter tier (`enforceCap failure propagates as an error, deleting nothing`) + the unit no-op guard (`zero-count increments are no-ops`); see Discovery — HTTP-path fault injection is not reachable from the harness

### §3 — Capture-loss counters — DONE

Two events in `innerRunnerMemoryCapture`: the byte-budget truncation branch (push stops early) and per-delta validation skips (oversized/empty key, content, category).

- **Dimension 3.1** — DONE — push exceeding `MAX_MEMORY_PUSH_BYTES` increments the truncation counter once per truncated push → `test_capture_truncation_counter`
- **Dimension 3.2** — DONE — each invalid delta increments the skip counter; valid deltas in the same push still persist → `test_capture_skip_counter_per_delta`

### §4 — Zero-hit search counter — DONE

Tenant `innerListMemories` with a `query` param returning zero rows is a recall miss signal (the model or operator searched for something the store couldn't substring-match).

- **Dimension 4.1** — DONE — query with no match increments → `test_search_zero_hit_counts`
- **Dimension 4.2** — DONE — query with ≥1 match increments nothing → `test_search_hit_no_count`
- **Dimension 4.3** — DONE — list path without `query` never increments → `test_list_never_counts_zero_hit`

### §5 — Render and naming — DONE

All six families on `/metrics` with `HELP` strings, names single-sourced as constants in the `zombie_memory_*` prefix family.

- **Dimension 5.1** — DONE — a scrape renders all six families with `HELP` lines → `test_metrics_render_memory_loss_families` (unit) + `test_metrics_render_memory_loss_families_http` (live `/metrics` scrape)
- **Dimension 5.2** — DONE — render reads atomics only; no allocator, no database touch → `test_render_no_db_no_alloc` (existing render-test pattern)

---

## Interfaces

New Prometheus families (global, label-free; names are the public surface — locked):

```
zombie_memory_hydration_dropped_entries_total   counter
zombie_memory_hydration_dropped_bytes_total     counter
zombie_memory_cap_evictions_total               counter
zombie_memory_capture_truncated_total           counter
zombie_memory_capture_skipped_total             counter
zombie_memory_search_zero_hits_total            counter
```

`enforceCap` gains an evicted-count return (Zig error union with an unsigned count); all other signatures unchanged. No HTTP surface changes; no OpenAPI change.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| increment on hot path fails | impossible by construction | increments are lock-free atomic adds, no allocation; nothing to fail — verified by the render/no-alloc test |
| rows-affected unavailable | driver exec result lacks the count | increment zero + `log.warn` once per occurrence; capture still succeeds — eviction is never blocked on telemetry |
| eviction DELETE fails | database blip | existing behaviour preserved: warn + continue; counter does not move (Dimension 2.3) |
| concurrent scrapes during increments | parallel requests | atomics, monotonic loads — same guarantee as existing families |
| counter overflow | very long uptime | u64 monotonic; Prometheus rate() handles wrap — accepted, matches existing families |
| search collect truncated mid-stream | database blip / OOM during row collection | NOT counted as a zero hit — `collectEntries` reports clean-drain and the counter moves only on a clean empty result; failure noise must not fabricate recall-miss evidence |
| retried failed push re-counts skips | store failure mid-push → runner retries whole push (upsert-idempotent) | `capture_skipped_total` over-counts by the skip count per retry and `entries_captured_total` under-counts the failed attempt's stored rows — accepted: counters are trend signals, not row accounting |
| zero-hit counter is tenant-influenceable | any tenant scripting non-matching searches inflates the global counter | accepted by the label-free design; M91_002 / Bucket-B consumers must treat it as adversarially influenceable evidence, not ground truth |

---

## Invariants (Hard Guardrails)

1. **No database call on the `/metrics` scrape path** — render reads atomics only; enforced by the render function taking no connection parameter (compile-level) + the existing render test.
2. **No per-zombie labels** — enforced by the `inc*` API shape: the functions take counts only, no identifier parameter exists to leak.
3. **Counters are monotonic** — production paths expose only `fetchAdd`; the lone reset is the test-only `resetForTest` (named, never called from production — same convention as every existing metrics module) and the declared gauge keeps its store.
4. **No memory content in logs or metrics** — counts, byte totals, caps only; enforced by LOGGING gate review + existing log-discipline tests.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_hydrate_drop_counters_exact` | seed rows exceeding window budget → hydrate → dropped-entries and dropped-bytes counters move by the exact arithmetic difference |
| 1.2 | integration | `test_hydrate_no_drop_when_fits` | small set → hydrate → both drop counters unchanged |
| 1.3 | integration | `test_tenant_list_never_counts_drops` | tenant GET list → hydration-drop counters unchanged |
| 2.1 | integration | `test_cap_eviction_counter_exact` | push cap+N entries → eviction counter +N; evicted rows are the coldest |
| 2.2 | integration | `test_under_cap_no_eviction_count` | push under cap → counter unchanged |
| 2.3 | integration + unit | `enforceCap failure propagates as an error, deleting nothing` + `zero-count increments are no-ops and never activate render` | injected eviction failure (adapter tier) → error propagates, nothing deleted; handler catch breaks to 0 → increment no-op; warn-and-continue unchanged (see Discovery — HTTP-path injection unreachable) |
| 3.1 | integration | `test_capture_truncation_counter` | push exceeding `MAX_MEMORY_PUSH_BYTES` → truncation counter +1, stored count matches kept prefix |
| 3.2 | integration | `test_capture_skip_counter_per_delta` | push with 2 invalid + 1 valid delta → skip counter +2, one row persisted |
| 4.1 | integration | `test_search_zero_hit_counts` | tenant query with no match → zero-hit counter +1, HTTP 200 empty items |
| 4.2 | integration | `test_search_hit_no_count` | tenant query matching one row → counter unchanged |
| 4.3 | integration | `test_list_never_counts_zero_hit` | tenant list (no query) on empty store → counter unchanged |
| 5.1 | integration | `test_metrics_render_memory_loss_families` | scrape `/metrics` → all six family names + `HELP` lines present |
| 5.2 | unit | `test_render_no_db_no_alloc` | render with testing allocator + no pool → output produced, zero leaks |

Regression: existing three `zombie_memory_*` families keep their names, types, and semantics (`test_existing_memory_families_unchanged`). Idempotency: re-running a capture push (same deltas) moves capture counters consistently with the upsert semantics — asserted inside 2.2.

---

## Acceptance Criteria

- [x] All six families render on `/metrics` with `HELP` — verified: `test_metrics_render_memory_loss_families` (unit) + `test_metrics_render_memory_loss_families_http` (live scrape) green in `make test-integration`
- [x] Each loss class moves only its own counter, by exact amounts — verified: `make test-integration` exit 0 (Dimensions 1.1–4.3 + category-arm guard)
- [x] `enforceCap` returns evicted count; all callers updated — verified: `make test-unit-zombied` + grep (callers: `runner/memory.zig`, `zombie_memory_integration_test.zig`)
- [x] Lint clean (`make lint-zig`) · unit lane passes (`make test-unit-zombied`, 1291 pass/0 fail) · `make test-integration` passes (exit 0) — note: the repo's targets are `lint-zig`/`test-unit-*`; `make test`/`make lint` per VERIFY_TIERS.md are stale names (Discovery)
- [x] pg-drain check passes (folded into `make lint-zig`, 460 files)
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` → XC-PASS
- [x] `gitleaks detect` clean · no source file over 350 lines (`metrics_runner` 282, `metrics_memory` ~180)

---

## Eval Commands (post-implementation)

```bash
# E1: Build + unit
zig build && make test 2>&1 | tail -3
# E2: Integration (memory + metrics suites)
make test-integration 2>&1 | tail -5
# E3: Lint + drain
make lint 2>&1 | grep -E "✓|FAIL"; make check-pg-drain 2>&1 | tail -2
# E4: Cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo XC-PASS
# E5: Family names present exactly once each (single-sourced)
grep -c "zombie_memory_hydration_dropped_entries_total" src/zombied/observability/*.zig
# E6: Gitleaks
gitleaks detect 2>&1 | tail -2
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted; `enforceCap`'s old void return is replaced in the same diff (no orphaned callers — E-grep above).

---

## Discovery (consult log)

- **Consults** —
  - *Handshake resolved (PLAN):* rows-affected comes from the vendored `pg.zig` driver — `conn.exec()` returns `!?i64` parsed from the Postgres `CommandComplete` tag (`vendor/pg/src/conn.zig:362,401-416`); null only when the tag carries no count → `enforceCap` warns (`memory_cap_evict_count_unavailable`) and reports 0, per Failure Modes.
  - *LENGTH split exercised:* `metrics_runner.zig` (319 lines pre-diff) would cross 350 with six families → split `metrics_memory.zig` per this spec's LENGTH gate row. All nine `zombie_memory_*` families (three existing + six new) moved there; `metrics_runner.renderPrometheus` delegates, so the `/metrics` composition and family names are byte-identical (pinned by `test_existing_memory_families_unchanged`). Exposition-format constants single-sourced in the new module (RULE UFS).
  - *Dimension 2.3 decomposition:* an eviction DELETE failure cannot be injected through the in-process HTTP harness. Coverage split: adapter tier proves the error propagates and deletes nothing (driver rejects a malformed zombie_id with `error.InvalidUUID` before the bind — the handler catch treats every error identically); the handler's catch path breaks to 0 and `incCapEvictions(0)` is a proven no-op (unit). Warn-and-continue behaviour unchanged.
  - *Test-fixture constraint:* `memory.memory_entries.uid` carries `ck_memory_entries_uid_uuidv7` (version nibble = 7), so the bulk seed composes deterministic v7-shaped uids in SQL — `gen_random_uuid()` (v4) is rejected.
  - *Doc drift (not fixed here, out of scope):* `docs/VERIFY_TIERS.md` tier 1 says `make test`, but the Makefile's lanes are `test-unit-*` (`test-unit-zombied` used as tier 1 here); same for `make lint` → `lint-zig`/`lint-all` and `make check-pg-drain` → folded into `lint-zig`. Surfaced for Indy.
- **Skill chain outcomes** —
  - `/write-unit-test` (Change-set mode): diff ledger 27/27 resolved — 24 tested, 3 `won't-test` with reasons (driver-unreachable `orelse`/`n<0` arms in `enforceCap`; log-field assertions per repo convention). One gap closed during the audit: `test_category_filter_never_counts_zero_hit`. Negative-path ratio ~53%; no structural perf/concurrency findings (lock-free fetchAdds, zero new DB round-trips).
  - `/review` (adversarial, Claude subagent + Codex cross-model): 5 FIXABLE fixed in `23a6e87b` — F1 zero-hit integrity (`collectEntries` clean-drain), F2 fixture uid/id uniqueness, F3 spec test-name drift, F4 invariant wording, F5 atomic-ordering honesty; 4 INVESTIGATE dispositioned into Failure Modes / Product Clarity (F6 torn-pair scrape skew — same guarantee as existing families; F7 retried-push skip over-count; F8 tenant-influenceable zero-hit; F9 render-gating widening, owned). Codex verdict: "Ship as-is". The F1 no-count-on-truncated-collect branch is compile-enforced at the call site; runtime injection of a mid-collect failure is `needs-infra` (same class as the eviction HTTP injection).
  - `/review-pr` + `kishore-babysit-prs` — pending PR creation (blocked on `main` push; see Deferrals/handoff).
- **Deferrals** — no spec Section/Dimension deferred. Two rule-blocked CHORE(close) outputs await Indy (not scope cuts — both are Hard-Safety/handoff gates only Indy can release):
  1. **Changelog `<Update>` in `~/Projects/docs/changelog.mdx`** — cross-repo writes to the docs repo need an explicit per-session ask (AGENTS.md Hard Safety); land on `chore/m91-001-counters-changelog` once authorized.
  2. **PR creation** — local `main` is ahead of `origin/main` by the two M91 spec commits; the handoff requires Indy to push (or authorize pushing) `main` before any PR opens. `/review-pr` + `kishore-babysit-prs` fire immediately after.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |
| After every push | `kishore-babysit-prs` | final report in Discovery |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-zombied` | 1291 pass / 0 fail / 359 skipped (no-DB lane) | ✅ |
| Integration tests | `make test-integration` | exit 0 — "All integration tests passed" (live DB + Redis) | ✅ |
| Lint + drain | `make lint-zig` (fmt + ZLint + pg-drain + depth + line-limit + role/legacy) | all green; pg-drain 460 files | ✅ |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | XC-PASS | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |
| Test delta | `make _lint_zig_test_depth` vs CHORE(open) baseline | unit 1966→1986 (+20) · integration 172→173 (+1 suite file) | ✅ |
| Coverage lanes | `make test-coverage-all` | app 883 + website 149 + zombiectl 403 pass (first run flaked on a cold-cache vitest timeout; clean on re-run) | ✅ |

---

## Out of Scope

- Per-zombie loss attribution (cardinality — inspection belongs to M91_004's CLI).
- Grafana dashboards / alert rules — operator tooling, separate concern.
- Child-side (in-run) metrics — the sandboxed child stays log-only by design.
- Any selection-policy change — M91_002 consumes these counters; this workstream only measures.
