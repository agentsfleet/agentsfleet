# M28_004: Split `src/zombie/config.zig` into per-concern modules

**Prototype:** v0.19.0
**Milestone:** M28
**Workstream:** 004
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P3 — deferrable; no user-visible behavior change
**Batch:** B1
**Branch:** _unassigned_
**Depends on:** M28_001 (webhook auth; last addition that pushed the file over the 350-line cap)

---

## Overview

**Goal (testable):** `src/zombie/config.zig` (currently 448 lines) is split into per-concern modules so no single file exceeds the 350-line RULE FLL cap, with zero behavior change. Public API surface preserved; all imports updated; all tests still pass.

**Why deferred from M28_001.** M28_001 §3 added 17 lines (`WebhookSignatureConfig` + `InvalidSignatureConfig` + deinit wiring) to a file that was already at 431 lines. Splitting at that point would have substantially ballooned the M28_001 diff with a mechanical refactor unrelated to the security/feature delivery. The violation is documented in the M28_001 Ripley's Log and deferred here for clean history.

---

## Proposed split

| New file | Extracted content | Approx lines |
|---|---|---|
| `src/zombie/config.zig` (kept) | `ZombieConfigError`, `ZombieStatus`, top-level `ZombieConfig` struct + `deinit` wiring | ~180 |
| `src/zombie/config_trigger.zig` (new) | `ZombieTriggerType`, `ZombieTrigger` union, `WebhookSignatureConfig` + `MAX_SIGNATURE_HEADER_LEN` | ~90 |
| `src/zombie/config_skill.zig` (new) | Skill/credential reference types if present | ~80 |
| `src/zombie/config_budget.zig` (new) | Budget + retry types if present | ~80 |

Final line distribution kept under 250 per file to leave headroom.

**Imports across the codebase** — every file importing `config.ZombieTrigger` or `config.WebhookSignatureConfig` needs its import path updated. Affected callers (rough grep — verify at implementation):
- `src/zombie/config_helpers.zig`
- `src/cmd/serve_webhook_lookup.zig`
- `src/cmd/common.zig` (trigger dispatch)
- Tests under `src/zombie/*_test.zig`

---

## Execution Plan

1. Inventory every public symbol in `config.zig` and every external caller.
2. Extract `WebhookSignatureConfig` + trigger types into `config_trigger.zig`. Re-export from `config.zig` as `pub const WebhookSignatureConfig = @import("config_trigger.zig").WebhookSignatureConfig;` — keeps all current imports working without call-site edits. Saves blast radius.
3. Run `zig build` after each extraction. Full `zig build test` after all extractions.
4. Re-run VERIFY gates: `make lint`, `make check-pg-drain`, cross-compile, `make memleak`.
5. Assert every touched/new `.zig` file ≤ 350 lines via the 350-line gate command.

---

## Acceptance Criteria

- [ ] `src/zombie/config.zig` ≤ 350 lines.
- [ ] No new file created by this workstream exceeds 250 lines.
- [ ] All existing imports continue to work (via re-exports where needed) — no call-site churn required for consumers.
- [ ] `zig build test` passes with identical assertion count before/after.
- [ ] `make test-integration` passes.
- [ ] `make memleak` passes.

---

## Applicable Rules

- RULE FLL — Files ≤ 350 lines.
- RULE ORP — Cross-layer orphan sweep after moves.

---

## Out of Scope

- Renaming or reorganizing public API. Re-exports preserve the current surface.
- Splitting `config_helpers.zig` (currently within cap).
- Any behavioral change — this is a pure mechanical move.
