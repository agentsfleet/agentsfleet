# M83_001: Remove verified-dead src/ files & exports; re-aggregate two orphan tests

**Prototype:** v2.0.0
**Milestone:** M83
**Workstream:** 001
**Date:** Jun 03, 2026
**Status:** DONE
**Priority:** P2 — pre-2.0 hygiene (RULE NDC/NLG); no customer-facing behaviour change.
**Categories:** API, DOCS
**Batch:** B1 — standalone; no concurrent workstream.
**Branch:** feat/m83-dead-code-cleanup
**Depends on:** none.
**Provenance:** agent-generated (folder-by-folder reachability audit of src/, Jun 03 2026; method + evidence in memory `project_dead_code_audit_method.md`).

> **Provenance is load-bearing.** Every dead-code claim below was adversarially verified (a skeptic per candidate tried to *refute* deadness) against BOTH build graphs. Trust the claims, but the agent re-confirms each with the dead-code sweep before deleting.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (runner plane) + `docs/AUTH.md` (auth middleware registry). Cleanup spans both build graphs (`zombied` + `zombie-runner`) and `src/lib`.

---

## Implementing agent — read these first

1. `~/.claude/projects/-Users-kishore-Projects-usezombie/memory/project_dead_code_audit_method.md` — THE method. Two build graphs + **transitive test-block reachability**: a `*_test.zig` runs when a reached parent pulls it in via `test {}`/`comptime {}`. Do NOT treat "not imported by `tests.zig` directly" as dead — that produced 16 false positives. Only the two files in §1 are genuine orphans.
2. `src/runner/engine/runner.zig` (~line 303) — the engine test-block aggregator (`test { _ = @import("…_test.zig"); }`). This is the RULE ORP pattern to mirror in §1, and the live home of the surviving security suites in §4.
3. `src/zombied/auth/middleware/mod.zig` + `bearer_or_api_key.zig:109` — the `MiddlewareRegistry`; `bearer_or_api_key` lifts `.platform_admin` identically to the dead `bearer_oidc` (both off `claims.zig`), which is why §3's deletion loses no auth gating.
4. `docs/ZIG_RULES.md` — "Progressive Cleanup (apply on file touch)" + "New File Rules" + cross-compile.
5. `docs/greptile-learnings/RULES.md` — NDC, NLR, NLG, UFS, ORP.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Remove verified-dead src/ code; restore the one live orphan test
- **Intent (one sentence):** Strip pre-2.0 dead code an exhaustive reachability audit proved unreachable, restore the one genuinely-live orphaned test suite (`policy_http_request_test`) — deleting the other (`session_test`) once post-merge review found its subject `session.zig` itself dead — and fix the stale comments/doc-paths that masked the rot, leaving `src/` honest with zero behaviour change.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. A mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (no dead code at write time), **NLR** (touch-it-fix-it on every file touched), **NLG** (pre-2.0: no legacy/compat framing — applies to the `bearer_oidc`/`security_headers` removals and reworded comments), **UFS** (§5 `wire.zig` constant triage), **ORP** (orphan sweep — §1 re-aggregation + every deletion).
- **`docs/ZIG_RULES.md`** — Progressive Cleanup, New File Rules, ZLint Policy (`unused-decls: error` is the safety net for symbol-level removals), Cross-Compile Verification. Diff is almost entirely `*.zig`.
- **`docs/AUTH.md`** — re-read before §3 (auth middleware) and §5 (path fixes); the live model is `bearer_or_api_key`.
- REST / SCHEMA / BUN / LOGGING rule files — **N/A** (no handlers, schema, Bun, or log-emit surfaces change).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` edited/deleted | Build BOTH graphs + cross-compile both linux targets after each section; read ZIG_RULES. |
| PUB / Struct-Shape | yes — removing `pub` surface (`BearerOidc`, `ClientError`, `applyHsts`, `SessionStore`, `SUPPORT_EMAIL`, `wire.*`) | Removal-only; verdict per surface = "last consumer gone → delete." zlint `unused-decls` confirms no new dead `pub`. |
| File & Function Length | no | Net-removing lines; no file approaches the cap. |
| UFS | yes — §5 `wire.zig` | Per unused constant: wire the caller to it OR remove it; never leave a defined-unused single-source constant. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | `client_errors.zig` keeps its live `ERR_*` codes (registry untouched); no log-emit, lifecycle, or schema change. |
| UI / DESIGN TOKEN | no | No `ui/` files. |

---

## Overview

**Goal (testable):** After this PR, `zig build` + `zig build --build-file build_runner.zig` + cross-compile both linux targets pass, `make test` + runner tests are green, the dead-code sweep greps all return zero, and `policy_http_request_test` now compiles and runs — with no production response or behaviour changed. (The audit originally listed `session_test` as a second orphan, but post-merge review found its subject `session.zig` is itself dead → deleted, not restored. See §1 correction.)

**Problem:** A folder-by-folder reachability audit (387 `.zig` files, two build graphs) found a small set of files/exports nothing reaches, plus an orphaned test suite (`policy_http_request_test`) that never compiles because no aggregator pulls it in, plus stale comments and doc-paths (`src/auth/` post-M80) that actively mislead. Pre-2.0 RULE NDC/NLG say this cruft should not linger.

**Solution summary:** Delete the verified-dead files and exports; re-aggregate the one live orphan test (`policy_http_request_test`) via the existing test-block pattern (RULE ORP); triage `wire.zig`'s unused single-source constants (use or remove); and fix the stale comments/doc-paths the audit surfaced. Pure hygiene — the only *behavioural* deltas are that one previously-dead test suite (`policy_http_request_test`, ~10 tests) now runs, and the dead-`Session` test ecosystem (−39 tests, see §1/§4 corrections) is removed.

---

## Prior-Art / Reference Implementations

- **Orphan re-aggregation** → `src/runner/engine/runner.zig`'s `test { _ = @import("…_test.zig"); }` block (the RULE ORP precedent that re-homed the engine suites after the M80 cutover deleted `runner_test.zig`). §1 mirrors it.
- **Auth deletion safety** → `docs/AUTH.md` + `src/zombied/auth/middleware/bearer_or_api_key.zig` (the live JWT path that subsumes `bearer_oidc`).
- No new abstractions; this is removal + re-wiring against existing patterns.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/session_test.zig` | DELETE | **Correction (Indy, post-merge):** `session.zig`'s `Session` is dead post-M80 (the runner is stateless: `runner.execute` → `ExecutionResult`; no prod caller). This suite tested dead code → deleted, not restored. |
| `src/runner/engine/runtime/policy_http_request_test.zig` | EDIT | Fix drift vs current `policy_http_request.zig`; keep (live equivalent — `PolicyHttpRequestTool` is instantiated at `tool_builders.zig:184`). |
| `src/runner/engine/runner.zig` | EDIT | Drop the dead-`Session` suites (`session_test`/`sandbox_edge_test`/`resource_security_test`) from the engine test block; keep `runner_security_test` + `runner_progress_redact_test`. |
| `src/runner/engine/runtime/policy_http_request.zig` | EDIT | Add `test {}` pulling its sibling test; delete the false "Tests live in sibling" comment (line ~152). |
| `src/zombied/reliability/backoff.zig` | DELETE | Dead `expBackoffJitter` — no prod caller (worker consumer removed at M80). |
| `src/runner/engine/client_errors.zig` | EDIT | Remove the unused `ClientError` error set (~lines 5–17); KEEP the live `ERR_*` string codes. |
| `src/zombied/auth/middleware/bearer_oidc.zig` | DELETE | Superseded by `bearer_or_api_key`; zero prod instantiation. |
| `src/zombied/auth/middleware/security_headers.zig` | DELETE | Unwired HSTS skeleton; `applyHsts` zero callers; prod HSTS is load-balancer-only. |
| `src/zombied/auth/middleware/mod.zig` | EDIT | Drop `bearer_oidc`/`BearerOidc` + `security_headers` re-exports. |
| `src/zombied/auth/tests.zig` | EDIT | Drop isolation imports of the two deleted middlewares (lines 29, 31). |
| `src/zombied/auth/middleware/bearer_or_api_key.zig` | EDIT | Reword the now-stale "mirrors bearer_oidc" comment (line ~30). |
| `src/runner/engine/runtime/session_store.zig` | DELETE | Production-dead `SessionStore`; no prod path reaches it. |
| `src/runner/engine/session.zig` | DELETE | **Correction:** dead post-M80 — the runner is stateless; `Session` has no prod caller (only the deleted tests + a stale `context_budget` comment). |
| `src/runner/engine/resource_security_test.zig` | DELETE | **Correction:** the whole suite is `Session`-resource-metrics tests (dead). Live-symbol bits (`mapError`/`FailureClass`) are covered in `runner_security_test.zig`. |
| `src/runner/engine/sandbox_edge_test.zig` | DELETE | **Correction:** predominantly `Session` lifecycle/edge tests (dead). Live bits (`mapError`/`ResourceLimits`/`LeaseState`/`appendBwrapNetworkArgs`) covered in `types.zig`/`network.zig`/`runner_security_test.zig`/`sandbox_args_edge_test.zig`. |
| `src/runner/tests.zig` | EDIT | Drop the `engine/session.zig` aggregator import. |
| `src/runner/engine/context_budget.zig` | EDIT | Reword the stale `Session.create` lifetime comment. |
| `src/runner/sandbox_args_edge_test.zig` | EDIT | Drop the comment pointer to the deleted `sandbox_edge_test.zig`. |
| `src/zombied/config/contact.zig` | DELETE | Cross-tier `SUPPORT_EMAIL` pin with no Zig consumer. |
| `src/zombied/config/contact_test.zig` | DELETE | Tests only the deleted constant. |
| `src/zombied/tests.zig` | EDIT | Remove `contact`, `contact_test`, `backoff` aggregator imports (lines 19, 20, 75). |
| `src/runner/engine/wire.zig` | EDIT | Triage ~17 unused field-name constants (use or remove); fix the "single source of truth" header. |
| `src/runner/engine/json_helpers.zig` | EDIT | Fix stale "Used by handler.zig … client.zig" comment (neither exists). |
| `src/zombied/state/zombie_events_store_test.zig` | EDIT | Fix comment pointing at non-existent `tests/integration/`. |
| `src/zombied/http/route_table.zig` | EDIT | Fix "Batch E removes it" comment (already removed). |
| `src/zombied/http/handlers/common.zig` | EDIT | Reword "backward compatibility during migration" (re-exports are now the live access path). |
| `docs/AUTH.md` | EDIT | `src/auth/` → `src/zombied/auth/` (7 occurrences). |
| `docs/architecture/roadmap.md` | EDIT | Same M80 path drift fix. |

