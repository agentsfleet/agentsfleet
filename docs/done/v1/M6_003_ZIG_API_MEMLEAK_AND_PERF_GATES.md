# M6_003: Zig API Memleak And Performance Gates

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 003
**Date:** Mar 07, 2026
**Status:** ✅ DONE
**Priority:** P0 — v1 stability and stress confidence
**Depends on:** M1_003 (Observability and Policy)

---

## 1.0 Ghostty Precedent And Why It Matters

**Status:** ✅ DONE

Adopt the proven pattern used by Ghostty: explicit Valgrind execution targets in build orchestration (`run-valgrind`, `test-valgrind`) and operator documentation for leak checks.

Ghostty precedent references:
- `build.zig` defines `run-valgrind` and `test-valgrind` steps for deterministic leak checks.
- `HACKING.md` documents when and how to use Valgrind, including suppression file usage for known noise.

Why it helps UseZombie:
- Detects C/library-level leaks and misuse paths Zig allocator checks cannot fully observe.
- Turns leak checks into one-command, repeatable developer/CI workflows.
- Reduces regressions by making memory safety a routine gate instead of ad-hoc debugging.

Suppression-file policy:
- Default policy is zero suppressions for M6_003.
- Linux gate uses strict Valgrind flags with `--error-exitcode=1` and no suppression file.
- If suppressions are ever introduced, they must be explicitly reviewed and documented in this workstream evidence before enabling in CI.

Evidence format used:
- Command line invoked.
- Key summary metrics (`total`, `ok`, `fail`, `timeout`, latency percentiles, RSS growth).
- Artifact path under `.tmp/`.
- Pass/fail exit behavior.

**Dimensions:**
- 1.1 ✅ DONE Document Ghostty precedent and map equivalent commands for UseZombie runtime
- 1.2 ✅ DONE Define platform behavior (`valgrind` required on Linux; explicit unsupported handling on macOS)
- 1.3 ✅ DONE Define suppression-file policy and storage location for deterministic results
- 1.4 ✅ DONE Define evidence format (command, output summary, leak/error counts)

---

## 2.0 Standard Make Targets (Minimal Surface)

**Status:** ✅ DONE

Introduce a minimal, stable stress/perf target set for Zig API hardening.

Required targets:
- `make memleak` — memory leak checks (allocator + valgrind/leaks path where supported)
- `make bench` — benchmark entrypoint with mode support (`BENCH_MODE=bench|soak|profile`)
- `make bench BENCH_MODE=soak` — soak mode via shared benchmark runner
- `make bench BENCH_MODE=profile` — profile mode via shared benchmark runner (private wrappers `_soak` and `_bench_apiprofile` also available)

**Dimensions:**
- 2.1 ✅ DONE Add `memleak` target with deterministic pass/fail contract
- 2.2 ✅ DONE Add `bench` target with shared runner and mode-based execution
- 2.3 ✅ DONE Add soak mode with default duration and explicit thresholds via `BENCH_MODE=soak`
- 2.4 ✅ DONE Add profile mode with artifact output via `BENCH_MODE=profile`

---

## 3.0 Metrics, Thresholds, And Failure Contracts

**Status:** ✅ DONE

Define objective, machine-checkable thresholds so stress tests are actionable.

Default gate thresholds:
- `bench`: `error_rate <= 0.01`, `p95 <= 250ms`, `rss_growth <= 128MB`
- `soak`: `error_rate <= 0.02`, `p95 <= 400ms`, `rss_growth <= 256MB`
- `profile`: `error_rate <= 0.02`, `p95 <= 300ms`, `rss_growth <= 192MB`
- Leak-counter acceptance: `make memleak` is red on allocator-leak test failure and Linux valgrind leak/error exit (`--error-exitcode=1`)

**Dimensions:**
- 3.1 ✅ DONE Define default SLO thresholds (error-rate, p95/p99 latency, timeout budget)
- 3.2 ✅ DONE Define memory regression thresholds (RSS growth and leak counter acceptance)
- 3.3 ✅ DONE Define output schema for bench/soak/profile summaries under `.tmp/` and/or `coverage/`
- 3.4 ✅ DONE Define failure exit-code contract for local and CI usage

---

## 4.0 Status Transition Discipline (Execution Rule)

**Status:** ✅ DONE

When this milestone is executed, status updates are mandatory and explicit:
- Milestone status must move `PENDING -> IN_PROGRESS -> ✅ DONE`.
- Every section must move `PENDING -> IN_PROGRESS -> ✅ DONE`.
- Every dimension must move `PENDING -> IN_PROGRESS -> ✅ DONE`.
- No section or dimension may remain `PENDING` when milestone status is `✅ DONE`.

**Dimensions:**
- 4.1 ✅ DONE Apply transition rule to all milestone updates in this workstream
- 4.2 ✅ DONE Enforce green-tick completion markers (`✅ DONE`) in final state
- 4.3 ✅ DONE Capture verification evidence before marking any item `✅ DONE`

---

## 5.0 Acceptance Criteria

**Status:** ✅ DONE

- [x] 5.1 `make memleak` passes deterministically and catches intentional leak fixture
- [x] 5.2 `make bench` provides repeatable latency/throughput output and threshold result
- [x] 5.3 `make bench BENCH_MODE=soak` completes duration run with explicit pass/fail summary
- [x] 5.4 `make bench BENCH_MODE=profile` produces profile artifacts and locations are documented
- [x] 5.5 CI can execute at least `memleak` + `bench` without manual intervention (`.github/workflows/memleak.yml`, `.github/workflows/bench.yml`)

---

## 6.0 Out of Scope

- Cross-region distributed load generation for v1
- Full production traffic replay before v1
- UI performance benchmarking (tracked separately)

---

## 7.0 Post-Completion Revalidation

**Status:** ✅ DONE

`Mar 10, 2026: 06:35 PM` — Revalidated Zig 0.15.2 compatibility after fixing compile blockers in GitHub auth and PR flow paths.

**Dimensions:**
- 7.1 ✅ DONE `zig build --summary all` passes after the compatibility fixes
- 7.2 ✅ DONE Release-target verification passes for `x86_64-linux`, `aarch64-linux`, `x86_64-macos`, and `aarch64-macos`
