# M61_001: Remove production-dead Zig — AI Firewall Engine + OTEL push-export

**Prototype:** v2.0.0
**Milestone:** M61
**Workstream:** 001
**Date:** May 04, 2026
**Status:** DONE
**Priority:** P1 — biggest single deletion lever found in the May 04 audit. ~2.6k LOC of compiled-but-unwired code; every commit pays for it in build time, test time, and reader cognitive load. Pre-v2.0.0, RULE NLG forbids carrying unwired engines on speculation.
**Categories:** API
**Batch:** B1
**Branch:** feat/m61-firewall-otel-export-removal
**Depends on:** none

**Canonical architecture:** `docs/architecture/` — N/A (pure deletion of unreachable surface; no flow change).

---

## Implementing agent — read these first

1. `AGENTS.md` — RULE NLG (no legacy framing pre-v2.0.0), RULE ORP (orphan sweep), Verification Gate, Schema Removal Guard (not relevant here, but read for the deletion-discipline pattern).
2. `docs/v2/done/M56_001_P2_API_DEAD_CODE_SWEEP_SRC.md` — sibling spec from Apr 30, 2026 that established the orphan-sweep evidence pattern. Mirror its `Eval Commands` shape.
3. `docs/ZIG_RULES.md` — `pub` audit (any kept symbol with no external consumer must drop `pub`); test-discovery model (a `*_test.zig` that no parent `@import`s never compiles into `make test`, so deleting it does not change the test count).
4. `build.zig` — entry points are `src/main.zig`, `src/executor/main.zig`, `src/auth/tests.zig`, `src/zbench_micro.zig`, plus the `hmac_sig` build-module. Only the **production** binaries (`src/main.zig` and `src/executor/main.zig`) ship; the test entries gate `zig build test`.
5. `src/main.zig` — read the giant `test { _ = @import(...); }` bridge block (≈ lines 130-265). Every file in the deletion list below appears ONLY there: imported for test discovery, never reached from production code. Cross-check by grepping for `@import(".*<basename>.zig")` outside `src/main.zig` and `_test.zig` siblings — zero hits is the keep/delete discriminator.

---

## Applicable Rules

- `AGENTS.md` — RULE NLG (no "in case we need it"), RULE ORP (orphan sweep), RULE TST-NAM, Milestone-ID Gate, Verification Gate, File & Function Length Gate.
- `docs/greptile-learnings/RULES.md` — RULE ORP, RULE FLL.
- `docs/ZIG_RULES.md` — `pub` audit, test discovery via parent `comptime { _ = @import(...) }`, cross-compile.

---

## Overview

**Goal (testable):** After this workstream, `rg '@import\("zombie/firewall/' src/` returns zero hits, `rg '@import\("observability/otel_(export|histogram|json)' src/` returns zero hits, `make lint` is clean, `make test` and `make test-integration` pass on equal-or-fewer test counts (tests we delete are tests for code we delete — no production behavior changes), and cross-compile to `x86_64-linux`+`aarch64-linux` succeeds.

**Problem:** A May 04, 2026 forensic audit (post-M56) built the **production-only** `@import` closure starting from `src/main.zig` and `src/executor/main.zig`, ignoring the `test {}` bridge block in `src/main.zig`. Two large module families fall outside that closure:

1. **AI Firewall Policy Engine (M6_001)** — 5 production source files + 3 test files at `src/zombie/firewall/`, ~1488 LOC total. Imported only inside the `test {}` block of `src/main.zig` (lines 257-264). No call site in `src/zombie/event_loop*.zig`, `src/http/handlers/zombies/**`, `src/cmd/**`, or anywhere else in the production graph. The engine compiles, its tests run, but no zombie execution path consults it. RULE NLG: pre-v2.0.0, an unwired engine is not "WIP we'll get to" — it is dead weight that misleads readers about what the system enforces.
2. **OTEL push-export pipeline** — `src/observability/otel_export.zig` (331), `otel_histogram.zig` (263), `otel_json.zig` (170), plus their two test files (208), ~972 LOC. The live observability stack uses `otel_logs.zig` and `otel_traces.zig` (both wired into `src/main.zig`, `src/cmd/preflight.zig`, `src/zombie/metering.zig`, `src/http/server.zig`). The push-export trio (`otel_export.*`) is never imported from any of those — only from the test bridge.

These survived M53/M54/M56 because those sweeps walked the *unioned* closure (test-bridge + production), under which every test-imported file looks reachable. The production-only closure is the right lens for binary size, startup latency, and "is this engine actually enforcing anything in prod" questions.