> `wire.zig` triage may touch a few runner callers (e.g. `child_exec.zig`) if a constant is wired rather than removed — agent's per-constant call; add to this table at PLAN if so. Done-specs under `docs/v2/done/` stay frozen (historical record).

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections, each a self-contained slice of the audit (restore-coverage; trivial deletes; auth deletes; runner-module delete with test de-coupling; UFS + comment/doc hygiene). One PR — the items share the "audit said dead" provenance and the same verify spine.
- **Alternatives considered:** (a) one delete-everything commit — rejected: mixes coverage-restoring edits with deletions, harder to bisect a regression; (b) defer §5 (`wire.zig` UFS) to its own spec — rejected: it is the same audit's finding and small; folding it in keeps the audit's output in one trackable place.
- **Patch-vs-refactor verdict:** **patch** — removal + re-wiring against existing patterns, no new abstraction. No larger refactor is hiding here; the binary split (M80) already did the structural work.

---

## Sections (implementation slices)

### §1 — Restore the live orphan test suite (RULE ORP) — **DONE**

> **CORRECTION (Indy, post-merge):** only `policy_http_request.zig` is a live production file — `PolicyHttpRequestTool` is instantiated at `tool_builders.zig:184` (← `tool_bridge`). `session.zig`'s `Session` is **dead** post-M80: the runner is stateless (`runner.execute` → `ExecutionResult`), nothing constructs a `Session`. So `session_test.zig` tested dead code and was **deleted**, not restored; only the policy suite (§1.2) is re-aggregated. See Discovery.

