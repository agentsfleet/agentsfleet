# M81_001: Split the HTTP test harness into cohesive ≤350-line modules

**Prototype:** v2.0.0
**Milestone:** M81
**Workstream:** 001
**Date:** May 31, 2026
**Status:** PENDING
**Priority:** P2 — test infrastructure hygiene; the single harness file is ~2× the file-length cap and blocks clean edits to it.
**Categories:** API
**Batch:** B1 — standalone; no concurrent dependency.
**Branch:** {feat/m81-001-test-harness-split — added when work begins}
**Depends on:** none.
**Provenance:** LLM-drafted (Opus 4.8, May 31 2026)

> **Provenance is load-bearing.** LLM-drafted — cross-check the symbol inventory against the live file before moving anything; the line ranges below are descriptive, not authoritative.

**Canonical architecture:** none — test infrastructure under `src/zombied/http/`. Shape mirrors the repo's existing `<module>_<concern>.<ext>` sibling-file convention.

---

## Implementing agent — read these first

1. `src/zombied/http/test_harness.zig` — the 678-line file being split; read it whole to inventory the public surface before moving symbols.
2. `docs/gates/file-length.md` — the FLL caps (file ≤350) and the **splitting conventions** (concern-named sibling files, not `harness2.zig`).
3. `docs/gates/pub-surface.md` — moving `pub` types across files trips the PUB gate; read the shape-verdict + re-export expectations.
4. `docs/ZIG_RULES.md` — pub-surface, cross-compile, and `zlint unused-decls` (the orphan check after a move).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Split test_harness.zig into core + server + message + tests modules
- **Intent (one sentence):** Bring the HTTP test harness under the 350-line cap by extracting cohesive modules, with **zero** import or behaviour change for the 22 test files that consume it.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch vs Intent → STOP.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specific IDs the diff trips: **FLL** (the driver — every resulting file ≤350), **NDC** (no dead code left in core after a move), **NLR** (touch-it-fix-it on the file being split), **ORP** (orphan sweep — no symbol left referenced-but-moved or moved-but-unreferenced).
- **`docs/ZIG_RULES.md`** — `*.zig`: pub-surface discipline, cross-compile both linux targets, `zlint unused-decls` catches stragglers.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | **yes** | The whole point. Each resulting file ≤350; no function reshaped (pure move), so the fn/method sub-caps are untouched. |
| PUB / Struct-Shape | **yes** | `Request`/`Response` become `pub` in `test_http_message.zig`; core re-exports them (`pub const Request = @import(...).Request;`) so the consumer surface is byte-identical. Own shape verdict per new file; no inheritance. |
| ZIG GATE | **yes** | Cross-compile `x86_64-linux` + `aarch64-linux`; the harness is in the zombied test build only, but the gate still applies. |
| UFS (repeated/semantic literals) | **yes** | The shared test peppers (`TEST_AUTH_SESSION_PEPPER`, `TEST_AUDIT_LOG_PEPPER`) keep a **single** definition in core and are imported by the split files — never copy-pasted. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | Pure relocation; no log emits, no `init/deinit` reshape, no error codes, no schema. |

---

## Overview

**Goal (testable):** After the split, `make test-integration` compiles and passes all 22 harness-consuming test files with no edit to their `@import`/symbol references, and every file under `src/zombied/http/test_harness*.zig` + `test_http_message.zig` is ≤350 lines.

**Problem:** `test_harness.zig` is 678 lines — nearly 2× the FLL cap — and folds five unrelated concerns into one file. Any net-add to it (e.g. a new harness option) trips the FLL self-audit, so the file can't be evolved cleanly.

**Solution summary:** Mechanically extract three cohesive concerns into sibling files and move the harness's own unit tests into a `_test.zig` file (their correct home, and excluded from the FLL self-audit). Core re-exports the moved public types so every consumer's import surface is unchanged. No behaviour changes — proven by the existing suite passing.

