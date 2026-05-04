# M61_002: Small-orphan sweep — clerk shim, sys/error pair, smol_str, route_manifest, etc.

**Prototype:** v2.0.0
**Milestone:** M61
**Workstream:** 002
**Date:** May 04, 2026
**Status:** PENDING
**Priority:** P2 — pre-v2.0.0 hygiene; long tail of single-file orphans found by the May 04 production-only `@import` closure. Each one is small, but together they're ~1.2k LOC of compiled-but-unwired code that lies to readers about what the binary does.
**Categories:** API
**Batch:** B1
**Branch:** feat/m61-small-orphan-sweep
**Depends on:** M61_001 only for sequencing — both can proceed in parallel; this spec just shouldn't merge first if the firewall decision is still open, because the orphan-eval baseline shifts.

**Canonical architecture:** N/A — pure deletion of unreachable surface.

---

## Implementing agent — read these first

1. `docs/v2/pending/M61_001_P1_API_ZIG_FIREWALL_AND_OTEL_EXPORT_REMOVAL.md` — sibling spec; same audit method, same `Eval Commands` codification (E1 production-only orphan eval). Mirror its evidence pattern.
2. `docs/v2/done/M56_001_P2_API_DEAD_CODE_SWEEP_SRC.md` — the Apr 30 sweep that established the prior baseline.
3. `AGENTS.md` — RULE NLG, RULE ORP, Verification Gate, Length Gate.
4. `docs/ZIG_RULES.md` — `pub` audit; if any kept symbol drops its only external caller as a side-effect of these deletions, drop `pub`.
5. `src/main.zig` — the `test {}` bridge block (≈ lines 130-265). Every file in the deletion list appears ONLY there.

---

## Applicable Rules

- `AGENTS.md` — RULE NLG, RULE ORP, Milestone-ID Gate, Verification Gate, Length Gate.
- `docs/greptile-learnings/RULES.md` — RULE ORP, RULE FLL, RULE TST-NAM.
- `docs/ZIG_RULES.md` — `pub` audit, cross-compile.

---

## Overview

**Goal (testable):** After this workstream, the production-only orphan eval (E1 from M61_001) returns zero entries from the file set listed below, `make lint`/`make test`/`make test-integration` pass, cross-compile to `x86_64-linux`+`aarch64-linux` succeeds, and `rg -nF '@import' src/main.zig` no longer references any of the deleted files.

**Problem:** The May 04, 2026 production-only `@import` closure (M61_001 §Overview) flagged 19 files outside production reachability. M61_001 retires 8 (firewall + otel-export). The remaining 11 are small, semantically distinct, and break into five clusters:

1. **`src/auth/clerk.zig`** (82 LOC) — only consumer is `src/auth/tests.zig`. Live Clerk webhook handling routes through `src/http/handlers/webhooks/clerk.zig` (a different file). The `src/auth/clerk.zig` module is a parallel shim with no production caller.
2. **`src/state/workspace_integrations.zig`** (114 LOC) — zero production callers. Workspace-integration storage in production goes through `src/state/workspace_credentials_store.zig` and `src/state/credentials_store.zig`. The `workspace_integrations` table itself was renamed/folded; this Zig module is the orphan that didn't get cleaned up.
3. **`src/secrets/crypto.zig`** (30 LOC) — zero production callers. `src/secrets/crypto_store.zig` (not this file) is the live crypto path; M56_001 §3 sketched cleanup but missed this 30-line stub.
4. **`src/sys/error.zig` (195) + `src/sys/errno.zig` (259)** — pair imports each other and nothing else. No production caller. Both modules predate the standardized error registry in `src/errors/`. ~454 LOC together.
5. **`src/util/strings/smol_str.zig` (294) + `src/util/strings/case_insensitive_ascii_map.zig` (124)** — zero production callers. The string-utility adoption from M55_001 picked `string_joiner.zig`/`string_builder.zig` as the canonical shapes; smol_str + case-insensitive map were never adopted.
6. **`src/http/route_manifest.zig`** (111 LOC) — ZERO references anywhere in `src/` (not even in the test bridge). Pure orphan — cannot tell from the file alone which milestone introduced it.
7. **`src/http/handlers/workspaces/mod.zig`** (8 LOC) — empty `mod.zig` shim; the workspaces handlers are imported directly via `route_table_invoke.zig` and similar.
8. **`src/http/webhook_test_signers.zig`** (215 LOC) — only consumed inside the test bridge of `src/main.zig`; check during PLAN whether any `*_test.zig` actually uses its exports. If so, leave it (it's a test fixture, not production-dead). If not, delete.

**Solution summary:** Delete the orphan files, drop their `_ = @import(...);` lines from `src/main.zig`'s test block, re-run the orphan eval, ship.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/auth/clerk.zig` | DELETE | 82 LOC; not consumed by `auth/tests.zig` after this lands; live Clerk webhook is `handlers/webhooks/clerk.zig` (different file). |
| `src/state/workspace_integrations.zig` | DELETE | 114 LOC; no production caller; superseded by `workspace_credentials_store.zig`. |
| `src/secrets/crypto.zig` | DELETE | 30 LOC; no production caller; live path is `secrets/crypto_store.zig`. |
| `src/sys/error.zig` | DELETE | 195 LOC; only consumer was `sys/errno.zig`; superseded by `src/errors/`. |
| `src/sys/errno.zig` | DELETE | 259 LOC; only consumer was `sys/error.zig`. |
| `src/sys/` (directory) | DELETE if empty after the two files leave | Audit during EXECUTE; if other production files live in `src/sys/`, keep dir. (`ls src/sys/` shows only `errno.zig` and `error.zig` per May 04 audit.) |
| `src/util/strings/smol_str.zig` | DELETE | 294 LOC; never adopted by M55_001; no production caller. |
| `src/util/strings/case_insensitive_ascii_map.zig` | DELETE | 124 LOC; never adopted; no production caller. |
| `src/http/route_manifest.zig` | DELETE | 111 LOC; ZERO references anywhere — pure orphan. |
| `src/http/handlers/workspaces/mod.zig` | DELETE | 8 LOC empty shim; `workspaces` handlers are imported directly. |
| `src/http/webhook_test_signers.zig` | DECIDE | 215 LOC; if `_test.zig` files import it, KEEP; else DELETE. PLAN must run `rg -l 'webhook_test_signers' src/ -g '*_test.zig'` and put the verdict in `Discovery` before EXECUTE. |
| `src/auth/tests.zig` | EDIT | Drop the `_ = @import("clerk.zig");` line at the top of the file. |
| `src/main.zig` | EDIT | Drop the eight `_ = @import(...);` lines for the deleted files from the `test {}` bridge block. |

No schema, HTTP, CLI, UI, or interface changes.

---

## Sections (implementation slices)

### §1 — Triage cluster-by-cluster, decide-then-delete

For each of the seven file clusters, walk: (a) read the file, (b) run `rg -nw '<basename>' src/ -g '!*_test.zig'` to confirm zero non-test consumers, (c) run `rg -nF '@import' src/ | grep '<basename>.zig'` to confirm only the test-bridge import, (d) record verdict in `Discovery`. Then `git rm`. Implementation default: bundle into one commit per cluster (auth, state, secrets, sys, util/strings, http/route_manifest, http/handlers/workspaces, optional webhook_test_signers) so revert is granular.

### §2 — `src/sys/` pair: confirm error-registry handover

Before deleting `sys/error.zig` + `sys/errno.zig`, confirm `src/errors/` (the live error registry) covers the cases the deleted pair handled. Read `src/errors/registry.zig` and the comptime-error-table. If a single error code in `sys/error.zig` lacks a registry counterpart, surface it in `Discovery` with `LEGACY CONSULT` framing: A) port-then-delete, B) delete and accept the tiny coverage gap if no caller, C) keep one of the two. Default A iff a real consumer surfaces; otherwise B.

### §3 — `webhook_test_signers.zig` decision

