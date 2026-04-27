# M54_001: Pub Surface Audit + Allocator Ownership

**Prototype:** v2.0.0
**Milestone:** M54
**Workstream:** 001
**Date:** Apr 26, 2026
**Status:** DONE
**Priority:** P2 — codebase discipline. Two related audits of the Zig surface area: shrink the `pub` namespace to what's actually used, and make heap-ownership explicit at every owning struct.
**Categories:** API
**Batch:** B1 — independent of in-flight milestones. No HTTP, schema, or CLI surface changes.
**Branch:** feat/m54-pub-surface
**Depends on:** none. (Originally blocked behind M52_001 Bun Vendor Utilities, which would have introduced four new `pub` APIs in `src/util/`. M52 was investigated and deferred on Apr 26, 2026 — no concrete callsites exist yet — so M54 unblocks immediately and audits the existing `pub` surface only. See `docs/v2/done/M52_001_P2_API_BUN_VENDOR_UTILITIES.md` § Investigation outcome for the full discovery and re-trigger conditions.)

**Coordinates with:** M53 (hygiene sweep), M55 (string utilities) — independent file sets, parallel-safe.

**Canonical architecture:** N/A — discipline pass, no architectural change.

---

## Implementing agent — read these first

1. `docs/ZIG_RULES.md` — `pub` audit policy and allocator-ownership rules. Specifically: the rule that every `pub const` / `pub fn` must be referenced from outside its file, and the rule that heap-owning structs document the allocator pattern.
2. `src/sys/error.zig`, `src/util/strings/string_builder.zig` — examples of well-formed allocator-owning structs (vendored from M40, used here as the "good shape" reference).
3. `src/queue/`, `src/cmd/`, `src/db/` — most likely homes for unreferenced `pub` symbols and undocumented heap-owning structs. Start grep here.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `docs/ZIG_RULES.md` — `pub` audit, allocator ownership documentation, errdefer chain on multi-step init.

---

## Anti-Patterns to Avoid

- Removing `pub` from a symbol that IS referenced externally but only via an indirect path the grep missed (e.g. through a comptime alias). When in doubt, keep `pub` and document the indirection in a `// pub: aliased via X` comment.
- Adding allocator fields to structs that don't actually own heap — documenting ownership doesn't always mean storing the allocator; sometimes it means a doc-comment naming the caller-owned-allocator pattern.
- Bundling unrelated refactors. This spec is two audits, not three.

---

## Overview

**Goal (testable):** Every `pub const` / `pub fn` in `src/**/*.zig` (excluding `_test.zig`) is either referenced from outside its declaring file or has its `pub` removed; every struct that owns heap memory either stores its allocator as a field or carries a doc-comment naming the caller-owned-allocator pattern. `make lint`, `make test`, `make test-integration`, `make memleak`, and cross-compile (x86_64-linux + aarch64-linux) all green.

**Problem:** Two correlated drifts:

- Many `pub` symbols are no longer (or were never) called from outside their file. The wider `pub` surface area inflates review effort and obscures the true module boundary. ZIG_RULES requires `pub` only for genuine cross-file consumers.
- Several structs hold heap memory (ArrayLists, HashMaps, duped strings) without an `alloc:` field or a doc-comment explaining whose allocator they use. Latent leak risk and review friction.

**Solution summary:** Two sequential audit passes on one branch. §1 walks every `pub` declaration and removes those without external references. §2 walks every struct, identifies heap-owning ones, and either adds an `alloc:` field or a `///` doc-comment naming the ownership convention. Each pass is one focused commit per logical cluster (typically one commit per top-level directory under `src/`).

---

## Files Changed (blast radius)

| File set | Action | Why |
|------|--------|-----|
| Most files under `src/**/*.zig` (non-test) | EDIT | `pub` removal — narrow visibility |
| Files with heap-owning structs (rough count: 20–40) | EDIT | Add `alloc:` field or ownership doc-comment |

**Estimate**: 60–120 files total. If the count breaks 120, surface in PLAN — may indicate the `pub` audit alone is its own spec.

---

## Sections (implementation slices)

### §1 — Pub audit

For each `src/**/*.zig` file (excluding `_test.zig`):