Originally framed as two genuinely-dead tests covering live files. For the live one (`policy_http_request.zig`), re-aggregate via the existing test-block pattern — a `test {}` pulling its sibling (the reachable parent, RULE ORP) — and fix any drift so it compiles and passes.

- **Dimension 1.1** — **VOIDED**: `session.zig`/`Session` is dead → `session_test.zig` deleted (it tested dead code), not restored.
- **Dimension 1.2** — `policy_http_request_test.zig` compiles and runs under the runner test build; the false coverage comment is gone → Test `policy http request suite`.

### §2 — Trivial dead-code deletes (no behavioural risk) — **DONE**

Three removals nothing reaches: `reliability/backoff.zig` (+ its `tests.zig` import), the `ClientError` error set in `client_errors.zig` (the file and its live `ERR_*` codes stay), and `config/contact.zig` + `contact_test.zig` (+ both `tests.zig` imports). **Invariant to protect:** the `ERR_*` string codes in `client_errors.zig` remain referenced by `runner.zig`/`child_exec.zig`/`tool_bridge.zig` — remove only the `error{…}` set.

- **Dimension 2.1** — `backoff.zig` gone; `expBackoffJitter` unreferenced repo-wide → Test `dead-code sweep: backoff`.
- **Dimension 2.2** — `ClientError` error set gone; `ERR_*` codes intact and both build graphs green → Test `dead-code sweep: ClientError` + build.
- **Dimension 2.3** — `contact.zig` + `contact_test.zig` gone; `SUPPORT_EMAIL` unreferenced in `src/` → Test `dead-code sweep: contact`.

