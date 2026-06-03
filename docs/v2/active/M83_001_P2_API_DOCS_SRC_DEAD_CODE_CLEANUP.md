# M83_001: Remove verified-dead src/ files & exports; re-aggregate two orphan tests

**Prototype:** v2.0.0
**Milestone:** M83
**Workstream:** 001
**Date:** Jun 03, 2026
**Status:** IN_PROGRESS
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

- **PR title (eventual):** Remove verified-dead src/ code; re-aggregate two orphan tests
- **Intent (one sentence):** Strip pre-2.0 dead code an exhaustive reachability audit proved unreachable, restore two real test suites that silently never ran, and fix the stale comments/doc-paths that masked both — leaving `src/` honest with zero behaviour change.
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

**Goal (testable):** After this PR, `zig build` + `zig build --build-file build_runner.zig` + cross-compile both linux targets pass, `make test` + runner tests are green, the dead-code sweep greps all return zero, and `session_test`/`policy_http_request_test` now compile and run — with no production response or behaviour changed.

**Problem:** A folder-by-folder reachability audit (387 `.zig` files, two build graphs) found a small set of files/exports nothing reaches, plus two real test suites that never compile because no aggregator pulls them in, plus stale comments and doc-paths (`src/auth/` post-M80) that actively mislead. Pre-2.0 RULE NDC/NLG say this cruft should not linger.

**Solution summary:** Delete the verified-dead files and exports; re-aggregate the two orphan tests via the existing test-block pattern (RULE ORP); triage `wire.zig`'s unused single-source constants (use or remove); and fix the stale comments/doc-paths the audit surfaced. Pure hygiene — the only *behavioural* delta is that two previously-dead test suites now run.

---

## Prior-Art / Reference Implementations