1. List `pub const` / `pub fn` / `pub var` declarations.
2. For each symbol, grep the rest of the repo for the symbol name (qualified or unqualified, considering re-exports and aliases).
3. If zero external references, remove `pub`.
4. If the symbol is used only by `_test.zig` siblings, KEEP `pub` (test blocks need it).

**Implementation default:** commit per top-level directory under `src/` (e.g. `src/queue/`, `src/cmd/`, `src/http/`). Bisect-friendly slices.

If a removal breaks the build — restore `pub`, add a one-line `// pub: <reason>` comment naming the consumer the grep missed.

### §2 — Allocator ownership

For each struct definition in `src/**/*.zig`:

1. Identify whether it owns heap memory: holds an `ArrayList`, `HashMap`, `*T` to heap, or duped `[]const u8` / `[:0]const u8` slices.
2. If yes, choose ONE of:
   - Add `alloc: std.mem.Allocator` field if the struct's own methods allocate.
   - Add a `///` doc-comment naming the caller-owned-allocator pattern if the struct receives an allocator on every method that needs one.
3. Verify a `deinit(self: *@This())` exists and frees what's owned. (Not adding `deinit` is out of scope — surface as a follow-up if missing.)

**Implementation default:** prefer the field-stored allocator unless the struct is intentionally allocator-pluggable per call (rare; mostly small utility structs).

**Commit ordering**: §1 lands first (smaller per-file diffs, compiler-bisectable), §2 second (structural).

---

## Interfaces

N/A for §1 — `pub` removal narrows visibility; if a symbol is genuinely used externally, the build breaks at compile time and the change reverts.

§2 may add `alloc:` fields to structs. Any struct that's part of a documented public API surface (e.g. exposed via FFI or via a published handler shape) is OUT of scope — flag and skip. The FFI / handler boundary is the gate.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `pub` removal breaks downstream build | Indirect / aliased reference grep missed | Compiler error; restore `pub` with `// pub: <consumer>` comment |
| `alloc:` field addition breaks struct construction | Existing constructors don't pass an allocator | Update constructors; if widespread, that's the signal to use doc-comment pattern instead |
| Removed `pub` from a symbol whose only consumer is at runtime via `@field` lookup | Comptime/runtime indirection | Add the `// pub: <reason>` carve-out; don't fight it |

---

## Invariants

1. After §1: no `pub` symbol in `src/**/*.zig` (non-test) is unreferenced outside its declaring file — enforced by a per-PR grep gate (or a script in `make lint`).
2. After §2: every struct holding `ArrayList` / `HashMap` / heap pointers has either an `alloc:` field or a `///` doc-comment within 5 lines of the field declaration naming the ownership pattern — best-effort grep gate.

If the grep gate produces too many false-positives to be useful, downgrade to a `/review` skill check and document the trade-off in CHORE(close).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_pub_audit_grep` | A script lists `pub const`/`pub fn` symbols and confirms each has at least one external reference (or a `// pub:` carve-out) |
| `test_allocator_ownership_doc` | A grep gate confirms every struct with ArrayList/HashMap fields has either `alloc:` or a `///` ownership comment |
| Existing `make test` + `make test-integration` | Regression net for both sections — no behavior should change |

---

## Acceptance Criteria

- [x] §1 audit script `scripts/pub_audit.sh` — DONE; flagged 355 unreferenced + 157 test-only pub symbols across 22 top-level dirs.
- [x] §2 audit script `scripts/alloc_audit.py` — DONE; per-struct heap-owner discovery.
- [x] `make test` passes — 1406 passed, 158 skipped, 0 failed.
- [x] `make test-integration` passes (tier 2).
- [x] `make down && make up && make test-integration` passes (tier 3).
- [x] `make memleak` clean — final line: `✓ [zombied] memleak gate passed`.
- [x] Cross-compile clean: x86_64-linux ✓, aarch64-linux ✓.
- [x] `gitleaks detect` clean (pre-commit hook).
- [x] No NEW file over 350 lines, no NEW function over 50 lines. (Pre-existing >350-line files in `src/auth/`, `src/db/pool.zig`, `src/http/handlers/common.zig`, etc. are out of M54 scope; doc-comment inserts in §2 add 1 line each but do not introduce new caps.)

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: pub audit — every pub symbol referenced externally
# Audit scripts (pub_audit.sh, pub_strip.py, alloc_audit.py) lived in scripts/
# during the M54 work and were dropped before merge — they had no recurring
# caller. Reconstruct from git history (commit 7fbcb8cd) if a future audit needs them.