### §3 — Remove superseded auth surface — **DONE**

Delete `bearer_oidc.zig` (superseded by `bearer_or_api_key`, which lifts `platform_admin` identically) and `security_headers.zig` (unwired HSTS skeleton). Drop their `mod.zig` re-exports + `auth/tests.zig` isolation imports; reword the stale "mirrors bearer_oidc" comment in `bearer_or_api_key.zig`. **Invariant to protect:** `platform_admin` gating of `POST /v1/runners` is unchanged — the existing `platform_admin.zig` middleware tests must stay green (they read the principal claim set by `bearer_or_api_key`, not `bearer_oidc`).

- **Dimension 3.1** — `bearer_oidc.zig` gone; `BearerOidc`/`bearer_oidc` unreferenced; `test-auth` portability target + `platform_admin` tests green → Test `auth suite post-bearer_oidc`.
- **Dimension 3.2** — `security_headers.zig` gone; `applyHsts`/`HSTS_*` unreferenced; no production response changes → Test `dead-code sweep: security_headers`.

### §4 — Remove runner `SessionStore`, de-couple its security suites — **DONE**

> **CORRECTION (Indy, post-merge):** the surgical de-coupling was superseded once `Session` itself was found dead (§1 correction). `session.zig` + the whole `Session` test ecosystem — `session_test.zig`, `resource_security_test.zig`, `sandbox_edge_test.zig` — were **deleted wholesale** (they exercise the dead `Session` resource-metrics/lifecycle abstraction). The live-symbol coverage they incidentally carried (`mapError`/`FailureClass`/`ResourceLimits`/`LeaseState`/`appendBwrapNetworkArgs`/`executionId`) is preserved in those symbols' own files (`runner_security_test.zig`, `types.zig`, `network.zig`, `sandbox_args_edge_test.zig`, `child_supervisor_test.zig`).