---

## Prior-Art / Reference Implementations

No external prior art — this mirrors the repo's own `<module>_<concern>.zig` sibling convention already used across `src/` (e.g. the `*_test.zig` split that `557454e5` applied to `child_supervisor`). Re-export-from-core to preserve a stable public surface is the standard Zig facade pattern.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/http/test_harness.zig` | EDIT | Shrinks to core: `Config`, `TestHarness` struct + lifecycle (`start`/`deinit`/`connectRedis`/`acquireConn`/`releaseConn`) + verb dispatch (`get`/`post`/…); re-exports `Request`/`Response`; imports the two new helper modules. |
| `src/zombied/http/test_harness_server.zig` | CREATE | Server bring-up plumbing: `defaultRegistry`, the lookup stubs, `bringUpServer`, `serverThread`, `waitForServer`, bind-retry constant. |
| `src/zombied/http/test_http_message.zig` | CREATE | `Request` builder (`header`/`bearer`/`json`/`send`) + `Response` (`expectStatus`/`expectErrorCode`/`bodyContains`). |
| `src/zombied/http/test_harness_test.zig` | CREATE | The harness's own unit tests (`fakeHarness`/`makeResponse` + the `test "…"` blocks) relocated from the core file. |
| `src/zombied/main.zig` | EDIT | Register `_ = @import("http/test_harness_test.zig");` so the relocated tests run (the helper modules carry no `test` blocks and are reached transitively). |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a four-way split along the five concerns, with core as a facade that re-exports moved public types — the minimal cut that gets every file under the cap while keeping the consumer surface frozen.
- **Alternatives considered:** (a) a single `LENGTH GATE: SKIPPED` override on the file — rejected: it perpetuates the god-file and the next editor inherits it; (b) merge `Request`/`Response` into core and only extract the server plumbing — rejected: core stays >350. (c) migrate the 22 consumers to new import paths — rejected: needless churn and review surface; the facade avoids it.
- **Patch-vs-refactor verdict:** this is a **refactor** (multi-file restructure) because the file is structurally over-cap; it is scoped tight (pure relocation, no behaviour change) so it is review-clean by construction.

---

## Sections (implementation slices)

### §1 — Extract the HTTP message types

Move `Request` + `Response` into `test_http_message.zig`; core re-exports them so consumers that reference `harness_mod.Response` / `harness_mod.Request` compile unchanged.

- **Dimension 1.1** — `Request`/`Response` live in the new file and are re-exported from core; all 22 consumers compile → Test `test_message_types_reexported_from_core`
- **Dimension 1.2** — `Response` assertions (`expectStatus`/`expectErrorCode`/`bodyContains`) behave identically post-move → covered by the relocated harness unit tests (§3)

### §2 — Extract the server bring-up plumbing

Move `defaultRegistry`, the lookup stubs, `bringUpServer`, `serverThread`, `waitForServer`, and the bind-attempt constant into `test_harness_server.zig`; core's `start` calls into it.

- **Dimension 2.1** — the harness still starts on a free port with bind-retry and serves requests → Test: any existing `TestHarness.start`-based integration test (e.g. `service_renew_integration_test`) passes

### §3 — Relocate the harness's own tests; land core under the cap

Move `fakeHarness`/`makeResponse` and the `test "…"` blocks into `test_harness_test.zig`; register it in `main.zig`.

- **Dimension 3.1** — the relocated harness unit tests run under the suite and pass → Test: the moved `test "…"` blocks themselves
- **Dimension 3.2** — every resulting file is ≤350 lines → Test/AC: line-count check (E7)

---

## Interfaces

```
Public surface that MUST remain importable from test_harness.zig (unchanged):
  TestHarness (struct) — fields .pool, .ctx, .queue, .alloc + methods
    .start(alloc, Config) .deinit() .connectRedis() .tryConnectRedis()
    .acquireConn() .releaseConn() .get/.post/.put/.delete/.request
  Config (struct)
  Request (struct)  — .header .bearer .json .send         [moved, re-exported]
  Response (struct) — .deinit .expectStatus .expectErrorCode .bodyContains   [moved, re-exported]
```