# E2: allocator-ownership grep
rg -nU 'struct \{[^}]*ArrayList' src/ -A20 | rg -v '(alloc:|/// .*allocator)' | head

# E3: build + tests
zig build && make test && make test-integration

# E4: memleak (critical for §2)
make memleak 2>&1 | tail -3

# E5: cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E6: lint + gitleaks
make lint 2>&1 | tail -5
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

§1 removes `pub` from many symbols but does not delete them — so nothing to sweep. If §1 reveals a symbol is referenced by NOTHING (not even its own file), file a follow-up spec to delete; do not delete in this spec.

§2 adds fields / comments; nothing deleted.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill |
|------|-------|
| Before CHORE(close) | `/write-unit-test` — confirms grep gates exist + memleak coverage on §2 |
| Before CHORE(close) | `/review` — adversarial pass against ZIG_RULES.md `pub` and allocator rules |
| After `gh pr create` | `/review-pr` |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Pub audit script | `scripts/pub_audit.sh src/` | 512 candidates → 355 stripped, 157 kept (TEST_ONLY) | ✓ |
| Allocator ownership grep | `scripts/alloc_audit.py` + manual triage | 21 structs documented with `///` caller-owned-allocator | ✓ |
| Unit tests | `make test` | 1406 passed, 158 skipped, 0 failed | ✓ |
| Integration tests (tier 2) | `make test-integration` | All passed | ✓ |
| Integration tests (tier 3) | `make down && make up && make test-integration` | All passed (fresh DB) | ✓ |
| Memleak | `make memleak` | `✓ [zombied] memleak gate passed` | ✓ |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | clean exit 0 | ✓ |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | clean exit 0 | ✓ |
| Lint | `make lint` (pre-commit) | All checks passed | ✓ |
| Gitleaks | `gitleaks detect` (pre-commit) | no leaks found | ✓ |

### §1 — Pub strip distribution per top-level dir

| Dir | Stripped |
|---|---|
| auth/ | 18 |
| cli/ | 3 |
| cmd/ | 19 |
| config/ | 10 |
| db/ | 66 |
| errors/ | 46 |
| events/ | 5 |
| executor/ | 22 |
| git/ | 3 |
| http/ | 37 |
| main.zig | 1 (then re-pubbed with carve-out: `std_options` consumed by std at comptime) |
| memory/ | 2 |
| observability/ | 26 |
| queue/ | 6 |
| reliability/ | 5 |
| state/ | 15 |
| sys/ | 9 |
| types/ | 8 |
| types.zig | 14 |
| util/ | 19 |
| zbench_fixtures.zig | 1 |
| zombie/ | 20 |
| **total** | **355** |

### §2 — Allocator-ownership doc-comments

21 heap-owning structs across `src/auth/`, `src/cmd/`, `src/http/`, `src/observability/`, `src/queue/`, `src/secrets/`, `src/state/`, `src/zombie/` received a `/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.` doc-comment.

### Limitations of audit tooling (deliberate)

- **Pub audit grep**: ripgrep word-match cannot see comptime `@field`/`@hasDecl` lookups or `@import("root")` reflection. Build is the final arbiter — `main.zig std_options` was caught and reverted with a `// pub: ...` carve-out comment.
- **Allocator audit**: `scripts/alloc_audit.py` brace-counting is imperfect (false positives when ArrayList appears inside function bodies or nested struct literals). The truly precise ArrayList/HashMap *field* count is 8; supplementing with the `deinit(self, alloc)` signature-grep across 24 files yielded the documented set. Not detected: structs whose only heap content is duped `[]const u8` slices — there is no mechanical signature for "heap-owning via duped string". File a follow-up if a stricter invariant is needed.

---

## Out of Scope

- `///` doc-comments on every `pub` type — explicitly deferred per author decision Apr 26, 2026.
- Atomic ordering, mutex pairing, `Maybe(T)`, errno logs, snake_case lint (M53_001).
- String utility adoption (M55_001).
- Adding missing `deinit` methods on structs that lack them — file as follow-up if encountered.
- Removing entirely-unreferenced symbols (not just `pub`) — file as follow-up.