Run the test-only-consumer check from §Files Changed. Two outcomes: KEEP (test fixture; record in `Discovery`, drop the row from §Files Changed); DELETE (zero consumers; treat like the others).

### §4 — Re-run orphan eval

Run E1 (from M61_001 §Eval Commands). Expected: zero entries from the M61_002 deletion set. If a cascade orphan surfaces (e.g. a helper that becomes unreachable after the cluster lands), surface it in `Discovery` and decide.

### §5 — Verification + ship

Standard `Verification Gate` block: `make lint` + `make test` + `make test-integration` + `make memleak` + cross-compile + Greptile sweep + `/write-unit-test` audit. The expected `make test` count drop is exactly the count of `test "..."` blocks inside the deleted files — record before/after.

---

## Interfaces

N/A — no public interface changes. Each deleted symbol has zero live external callers; the only references are inside `src/main.zig`'s `test {}` bridge and the file's own siblings (which we delete together).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Build break after delete | `@import` resolution missed (path-typo collision, build-module wiring) | `zig build` fails loudly. Restore the file via `git restore`, log the consumer in `Discovery`. |
| `make test` count drops more than expected | A test file inside the deletion list ran cases unrelated to the deleted production code (e.g. `webhook_test_signers.zig` had broader use) | Pause, investigate, decide whether to keep the file or extract its tests into a sibling that survives. |
| `src/sys/` registry-handover gap | An error code lived only in `sys/error.zig` | §2 catches it; port to `src/errors/registry.zig` in the same commit before deleting. |
| `auth/tests.zig` builds after dropping `clerk.zig` | Dropping the `_ = @import("clerk.zig");` line causes a `_test.zig` referencing `clerk.X` to fail to compile | Search `src/auth/` for any test referencing the file's exports; if zero, the drop is safe. The audit shows zero such tests. |

---

## Invariants

1. **Production-only `@import` closure has zero entries from the M61_002 deletion set** — enforced by E1 in CI.
2. **`make test` count drops by exactly the count of `test "..."` blocks in the deleted files** — measurable; record in PR Session Notes.
3. **No file under `src/` after the sweep carries `M{N}_{NNN}` etc. tokens** — Milestone-ID Gate self-audit.
4. **`src/errors/` continues to cover every error code that production code raises** — enforced by the existing error-registry tests; if §2 surfaces a gap, port BEFORE delete.

---

## Test Specification

No new tests. Each deletion is matched by deleting the test that exercised it. Existing tests for live siblings (`crypto_store_test.zig`, `workspace_credentials_store_test.zig`, the `errors/` registry tests, `string_joiner_test.zig`/`string_builder_test.zig`, `route_table_invoke_test.zig` for HTTP) prove the surviving production code still works.

---

## Eval Commands

Reuse E1 from M61_001 §Eval Commands. After this spec lands, the production-only orphan output should be empty (modulo whatever M61_001 left, if it merged in a different order).

Per-cluster grep evidence (record output in PR Session Notes):

```bash
for f in auth/clerk state/workspace_integrations secrets/crypto sys/error sys/errno \
         util/strings/smol_str util/strings/case_insensitive_ascii_map \
         http/route_manifest http/handlers/workspaces/mod http/webhook_test_signers; do
  echo "=== $f ==="
  rg -nF "@import(\"$f.zig\")" src/ | grep -v "src/$f.zig:" | grep -v "_test.zig:"
  rg -nF "@import(\"../$(basename $f).zig\")" "src/$(dirname $f)" 2>/dev/null
done
```

Empty output for every cluster except `webhook_test_signers` is the keep/delete signal.

---

## Discovery (filled during EXECUTE)

`webhook_test_signers.zig` decision: ____
`src/sys/` registry-coverage gap (if any): ____
Cascade orphans surfaced: ____

---

## Out of Scope

- Firewall + otel-export — owned by M61_001.
- SQL schema orphans — owned by M61_003.
- UI orphans — owned by M61_004.
- Refactoring kept files (RULE NLR scope is the files we touch, not the tree).
- Re-introducing any of the deleted modules — file a fresh milestone with engine + wiring + tests landed together.