Delete `runtime/session_store.zig` (production-dead) along with the dead `Session` it managed.

- **Dimension 4.1** — `session_store.zig` gone; `SessionStore` unreferenced repo-wide → Test `dead-code sweep: session_store`.
- **Dimension 4.2** — **REVISED**: `resource_security_test` + `sandbox_edge_test` deleted (dead-`Session` suites). Runner test build green at 233 (−39 dead-`Session` tests); live coverage retained in sibling files.

### §5 — `wire.zig` UFS triage + comment/doc hygiene — **DONE**

`wire.zig` defines 34 `pub` field-name constants as a single-source-of-truth but only ~17 are referenced as `wire.X`; callers hardcode the rest. Per RULE UFS, for each unused constant either wire the caller to use it or remove it — leave none defined-but-unused — and fix the header's "single source of truth" claim to match reality. Then fix the stale comments in `json_helpers.zig`, `zombie_events_store_test.zig`, `route_table.zig`, `handlers/common.zig`, and the `src/auth/`→`src/zombied/auth/` paths in `docs/AUTH.md` + `docs/architecture/roadmap.md`.

- **Dimension 5.1** — every `pub const` in `wire.zig` is either referenced as `wire.X` or removed; zlint `unused-decls` clean; header accurate → Test `wire.zig has no unused single-source constant`.
- **Dimension 5.2** — named stale comments + doc paths corrected; greps for the stale phrases/paths return zero in live docs/source → Test `stale-reference sweep`.

---

## Interfaces

No public HTTP, CLI, or cross-module *contract* changes. Removed internal symbols (`BearerOidc`, `ClientError` error set, `applyHsts`/`HSTS_*`, `SessionStore`, `SUPPORT_EMAIL`, unused `wire.*` constants) had zero consumers — confirmed by the dead-code sweep. The frozen `src/lib/contract` wire protocol is **untouched**.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Hidden consumer of a deleted symbol | Audit missed a reach path (e.g. comptime, named module) | Build fails on one of the two graphs → agent restores the symbol and re-investigates; never `--no-verify`. |
| `ERR_*` codes deleted with the error set | Over-broad edit in `client_errors.zig` | Build break (codes are used) → revert to set-only removal. |
| `platform_admin` gate regressed | `bearer_oidc` deletion assumed wrongly | `platform_admin.zig` middleware test fails → STOP; confirm `bearer_or_api_key` claim-lift before proceeding. |
| Security suite emptied, not de-coupled | §4 removes whole files instead of the SessionStore cases | Runner suite loses coverage → review diff: only the 3 SessionStore cases + 2 imports may go. |
| `wire.zig` constant removed but literal still drifts | A caller hardcodes the literal the removed constant pinned | UFS triage chose "remove" where "wire" was right → re-evaluate per the constant's caller. |

---

## Invariants