No signature changes. The only delta is which file a symbol is *defined* in; the import path (`test_harness.zig`) is frozen.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Consumer fails to compile | a moved `pub` type not re-exported from core | `make test-integration` build fails loud naming the missing symbol; fixed by adding the re-export |
| Duplicate constant | a shared pepper copy-pasted into a split file | UFS gate fires; single definition in core imported by the split files |
| Orphan in core | a symbol left behind after its only users moved | `zlint unused-decls` / ORP grep flags it; remove |
| Relocated tests don't run | `test_harness_test.zig` not registered in `main.zig` | test depth-gate count drops; register the import |

---

## Invariants

1. **Public surface unchanged** — every symbol the 22 consumers reference stays importable from `test_harness.zig`. Enforced by the compiler: the consuming suite builds only if the facade is complete.
2. **Each resulting file ≤350 lines.** Enforced by the FLL gate + AC E7.
3. **No behaviour change.** Enforced by the relocated harness unit tests + the full integration suite passing unmodified.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_message_types_reexported_from_core` | a test importing `harness_mod.Response`/`.Request` compiles and the values round-trip |
| 1.2 | unit | relocated `Response` assertion tests | `expectErrorCode`/`expectStatus`/`bodyContains` behave identically to pre-split |
| 2.1 | integration | existing `TestHarness.start` consumer | harness binds a free port (with retry) and serves a request post-split |
| 3.1 | unit | relocated harness unit tests | `fakeHarness`/`makeResponse` blocks pass under the suite |
| 3.2 | n/a | line-count gate (E7) | all `test_harness*.zig` + `test_http_message.zig` ≤350 lines |

**Regression:** the entire `make test-integration` suite is the regression proof — no consumer test changes, all pass. **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [ ] Public surface frozen — verify: `make test-integration` (all 22 consumers compile + pass, none edited)
- [ ] Every split file ≤350 — verify: E7 below
- [ ] No orphaned symbols — verify: `make lint` (`zlint unused-decls`) clean
- [ ] `make lint` clean · `make test` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: consumers untouched — diff names only the harness files + main.zig
git diff --name-only origin/main | grep -E 'http/(test_harness|test_http_message)' && echo "PASS"
# E2: Build — zig build
# E3: Tests — make test-integration
# E4: Lint  — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate over the split files —
wc -l src/zombied/http/test_harness.zig src/zombied/http/test_harness_server.zig \
      src/zombied/http/test_http_message.zig | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

**1. Orphaned files** — none deleted; the split moves code, it does not remove a file.

N/A — no files deleted.

**2. Orphaned references** — after each move, grep core for the moved symbol; non-zero in core (outside the re-export line) = a straggler.

| Moved symbol | Grep | Expected |
|--------------|------|----------|
| `bringUpServer` | `grep -n "fn bringUpServer" src/zombied/http/test_harness.zig` | 0 (lives in `_server.zig`) |
| `makeResponse` | `grep -n "fn makeResponse" src/zombied/http/test_harness.zig` | 0 (lives in `_test.zig`) |

---

## Discovery (consult log)

> Empty at creation. Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- Origin: surfaced during M80_006 (PR #354) CTO review of `test_harness.zig`; Indy directed a dedicated spec, filed alongside M80_006 in PR #354.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; coverage vs Test Spec recorded in Discovery |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | Comments addressed before merge |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration tests | `make test-integration` | {paste} | |
| Lint (incl. unused-decls) | `make lint` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Line-count gate | E7 | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- No new harness capabilities, verbs, or options (a separate follow-up adds the `balance_policy` injection used by M80_006).
- No migration of consumers to new import paths — the facade keeps `test_harness.zig` as the single entry point.