**Solution summary:** Delete the eight production source files and five test files listed below. No call-site rewiring, no schema work, no public-interface change. Re-run `make lint`, `make test`, `make test-integration`, cross-compile, and the production-orphan eval (E1 below). Expected wins: ~2.6k LOC deleted; faster `zig build test`; smaller binary (firewall regex tables and OTEL JSON encoder both contributed comptime-evaluated tables); zero runtime change because nothing called them.

> **Decision point not yet resolved (surface in PLAN, get user A/B/C before EXECUTE).** The M6_001 firewall spec is in `docs/v2/done/`, marked DONE. Deleting an engine that a "DONE" spec landed is the kind of move where Captain wants explicit acknowledgement. The right move per RULE NLG is to delete the unwired code and amend the M6_001 spec body (not move it back to pending) with an "Apr 30 → May 04: engine retired pre-v2.0 because no production path consults it; refile under M{N+k} when a wedge actually requires AI-firewalling and ship engine + wiring + tests in the same milestone." The agent SHOULD NOT assume this is approved — it is a Legacy-Design Consult Guard trigger. Surface options: (A) delete now + amend M6_001; (B) keep firewall, delete OTEL only; (C) keep both, mark this spec REJECTED.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombie/firewall/firewall.zig` | DELETE | 140 LOC; M6_001 engine entry point; zero production callers. |
| `src/zombie/firewall/domain_policy.zig` | DELETE | 66 LOC; only consumer was `firewall.zig`. |
| `src/zombie/firewall/endpoint_policy.zig` | DELETE | 243 LOC; only consumer was `firewall.zig`. |
| `src/zombie/firewall/injection_detector.zig` | DELETE | 182 LOC; only consumer was `firewall.zig`. |
| `src/zombie/firewall/content_scanner.zig` | DELETE | 284 LOC; only consumer was `firewall.zig`. |
| `src/zombie/firewall/firewall_test.zig` | DELETE | 110 LOC; tests deleted code. |
| `src/zombie/firewall/firewall_robustness_test.zig` | DELETE | 341 LOC; tests deleted code. |
| `src/zombie/firewall/firewall_greptile_test.zig` | DELETE | 122 LOC; tests deleted code. |
| `src/zombie/firewall/` (directory) | DELETE | Empty after the eight files above leave; remove the directory. |
| `src/observability/otel_export.zig` | DELETE | 331 LOC; OTEL HTTP push-export; zero live consumers (compare `otel_logs.zig`/`otel_traces.zig` which ARE wired). |
| `src/observability/otel_histogram.zig` | DELETE | 263 LOC; only consumer was `otel_export.zig` and its test. |
| `src/observability/otel_json.zig` | DELETE | 170 LOC; only consumers were `otel_export.zig` and `otel_histogram.zig`. |
| `src/observability/otel_export_test.zig` | DELETE | 86 LOC; tests deleted code. |
| `src/observability/otel_histogram_test.zig` | DELETE | 122 LOC; tests deleted code. |
| `src/main.zig` | EDIT | Drop the eight `_ = @import("zombie/firewall/...");` lines and the five `_ = @import("observability/otel_(export|histogram|json|...)*.zig");` lines from the `test {}` bridge block. |
| `docs/v2/done/M6_001_P0_API_AI_FIREWALL_POLICY_ENGINE.md` | EDIT | Append a final "Retired May 04, 2026" amendment block — see Decision Point above; only land this on user A/B/C answer = A. Otherwise this row drops. |

No schema changes. No HTTP handler changes. No CLI changes. No UI changes. No public interface changes.

---

## Sections (implementation slices)

### §1 — User decision on firewall retirement — DONE

Surface the Decision Point above as a Legacy-Design Consult block. Wait for A/B/C. Do not start §2 until answered. If B: drop firewall rows from §Files Changed, proceed to §3. If C: close this spec REJECTED. Implementation default: do not pick A on auto-mode — the engine is a P0 spec marker and Captain owns retirement of P0 commitments.

### §2 — Delete firewall engine (only on decision = A) — DONE

`git rm` all eight files in `src/zombie/firewall/` and the directory. Drop the eight `@import` lines from `src/main.zig`'s test block. Verify no remaining `firewall` references survive in src/ via `rg -nw 'firewall' src/ -g '!*_test.zig'` (acceptable hits: webhook signature verification `webhook_verify.zig`, which is unrelated; the audit's name collision was on the substring "firewall" inside webhook context — confirm by reading each hit). Append the Apr 30 → May 04 retirement amendment to the M6_001 spec.

### §3 — Delete OTEL push-export pipeline (always) — DONE

`git rm` the five files: `otel_export.zig`, `otel_histogram.zig`, `otel_json.zig`, `otel_export_test.zig`, `otel_histogram_test.zig`. Drop the five `@import` lines from `src/main.zig`'s test block. Confirm `otel_logs.zig` and `otel_traces.zig` remain wired and live: `rg -n '@import.*otel_(logs|traces)' src/cmd/preflight.zig src/zombie/metering.zig src/http/server.zig src/main.zig` should still show the existing imports.

### §4 — Re-run orphan eval — DONE

Run E1 (production-only orphan eval, see `Eval Commands`). Expected output: zero entries that were not already orphans before this spec. If a new orphan surfaces (e.g. a sibling helper that becomes unreachable after deletion), surface it in `Discovery` and decide remove-or-keep before COMMIT.

### §5 — Verification + ship — DONE

Run the full `Verification` block. Memleak evidence (last 3 lines of `make memleak`) lands in PR Session Notes per AGENTS.md. Cross-compile `x86_64-linux` and `aarch64-linux` mandatory because Zig changed.

---

## Interfaces

N/A — no public interface (HTTP, CLI, library, RPC) changes. No symbol in the deletion set has a live caller.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Build break after delete | Hidden `@import` discovered post-grep (string-literal path the audit's regex missed; `@embedFile` referencing a deleted path) | `zig build` fails loudly. Restore via `git restore <path>`, file the discovered consumer in `Discovery`, decide remove-vs-keep. |
| `make test` count drops by more than the 5 firewall + 2 otel test files | A wired test was passing under a different harness than expected | Run `zig build test --summary all` before and after. The expected drop is exactly the test cases inside the 5 deleted `*_test.zig` files. Any larger drop means we cut something live; investigate before COMMIT. |
| Cross-layer name collision in `Discovery` | Some downstream code calls `firewall_*` or `otel_export_*` via a shared interface (unlikely; Zig has no dynamic dispatch by name) | The `rg` sweep in §2/§3 catches it. Add hit to `Discovery` with `file:line` and recommend keep. |
| OTEL telemetry regression in dev | The push-export pipeline turns out to be the path one operator turned on via env var | Confirm via `rg -n 'otel_export' src/cmd/serve.zig src/cmd/worker.zig src/observability/init*.zig` before deletion that no production initializer ever wires it. The audit already verified this; re-confirm during PLAN. |

---

## Invariants

1. **Production-only `@import` closure has zero entries from the deletion set** — enforced by E1 below; CHORE(close) blocks if any entry shows up.
2. **`make test` test count drops by exactly the count of `test "..."` blocks inside the deleted `*_test.zig` files** — measurable via `zig build test --summary all`.
3. **No file under `src/` after the sweep carries `M{N}_{NNN}`, `§X.Y`, `T{N}`, or `dim X.Y` tokens** — Milestone-ID Gate self-audit.
4. **`otel_logs` and `otel_traces` remain wired** — enforced by `make lint` and `make test-integration` (logs/traces tests cover both).

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| Existing OTEL logs/traces tests | Production OTEL paths still wired and emitting | `src/observability/otel_logs_test.zig`, `src/observability/otel_traces_test.zig` (untouched by this spec). |
| Existing zombie event_loop integration tests | Zombie execution path unchanged after firewall deletion | `src/zombie/event_loop_*_test.zig` (untouched). |
| Eval E1 (manual `rg`) | No production-orphan regressions vs the baseline 19 | Run on demand per §Eval Commands. CI wiring deferred — see Discovery. |

No new test code lands with this spec — every deletion is matched by deleting the test that exercised it. The remaining test surface is unchanged.

---

## Eval Commands

**E1 — production-only orphan eval (manual `rg` workflow):**

A Python codification of this lived briefly at `scripts/audit-orphan-prod.py` during EXECUTE; it was removed before merge (see Discovery — codecov diff-coverage gate flagged it as untested, and a CI gate would fire-fail until M61_002 lands). Until a tested codification ships, run the eval by hand: walk `@import("...")` from `src/main.zig` and `src/executor/main.zig` ignoring `test "..." { ... }` / `test { ... }` blocks, and grep any `*.zig` under `src/` (excluding `vendor/`, `third_party/`, `.zig-cache/`, `*_test.zig`, `test_*.zig`, fixtures, `zbench_*.zig`, `auth/tests.zig`) outside the closure. Empty output = pass.

The May 04, 2026 audit baseline was 19 entries; this spec retires 8 of them (firewall family + otel-export trio). M61_002 retires the rest.

**E2 — firewall-name sweep:**

```bash
rg -nw 'firewall' src/ -g '!*_test.zig' -g '!webhook_verify*'
```

Expected after deletion: only webhook signature contexts (which mention "firewall" in comments about HTTP layer). Read every hit; none should reference `zombie/firewall/` symbols.

**E3 — Verification block** — full `make lint`, `make test`, `make test-integration`, `make memleak`, cross-compile per AGENTS.md `Verification Gate`.

---

## Discovery (filled during EXECUTE)

Reserve this section for surprises: a callsite the audit missed, a sibling that became orphan-by-cascade, a build.zig wiring the audit didn't model. Each entry: `file:line — observation — decision`.

- **May 04, 2026** — `src/errors/error_entries_runtime.zig:48,122-128` + `src/errors/error_registry.zig:175,205-207` + `src/errors/error_entries.zig:240` — Cascade orphans uncovered by firewall delete. Four error registry entries (`UZ-EXEC-008`, `UZ-FW-001/002/003`) and three pub constants (`ERR_FW_DOMAIN_BLOCKED`, `ERR_FW_APPROVAL_REQUIRED`, `ERR_FW_INJECTION_DETECTED`) had zero consumers in `src/`, public docs (`docs.usezombie.com` mdx), and JS/TS — verified via `rg`. Decision: remove with the engine per RULE ORP. Comment in `error_entries.zig` listing error families also dropped the now-stale "firewall" mention. No public-API impact: the codes were never reachable in production responses (the engine that returns them was never wired).
- **May 04, 2026** — `src/observability/metrics_counters.zig:50-52,79-81,123-131,167-169` + `metrics_render.zig:114-116` + `metrics.zig:29-31,100-112` — Cascade orphans from OTEL push-export delete. Three Prometheus metrics (`zombie_otel_export_total`, `zombie_otel_export_failed_total`, `zombie_otel_last_success_at_ms`), the inc/set helpers (`incOtelExportTotal`, `incOtelExportFailed`, `setOtelLastSuccessAtMs`), the Snapshot fields, the atomic globals, and the integration test that asserted those metric names — all sole consumers were inside the deleted `otel_export.zig`. Decision: remove with the engine per RULE ORP. Side effect: `/metrics` Prometheus output drops three lines on a fresh deployment; harmless because nothing scraped them (the only emitter was unwired).
- **May 04, 2026** — `scripts/audit-orphan-prod.py` (authored, then removed in same PR) — Initial run after delete confirmed 11 remaining orphans (19 baseline − 8 retired here = 11). All are M61_002 territory (10 entries) plus `src/crypto/hmac_sig.zig` (false-positive — reachable via `build.zig`'s `add_module("hmac_sig", ...)`, which the @import-walk doesn't model). Zero new orphans introduced by this spec; invariant #1 satisfied. **Script removed before merge:** codecov/patch gate failed (0% diff hit, 90% target — no tests authored for the script in this PR), and a CI gate would hard-fail until M61_002 lands and clears the remaining 11. The eval lives as a manual `rg`/Python one-liner until M61_002 (or a follow-up) ships a tested codification.
- **May 04, 2026** — `src/main.zig` — Spec body said "drop five `@import("observability/otel_(export|histogram|json|...)*.zig")` lines from src/main.zig". Reality: only **one** line in `src/main.zig` (line 176, `otel_export.zig`); the other four files chained via `otel_export.zig`'s own imports (lines 12-13 + lines 328-330 test bridge) and `otel_histogram.zig:262`. Net effect on the test count is identical — files dying together — but the spec's count was off. Noted for spec hygiene; no action needed.

---

## Out of Scope

- Other audited orphans (clerk.zig, workspace_integrations.zig, secrets/crypto.zig, sys/error+errno pair, util/strings/smol_str + case_insensitive_ascii_map, route_manifest.zig, http/handlers/workspaces/mod.zig, webhook_test_signers.zig). M61_002 owns those.
- SQL schema orphans (`prompt_lifecycle_events`, `ops_ro_access_events`, `enable_score_context_injection`). M61_003 owns those.
- UI orphans (`components/ui/card.tsx`, unwired analytics components). M61_004 owns those.
- Refactoring kept files. RULE NLR is touch-it-fix-it for the files we *edit*, not an excuse to bundle.
- Reintroducing a firewall engine or OTEL push-export later: that's a new milestone with engine + wiring + tests in the same commit. Do not stage "we'll wire it later" code — that's exactly the rot we're cleaning.