1. **Both build graphs compile after every section** — `zig build` AND `zig build --build-file build_runner.zig` — enforced by CI/make (a hidden consumer breaks the build).
2. **zlint `unused-decls: error` stays clean** — enforced by `make lint`; catches any symbol left dead by a partial removal.
3. **`client_errors.zig` `ERR_*` codes remain referenced** — enforced by the runner build (they're consumed) + the dead-code sweep.
4. **`platform_admin` gating preserved** — enforced by the existing `platform_admin.zig` middleware tests (independent of `bearer_oidc`).
5. **No production response/behaviour change** — enforced by the unchanged integration suites passing (only test-build composition changes).
6. **No new dead code (RULE NDC) / no legacy framing (RULE NLG)** — enforced by zlint + the stale-reference sweep.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete → expected) |
|-----------|------|------|-------------------------------|
| 1.1 | unit | `session lifecycle suite` | `session_test.zig`'s cases compile against current `session.zig` and pass under the runner test build. |
| 1.2 | unit | `policy http request suite` | `policy_http_request_test.zig`'s cases compile and pass; no "tests live in sibling" comment remains. |
| 2.1 | regression | `dead-code sweep: backoff` | `grep -rn expBackoffJitter src/` → 0; `make test` still green. |
| 2.2 | regression | `dead-code sweep: ClientError` | `grep -n "ClientError" src/runner/engine/client_errors.zig` → 0; `ERR_*` present; both graphs build. |
| 2.3 | regression | `dead-code sweep: contact` | `grep -rn SUPPORT_EMAIL src/` → 0; `make test` green. |
| 3.1 | integration | `auth suite post-bearer_oidc` | `grep -rn BearerOidc src/` → 0; `make test-auth` + `platform_admin` tests pass. |
| 3.2 | regression | `dead-code sweep: security_headers` | `grep -rn "applyHsts\|HSTS_HEADER" src/` → 0; integration suite unchanged. |
| 4.1 | regression | `dead-code sweep: session_store` | `grep -rn SessionStore src/` → 0. |
| 4.2 | unit | `runner security suites survive` | `resource_security_test` + `sandbox_edge_test` compile and their remaining cases pass under the runner test build. |
| 5.1 | unit | `wire.zig has no unused single-source constant` | every `pub const` in `wire.zig` is referenced as `wire.X` or absent; zlint clean. |
| 5.2 | regression | `stale-reference sweep` | greps for the named stale phrases + `src/auth/` in live docs/source → 0. |

**Regression:** the full `make test` + runner suite is the regression net — pre-existing behaviour must not change. **Idempotency/replay:** N/A (no retry semantics touched).

---

## Acceptance Criteria

- [x] `policy_http_request_test` runs & passes (live — `tool_builders.zig:184`) — runner `229/233` (4 skips). `session_test` was deleted, not restored: `session.zig`/`Session` is dead (see Discovery).
- [x] zombied unit suite green — `zig build test` → `1189/1470` (281 integration skips). *(repo target is `zig build test`, not `make test`.)*
- [x] Both graphs build; cross-compile clean — `zig build` + `zig build --build-file build_runner.zig` + both graphs × `x86_64-linux` + `aarch64-linux` all exit 0.
- [x] `test-auth` portability gate green — `zig build test-auth` → `227/227` (platform_admin invariant holds).
- [x] Lint clean (zlint unused-decls) — `make lint-zig` → ZLint `0 errors / 0 warnings` across 387 files.
- [x] Dead-code sweep all-zero — removed symbols `0` in their graphs; remaining hits are the documented live collisions (`daemon ClientError`, `session_store_redis SessionStore`), correctly retained.
- [x] No `src/auth/` path left in live docs — `grep -rn "src/auth/" docs/AUTH.md docs/architecture/roadmap.md` → `0`.
- [x] `gitleaks detect` clean (2344 commits) · no file over 350 lines (FLL gate green).

---

## Eval Commands (post-implementation)