- **Orphan re-aggregation** → `src/runner/engine/runner.zig`'s `test { _ = @import("…_test.zig"); }` block (the RULE ORP precedent that re-homed the engine suites after the M80 cutover deleted `runner_test.zig`). §1 mirrors it.
- **Auth deletion safety** → `docs/AUTH.md` + `src/zombied/auth/middleware/bearer_or_api_key.zig` (the live JWT path that subsumes `bearer_oidc`).
- No new abstractions; this is removal + re-wiring against existing patterns.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/session_test.zig` | EDIT | Fix drift vs current `session.zig`; keep (live equivalent). |
| `src/runner/engine/runtime/policy_http_request_test.zig` | EDIT | Fix drift vs current `policy_http_request.zig`; keep (live equivalent). |
| `src/runner/engine/runner.zig` | EDIT | Add `session_test.zig` to the engine test-block aggregator. |
| `src/runner/engine/runtime/policy_http_request.zig` | EDIT | Add `test {}` pulling its sibling test; delete the false "Tests live in sibling" comment (line ~152). |
| `src/zombied/reliability/backoff.zig` | DELETE | Dead `expBackoffJitter` — no prod caller (worker consumer removed at M80). |
| `src/runner/engine/client_errors.zig` | EDIT | Remove the unused `ClientError` error set (~lines 5–17); KEEP the live `ERR_*` string codes. |
| `src/zombied/auth/middleware/bearer_oidc.zig` | DELETE | Superseded by `bearer_or_api_key`; zero prod instantiation. |
| `src/zombied/auth/middleware/security_headers.zig` | DELETE | Unwired HSTS skeleton; `applyHsts` zero callers; prod HSTS is load-balancer-only. |
| `src/zombied/auth/middleware/mod.zig` | EDIT | Drop `bearer_oidc`/`BearerOidc` + `security_headers` re-exports. |
| `src/zombied/auth/tests.zig` | EDIT | Drop isolation imports of the two deleted middlewares (lines 29, 31). |
| `src/zombied/auth/middleware/bearer_or_api_key.zig` | EDIT | Reword the now-stale "mirrors bearer_oidc" comment (line ~30). |
| `src/runner/engine/runtime/session_store.zig` | DELETE | Production-dead `SessionStore`; no prod path reaches it. |
| `src/runner/engine/resource_security_test.zig` | EDIT | Drop the `SessionStore` import + its test cases (T11 + concurrency); KEEP the rest of the suite. |
| `src/runner/engine/sandbox_edge_test.zig` | EDIT | Drop the `SessionStore` import + the `reapExpired` case (T3); KEEP the rest of the suite. |
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

### §1 — Restore the two orphan test suites (RULE ORP)

The only two genuinely-dead tests. Both cover live production files (`session.zig`, `policy_http_request.zig`) and were written May 27 but never compiled against current sources. Re-aggregate via the existing test-block pattern, then fix any drift so they compile and pass. **Implementation default:** wire `session_test.zig` into `runner.zig`'s engine test block; add a `test {}` to `policy_http_request.zig` pulling its sibling — because that is the reachable parent (RULE ORP), not a new aggregator.

- **Dimension 1.1** — `session_test.zig` compiles and runs under the runner test build → Test `session lifecycle suite`.
- **Dimension 1.2** — `policy_http_request_test.zig` compiles and runs under the runner test build; the false coverage comment is gone → Test `policy http request suite`.

### §2 — Trivial dead-code deletes (no behavioural risk)

Three removals nothing reaches: `reliability/backoff.zig` (+ its `tests.zig` import), the `ClientError` error set in `client_errors.zig` (the file and its live `ERR_*` codes stay), and `config/contact.zig` + `contact_test.zig` (+ both `tests.zig` imports). **Invariant to protect:** the `ERR_*` string codes in `client_errors.zig` remain referenced by `runner.zig`/`child_exec.zig`/`tool_bridge.zig` — remove only the `error{…}` set.

- **Dimension 2.1** — `backoff.zig` gone; `expBackoffJitter` unreferenced repo-wide → Test `dead-code sweep: backoff`.
- **Dimension 2.2** — `ClientError` error set gone; `ERR_*` codes intact and both build graphs green → Test `dead-code sweep: ClientError` + build.
- **Dimension 2.3** — `contact.zig` + `contact_test.zig` gone; `SUPPORT_EMAIL` unreferenced in `src/` → Test `dead-code sweep: contact`.

### §3 — Remove superseded auth surface

Delete `bearer_oidc.zig` (superseded by `bearer_or_api_key`, which lifts `platform_admin` identically) and `security_headers.zig` (unwired HSTS skeleton). Drop their `mod.zig` re-exports + `auth/tests.zig` isolation imports; reword the stale "mirrors bearer_oidc" comment in `bearer_or_api_key.zig`. **Invariant to protect:** `platform_admin` gating of `POST /v1/runners` is unchanged — the existing `platform_admin.zig` middleware tests must stay green (they read the principal claim set by `bearer_or_api_key`, not `bearer_oidc`).

- **Dimension 3.1** — `bearer_oidc.zig` gone; `BearerOidc`/`bearer_oidc` unreferenced; `test-auth` portability target + `platform_admin` tests green → Test `auth suite post-bearer_oidc`.
- **Dimension 3.2** — `security_headers.zig` gone; `applyHsts`/`HSTS_*` unreferenced; no production response changes → Test `dead-code sweep: security_headers`.

### §4 — Remove runner `SessionStore`, de-couple its security suites

Delete `runtime/session_store.zig` (production-dead). Its only users are two *running* security suites that exercise it as a fixture — surgically remove the `SessionStore` import + the cases that test it (`resource_security_test`: the no-leaks + concurrency cases; `sandbox_edge_test`: the `reapExpired` case), and KEEP every other case in both suites plus `runner.zig:306-307` (which aggregate the surviving suites). **Invariant to protect:** both suites still compile and their non-SessionStore assertions still run.

- **Dimension 4.1** — `session_store.zig` gone; `SessionStore` unreferenced repo-wide → Test `dead-code sweep: session_store`.
- **Dimension 4.2** — `resource_security_test` + `sandbox_edge_test` compile and pass with their remaining cases under the runner test build → Test `runner security suites survive`.

### §5 — `wire.zig` UFS triage + comment/doc hygiene

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

- [ ] `session_test` + `policy_http_request_test` run & pass — verify: `zig build --build-file build_runner.zig test 2>&1 | tail -5`
- [ ] zombied unit suite green — verify: `make test`
- [ ] Both graphs build; cross-compile clean — verify: `zig build && zig build --build-file build_runner.zig && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `test-auth` portability gate green — verify: `zig build test-auth`
- [ ] Lint clean (zlint unused-decls) — verify: `make lint`
- [ ] Dead-code sweep all-zero — verify: `grep -rn "expBackoffJitter\|BearerOidc\|applyHsts\|SessionStore\|SUPPORT_EMAIL" src/ | head`
- [ ] No `src/auth/` path left in live docs — verify: `grep -rn "src/auth/" docs/AUTH.md docs/architecture/roadmap.md`
- [ ] `gitleaks detect` clean · no file over 350 lines added

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
- **Skill chain outcomes** — populate during VERIFY/CHORE(close).
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

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Orphan tests run | `zig build --build-file build_runner.zig test` | {paste} | |
| Unit tests | `make test` | {paste} | |
| Auth portability | `zig build test-auth` | {paste} | |
| Lint (unused-decls) | `make lint` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| Dead-code sweep | `grep -rn "BearerOidc\|applyHsts\|SessionStore\|SUPPORT_EMAIL\|expBackoffJitter" src/` | {paste} | |

---

## Out of Scope

- Done-specs under `docs/v2/done/` that mention the deleted surfaces (`M18_002`, `M74_002`, `M80_005`) — historical record, frozen by changelog discipline; not edited.
- Any *new* HSTS implementation — Indy chose to drop the skeleton; load-balancer HSTS is the posture. Wiring app-level HSTS, if ever wanted, is a future security spec.
- Symbol-level dead-export hunting beyond the named findings — zlint `unused-decls` owns ongoing within-module enforcement.
