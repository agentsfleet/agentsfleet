# M28_005: Split `src/config/runtime.zig` into per-concern modules

**Prototype:** v0.19.0
**Milestone:** M28
**Workstream:** 005
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P3 — deferrable; no user-visible behavior change
**Batch:** B1
**Branch:** _unassigned_
**Depends on:** None (sibling to M28_004)

---

## Overview

**Goal (testable):** `src/config/runtime.zig` (currently 448 lines) is split into per-concern modules so no single file exceeds the 350-line RULE FLL cap, with zero behavior change. `ServeConfig.load()` signature preserved; all callers unchanged; all tests still pass.

**Why a sibling to M28_004.** Identical pattern: a single file accreted parser, validator, and type responsibilities past the 350-line cap. Same lifecycle-axis split strategy applies. Filed separately because the two live in unrelated subtrees (`src/config/` vs `src/zombie/`) and have independent blast radius.

---

## Proposed split (lifecycle axis — matches M28_004)

| New file | Extracted content | Approx lines |
|---|---|---|
| `src/config/runtime.zig` (kept) | `ServeConfig` struct + `load()` orchestrator + `deinit()` | ~180 |
| `src/config/runtime_env_parse.zig` (new) | `parseU16Env`, `parseI16Env`, `parseU32Env`, `parseI64EnvOptional`, `parseBoolEnv`, `parseStringEnv` — generic env-var readers | ~120 |
| `src/config/runtime_validate.zig` (new) | Per-field validation (`validateApiKeys`, `validateOidc`, `validateEncryptionKey`, `validateKekVersion`) | ~100 |
| `src/config/runtime_types.zig` (new) | `ValidationError` enum | ~30 |

Final line distribution kept under 250 per file to leave headroom.

**Imports across the codebase** — `ServeConfig` and `ValidationError` are the only public symbols. Façade re-exports in `runtime.zig` keep all existing imports working.

---

## Execution Plan

1. Inventory `pub` symbols; confirm only `ServeConfig` + `ValidationError` are external.
2. Extract `ValidationError` → `runtime_types.zig`. Re-export in `runtime.zig`.
3. Extract env parsing helpers → `runtime_env_parse.zig`. Re-export as needed (likely module-private after split).
4. Extract validation helpers → `runtime_validate.zig`.
5. `runtime.zig` keeps `ServeConfig` struct + `load()` orchestrator. Orchestrator delegates to env parsers + validators.
6. Per-module `_test.zig` for each new file. Add to `main.zig` test discovery.
7. Run `zig build` after each extraction. Full `zig build test` after all extractions.
8. Re-run VERIFY gates: `make lint`, cross-compile, `make memleak`.
9. Assert every touched/new `.zig` file ≤ 350 lines; every method ≤ 50 lines.

---

## Acceptance Criteria

- [ ] `src/config/runtime.zig` ≤ 350 lines.
- [ ] No new file exceeds 250 lines.
- [ ] Every method ≤ 50 lines (RULE FLL-50).
- [ ] All existing imports continue to work (via re-exports) — no call-site churn.
- [ ] `zig build test` passes with identical assertion count before/after.
- [ ] Cross-compile clean on x86_64-linux + aarch64-linux.
- [ ] `make lint` + `gitleaks detect` pass.

---

## Applicable Rules

- RULE FLL — Files ≤ 350 lines, methods ≤ 50 lines.
- RULE XCC — Cross-compile before commit.
- RULE ORP — Cross-layer orphan sweep after moves.

---

## Out of Scope

- Renaming or reorganizing public API. Re-exports preserve the current surface.
- Changes to `src/config/env_vars.zig` (201L, in cap) or `src/config/load.zig` (96L, in cap).
- Any behavioral change — pure mechanical move.
