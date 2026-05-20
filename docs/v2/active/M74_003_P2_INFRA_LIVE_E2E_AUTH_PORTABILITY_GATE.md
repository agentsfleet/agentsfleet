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
**Status:** IN_PROGRESS
**Priority:** P2 — gate is a build-time invariant, not a runtime correctness issue. The portability contract is still intact in the main binary build; only the isolation gate fails. Useful as a CI block once restored.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m74-003-live-e2e-auth-portability
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

## PR Intent & comprehension handshake

> The bridge from spec to merged PR — the agent confirms intent before writing code.

- **PR title (eventual):** Restore src/auth/ portability gate via named error_registry module
- **Intent (one sentence):** `make live-e2e-auth` goes green again — `src/auth/` compiles in isolation, proving it stays extractable into a standalone `zombie-auth` binary.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list the assumptions you proceed on (`ASSUMPTIONS I'M MAKING: …`). Two to name: (1) registering a `b.createModule` named module is build wiring, not a new `pub` surface — PUB GATE should not fire; (2) *every* compiling target that pulls `src/auth/middleware/*` must get `error_registry` in its `.imports`, or the main `zig build` breaks. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- `docs/ZIG_RULES.md` — applies. The fix is a module wiring change; no new pub surface beyond the named-module export.
- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it) applies if the diff lands near related dead code; RULE NLG forbids legacy framing.
- `docs/gates/pub-surface.md` — PUB GATE fires on any new `^pub` lines in `build.zig` (none expected — adding a module is not a pub surface change).

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: `build.zig` + two `src/auth/middleware/*.zig` imports + a codebase sweep. Zig only.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | read `docs/ZIG_RULES.md`; cross-compile both linux targets (`zig build -Dtarget=x86_64-linux` + `-Dtarget=aarch64-linux`) after rewiring. |
| PUB / Struct-Shape | no | adding a named module via `b.createModule` is build wiring, not a new `pub` decl; confirm no new `^pub` lines in the diff. |
| File & Function Length (≤350/≤50/≤70) | no | a few additive lines in `build.zig`; the import conversions are 1-for-1. |
| UFS (repeated/semantic literals) | yes | the module name `"error_registry"` recurs across `build.zig` + every `@import` — name it once as `S_ERROR_REGISTRY`, mirroring `hmac_sig_mod`/`log_mod`, and reference the const. |
| UI Substitution / DESIGN TOKEN | no | no `*.tsx`/`*.jsx`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | wiring `error_registry` as a module touches no `UZ-*` codes, no init/deinit, no schema. |

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

## Prior-Art / Reference Implementations

> Mirror the existing build-graph pattern exactly — don't invent a new module shape.

- **In-repo** → `build.zig:122-145` — the `hmac_sig_mod` + `log_mod` named-module definitions. The `error_registry` module is a verbatim mirror: `b.createModule(...)` + a `S_*` name const + an `.imports` entry on every consuming target.
- **Alignment:** no divergence — this restores a pattern the repo already uses for two other shared modules. Not greenfield; the shape is defined at `build.zig:122-145`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig` | EDIT | Add `S_ERROR_REGISTRY` const + `error_registry_mod = b.createModule(...)`. Add `{ .name = S_ERROR_REGISTRY, .module = error_registry_mod }` to every consumer's `.imports`. |
| `src/auth/middleware/errors.zig` | EDIT | Convert `@import("../../errors/error_registry.zig")` → `@import("error_registry")`. |
| `src/auth/middleware/tenant_api_key.zig` | EDIT | Same conversion. |
| (other relative-import sites — discover via grep) | EDIT | Convert for consistency. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — register the named module, convert the two violating imports, sweep + verify. Mirrors the existing `hmac_sig_mod`/`log_mod` pattern.
- **Alternatives considered:** (a) move `error_registry.zig` under `src/auth/` to satisfy the bounded path — rejected; the registry is shared across layers, not auth-specific. (b) relax the portability gate's module boundary — rejected; that defeats the gate's whole purpose (proving `src/auth/` extractability).
- **Patch-vs-refactor verdict:** this is a **patch** — a localized build-wiring fix restoring a pre-existing invariant that the M62 refactor broke. No architecture change.

---

## Sections (implementation slices)

### §1 — Named-module registration

Add the module definition to `build.zig` adjacent to `log_mod`. Wire it into the main exe's `.imports`, the `test` step's `.imports`, and the `test-auth` step's `.imports`. Verify each compiles in isolation.

### §2 — Auth middleware import conversion

Convert both relative imports in `src/auth/middleware/` to named-module form. Run `zig build test-auth --summary all` — must exit 0.

### §3 — Codebase sweep + lint

`grep -rn '"../.*errors/error_registry.zig"' src/` must return zero matches. Run full `zig build test` to ensure the main binary build still passes.

---

## Interfaces

Build-graph interface — a named module mirroring the existing `hmac_sig_mod` / `log_mod`:

```
build.zig:
  const S_ERROR_REGISTRY = "error_registry";
  const error_registry_mod = b.createModule(.{ .root_source_file = b.path("src/errors/error_registry.zig") });
  // wired into each consumer's .imports:
  //   exe · test step · test-auth step → { .name = S_ERROR_REGISTRY, .module = error_registry_mod }

consumer source (src/auth/middleware/*.zig):
  @import("error_registry")   // replaces @import("../../errors/error_registry.zig")
```

No HTTP/REST surface, no OpenAPI change, no CLI change. The only public contract is the build-graph module name `error_registry`, shared verbatim across `build.zig` and every consumer.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `test-auth` still fails "import of file outside module path" | A consumer of `error_registry` was missed when wiring `.imports` | Grep every `@import("../.*errors/error_registry.zig")` site (§3) and confirm each compiling target carries the module in its `.imports`. |
| Main binary build breaks after rewiring | A source file uses the named import but the exe target lacks the module | Wire the exe consumer in §1 *before* converting imports in §2; `zig build` must stay green throughout. |
| A future consumer reaches in via a relative path again | A later edit reintroduces the M62-class regression | The restored `make live-e2e-auth` gate catches it in CI; the grep invariant below is the durable guard. |
| Cross-compile target diverges | A target-specific `.imports` list misses the module | Run both `zig build -Dtarget=x86_64-linux` and `-Dtarget=aarch64-linux` post-rewiring. |

---

## Invariants

1. **`make live-e2e-auth` exits 0** — `src/auth/` compiles in isolation against its bounded module path. Enforced as a CI gate once restored.
2. **`zig build` and `zig build test` stay green** — the main binary build and unit tests are unaffected by the rewiring.
3. **Zero relative imports of `error_registry.zig` remain** — enforced by `grep -rn '"../.*errors/error_registry.zig"' src/` returning empty.
4. **No new `pub` surface** — adding a named module is build wiring; `error_registry`'s own pub surface is unchanged.

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
