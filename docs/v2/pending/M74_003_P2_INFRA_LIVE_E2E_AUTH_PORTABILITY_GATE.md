<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_003: Restore `src/auth/` portability gate (`make live-e2e-auth`)

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 003
**Date:** May 19, 2026
**Status:** PENDING
**Priority:** P2 — gate is a build-time invariant, not a runtime correctness issue. The portability contract is still intact in the main binary build; only the isolation gate fails. Useful as a CI block once restored.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m74-003-live-e2e-auth-portability (to be created on CHORE(open))
**Depends on:** None.
**Provenance:** Surfaced during M74_001 Piece 1 closeout (`docs/v2/done/M74_001_P1_CLI_EFFECT_TS_MIGRATION.md` — Piece 1 Closeout addendum, May 19, 2026).

**Canonical architecture:** `build.zig` named-module pattern (see `log_mod` / `hmac_sig_mod` at `build.zig:122-145`).

---

## Implementing agent — read these first

1. `build.zig` lines 122-145 — the existing `hmac_sig_mod` + `log_mod` definitions. The pattern to mirror exactly.
2. `build.zig` lines 318-342 — the `test-auth` step definition + its current `.imports` list (`httpz`, `hmac_sig`, `log`).
3. `src/errors/error_registry.zig` — the module that needs to be exposed as a named import. Pure stdlib, depends only on sibling files `error_entries.zig` + `error_entries_runtime.zig`. No cross-layer pull-back.
4. `src/auth/middleware/errors.zig:9` + `src/auth/middleware/tenant_api_key.zig:33` — the two violating sites currently using `@import("../../errors/error_registry.zig")`.
5. `make/acceptance.mk:75` — the `live-e2e-auth` target body that runs `zig build test-auth --summary all`.
6. Historical context: regression introduced by `b1728ea0 refactor(m62_001): unify hardcoded UZ-* error codes via error_registry` (the M62 refactor reached outside `src/auth/` via a relative path without registering a named module).

---

## Applicable Rules

- `docs/ZIG_RULES.md` — applies. The fix is a module wiring change; no new pub surface beyond the named-module export.
- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it) applies if the diff lands near related dead code; RULE NLG forbids legacy framing.
- `docs/gates/pub-surface.md` — PUB GATE fires on any new `^pub` lines in `build.zig` (none expected — adding a module is not a pub surface change).

---

## Overview

**Goal (testable):** `make live-e2e-auth` exits 0 against the current `src/auth/` tree. The `zig build test-auth` step compiles `src/auth/tests.zig` in isolation with `src/auth/`-bounded module path, and the `error_registry` named module is reachable from every consumer that previously used a relative `../../errors/error_registry.zig` import.

**Problem:** The `src/auth/` portability gate exists to guarantee `src/auth/` is extractable into a standalone `zombie-auth` binary. The gate fails today because two middleware files reach outside the `src/auth/` module path via relative imports introduced by M62_001:

```
src/auth/middleware/errors.zig:9: error: import of file outside module path
src/auth/middleware/tenant_api_key.zig:33: error: import of file outside module path
```

The portability contract is unchanged in the main binary build (where `src/main.zig` is the module root and the relative paths resolve fine), but the gate that enforces it doesn't run as part of regular CI today — the regression went undetected since M62 landed.

**Solution summary:** Register `error_registry` as a named module in `build.zig` (mirroring `hmac_sig_mod` + `log_mod`). Wire it into every consumer's `.imports` list (main exe, `test`, `test-auth`, any bench/integration target that compiles `src/auth/middleware/*`). Convert the two relative imports to `@import("error_registry")`. Sweep the rest of the codebase for other relative imports of `error_registry.zig` and convert for consistency.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig` | EDIT | Add `S_ERROR_REGISTRY` const + `error_registry_mod = b.createModule(...)`. Add `{ .name = S_ERROR_REGISTRY, .module = error_registry_mod }` to every consumer's `.imports`. |
| `src/auth/middleware/errors.zig` | EDIT | Convert `@import("../../errors/error_registry.zig")` → `@import("error_registry")`. |
| `src/auth/middleware/tenant_api_key.zig` | EDIT | Same conversion. |
| (other relative-import sites — discover via grep) | EDIT | Convert for consistency. |

---

## Sections (implementation slices)

### §1 — Named-module registration

Add the module definition to `build.zig` adjacent to `log_mod`. Wire it into the main exe's `.imports`, the `test` step's `.imports`, and the `test-auth` step's `.imports`. Verify each compiles in isolation.

### §2 — Auth middleware import conversion

Convert both relative imports in `src/auth/middleware/` to named-module form. Run `zig build test-auth --summary all` — must exit 0.

### §3 — Codebase sweep + lint

`grep -rn '"../.*errors/error_registry.zig"' src/` must return zero matches. Run full `zig build test` to ensure the main binary build still passes.

---

## Acceptance Criteria

- `make live-e2e-auth` exits 0.
- `zig build` exits 0 (full binary build still works).
- `zig build test` exits 0 (Zig unit tests still pass).
- `grep -rn '"../.*errors/error_registry.zig"' src/` returns zero matches.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| `make live-e2e-auth` green | `src/auth/` compiles in isolation against the bounded module path | CI |
| `zig build` green | main binary build still works post-rewiring | CI |
| `zig build test` green | Zig unit tests still pass | CI |

---

## Discovery

(none — single-PR fix, blast radius limited)

---

## Out of Scope

- Extracting `src/auth/` to a standalone `zombie-auth` repository. The gate ensures *extractability*; the actual extraction is a separate decision.
- Adding `error_registry` to bench targets that don't currently compile `src/auth/middleware/*`. Need-based — only the consumers that import the registry get the module wired.