```bash
# E1: Orphan tests now run (expect both suites in output)
zig build --build-file build_runner.zig test 2>&1 | tail -8 && echo PASS || echo FAIL
# E2: Build both graphs
zig build && zig build --build-file build_runner.zig && echo PASS || echo FAIL
# E3: zombied tests + auth portability
make test && zig build test-auth && echo PASS || echo FAIL
# E4: Lint (zlint unused-decls catches partial removals)
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: Dead-code sweep (empty = pass)
grep -rn "expBackoffJitter\|BearerOidc\|applyHsts\|SessionStore\|SUPPORT_EMAIL\|ClientError" src/ | grep -v "ERR_" | head
# E8: Stale-reference sweep (empty = pass)
grep -rn "src/auth/" docs/AUTH.md docs/architecture/roadmap.md
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/zombied/reliability/backoff.zig` | `test ! -f src/zombied/reliability/backoff.zig` |
| `src/zombied/auth/middleware/bearer_oidc.zig` | `test ! -f src/zombied/auth/middleware/bearer_oidc.zig` |
| `src/zombied/auth/middleware/security_headers.zig` | `test ! -f src/zombied/auth/middleware/security_headers.zig` |
| `src/runner/engine/runtime/session_store.zig` | `test ! -f src/runner/engine/runtime/session_store.zig` |
| `src/zombied/config/contact.zig` | `test ! -f src/zombied/config/contact.zig` |
| `src/zombied/config/contact_test.zig` | `test ! -f src/zombied/config/contact_test.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `expBackoffJitter` | `grep -rn expBackoffJitter src/` | 0 |
| `BearerOidc` / `bearer_oidc` | `grep -rn "bearer_oidc\|BearerOidc" src/` | 0 |
| `applyHsts` / `HSTS_HEADER` | `grep -rn "applyHsts\|HSTS_HEADER" src/` | 0 |
| `SessionStore` (runtime) | `grep -rn SessionStore src/runner` | 0 |
| `SUPPORT_EMAIL` | `grep -rn SUPPORT_EMAIL src/` | 0 |
| `ClientError` (engine) | `grep -n ClientError src/runner/engine/client_errors.zig` | 0 (ERR_* remain) |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **Scope consult (Jun 03 2026)** — Indy adjudicated the keep-vs-delete calls: delete `bearer_oidc`, `security_headers` (skeleton, not wire), `session_store` (+ de-couple tests), `contact`; wire-in the two orphan tests (live equivalents exist). Two deletion-safety gates verified green before authoring (platform_admin covered by `bearer_or_api_key`; HSTS is load-balancer-only).
- **Runner-CLI token scope decision (Jun 03 2026)** — Mid-work, Indy questioned the `zombie-runner register` token model (`--token`/`ZOMBIE_TOKEN`). Investigated M80_004 (DONE), the runner bootstrap playbooks, and `docs/architecture/runner_fleet.md`: `register` deliberately authenticates with the operator's platform-admin Clerk JWT via `ZOMBIE_TOKEN`/`--token` (verbatim `zombiectl` precedence); under Option B the host holds only `ZOMBIE_RUNNER_TOKEN` (the operator-minted `zrn_`), never an admin credential. **Decision: leave the runner CLI as-is** — the shared `ZOMBIE_TOKEN` is the agreed, integration-tested M80_004 design; a rename would supersede M80_004 and belongs in its own spec, NOT this dead-code cleanup. M83 stays dead-code-only. Recorded in agent memory `project_runner_register_admin_token_intentional`.
- **§4 SessionStore test-case count (Jun 03 2026)** — Adversarial re-confirmation found `resource_security_test.zig` references `SessionStore` in **three** cases (T8 "destroying one session in store" @76, T5+T8 concurrency @266, T11 no-leaks @411), not the two the §4 prose names ("no-leaks + concurrency"). All three must go for the import removal to compile (Invariant: both suites still compile); the neighbour-isolation intent survives via the bare-`Session` T8 @36. `sandbox_edge_test.zig` is as specified (one case, T3 reapExpired @268).
- **`session.zig`/`Session` is DEAD — §1/§4 corrected (Indy, post-merge, Jun 03 2026)** — After the PR opened, Indy asked whether the *subjects* of the restored tests are used in production. Tracing `runner.execute` (`runner.zig:64`) showed it is **stateless** (params → `ExecutionResult`); `session.zig`'s `Session` has **zero production caller** — every reference is a test fixture or the test aggregator, plus one stale `context_budget.zig:22` comment. The persistent session/checkpoint state lives **zombied-side** in `core.zombie_sessions` (`008_core_zombie_sessions.sql`, managed by `fleet/zombie_session.zig`'s `ZombieSession`), which the runner never touches (Option B, no datastore creds). So `Session` is pre-M80-worker leftover — dead like the `SessionStore` that managed it. **Audit blind spot:** the original audit caught `SessionStore` but treated `session.zig` as "live", so §1 *restored* `session_test.zig` for dead code. **Corrected:** deleted `session.zig` + `session_test.zig` + `resource_security_test.zig` + `sandbox_edge_test.zig` (the whole dead-`Session` test ecosystem, −39 tests); live-symbol coverage they incidentally carried (`mapError`/`FailureClass`/`ResourceLimits`/`LeaseState`/`appendBwrapNetworkArgs`/`executionId`) is retained in those symbols' own files. The `policy_http_request_test.zig` restore **stands** (live: `tool_builders.zig:184`). My §1 restore, `/write-unit-test`, and `/review` all validated tests against `session.zig` as if live and missed it; **Indy's question caught it** — a reminder that automated review checks "do the tests pass / do deletions drop a live consumer", not "is the code-under-test itself live."
- **Skill chain outcomes** — `/write-unit-test`: ledger 100% resolved. `/review`: ship-as-is — all 5 priority areas clean @9/10; one finding (pre-existing `///` mis-attachment in `bearer_or_api_key.zig` that §3 had touched) fixed under RULE NLR (commit 7243894e). `kishore-babysit-prs`: greptile reviewed clean (check SUCCESS, 0 findings). **Note:** the review chain validated `session.zig` as live — the dead-`Session` correction above came from Indy's adversarial question, not the skills.
- **Deferrals** — none.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification — esp. that §1's restored suites and §4's surviving cases are real, not happy-path. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, ZIG_RULES, Failure Modes, Invariants — esp. "did a deletion drop a live consumer?" | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY (Jun 03 2026, commit 7243894e).

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Orphan tests run | `zig build --build-file build_runner.zig test` | 229/233 pass, 4 skip; the live policy suite executes; dead-`Session` test ecosystem removed (−39) | ✅ |
| Unit tests | `zig build test` (repo target, not `make test`) | 1189/1470 pass, 281 integration skips | ✅ |
| Auth portability | `zig build test-auth` | 227/227 pass (platform_admin invariant) | ✅ |
| Lint (unused-decls) | `make lint-zig` | ZLint 0 errors / 0 warnings across 387 files; fmt + pg-drain + FLL + ORP guards green | ✅ |
| Cross-compile | both graphs × `-Dtarget=x86_64-linux` + `aarch64-linux` | all exit 0 | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (2344 commits, 138 MB) | ✅ |
| Dead-code sweep | `rg` per removed symbol in its graph | 0 hits; live `daemon ClientError` + `session_store_redis SessionStore` correctly retained | ✅ |
| Harness verify | pre-commit `make harness-verify` | ALL GATES GREEN (UFS, DESIGN TOKEN, SPEC TEMPLATE, ERROR REGISTRY, LOGGING, LIFECYCLE, CROSS-TIER RATES, MS-ID+UI) | ✅ |
| Skill chain | `/write-unit-test` · `/review` | write-unit-test: coverage adequate (ledger 100%); /review: ship-as-is, all 5 areas clean @9/10, one NLR doc-comment fix applied (7243894e) | ✅ |

---

## Out of Scope

- Done-specs under `docs/v2/done/` that mention the deleted surfaces (`M18_002`, `M74_002`, `M80_005`) — historical record, frozen by changelog discipline; not edited.
- Any *new* HSTS implementation — Indy chose to drop the skeleton; load-balancer HSTS is the posture. Wiring app-level HSTS, if ever wanted, is a future security spec.
- Symbol-level dead-export hunting beyond the named findings — zlint `unused-decls` owns ongoing within-module enforcement.
