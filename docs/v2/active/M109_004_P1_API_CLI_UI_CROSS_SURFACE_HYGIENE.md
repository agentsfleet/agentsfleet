<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M109_004: Deduplicate route method-checks, fix a permanently-wedged PostHog loader, and gate the operator UI on scope not the `platform_admin` boolean

**Prototype:** v2.0.0
**Milestone:** M109
**Workstream:** 004
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Test Baseline:** unit=2270 integration=243
**Priority:** P1 — the PostHog loader permanently stops delivering analytics after one transient failure for the rest of a page session (silent product-signal loss); the operator dashboard (runners, admin models) gates on a legacy `platform_admin` boolean the M104 scope migration left behind, so a correctly-scoped operator cannot reach the surface and platform-admin must be configured twice (boolean + scopes), which drift; the route-method duplication is a maintainability/DRY issue at P2-grade severity kept in this workstream only because it shares no scope conflict with the others.
**Categories:** API UI
**Batch:** B1 — independent of M109_001/002/003; no shared files.
**Branch:** feat/m109-004-cross-surface-hygiene
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, fleet-wide-refactor-audit `Workflow` run `wf_8ec169f4-8e4`, each finding independently re-verified against current source before this spec was drafted, Jul 02, 2026).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

> **Scope amendment (Jul 02, 2026):** the original §3 — restore the collapsed `ROLE_NAMESPACE_DEV`/`ROLE_NAMESPACE_COM` split in `cli/src/program/auth-token.ts` — was **dropped** before implementation. The static-role concept the namespaced claim fed has been removed from the product; restoring the dev/prod namespace distinction would reinstate plumbing for a claim nothing reads. Ack: `> Indy (2026-07-02): "I dont think you need 3 - since role is removed, if you add where will you use this ... since the static roles are nuked now" — context: §3 ROLE_NAMESPACE_DEV, dropped as vestigial.` Section numbering (§1, §2, §4) is preserved to keep continuity with the handoff and the operator-UI reconciliation discussion; there is no §3 in this branch.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/common.zig:285-303` — `requireUuidV7Id` (285-296) is the exact naming/shape convention `requireMethod` must follow (`require<X>(res, ...) bool`, side-effects a response on failure); `respondMethodNotAllowed` (300-303) is the primitive it wraps.
2. `ui/packages/app/lib/analytics/posthog.ts:142-152` — the sibling app-package PostHog loader already handles a failed dynamic import correctly (`try/catch` around `await import("posthog-js")`, flips a flag rather than leaving a permanently-rejected promise); this is the exact pattern §2 ports into the website package.
3. `ui/packages/app/lib/auth/platform.ts` + `src/agentsfleetd/http/route_scopes.zig:120-127` — the frontend `platform_admin` boolean gate to reconcile (§4) and the per-route scopes it must mirror (`runner:enroll` enroll, `runner:read` list, `model:read`/`model:admin` models); the session token now carries a top-level `scopes` claim the gate reads instead.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Add requireMethod helper, fix PostHog's permanently-wedged loader, gate operator UI on scope not the platform_admin boolean
- **Intent (one sentence):** A malformed route no longer needs a hand-copied 4-line check at 63 call sites, a transient PostHog load failure no longer kills analytics for the rest of the page session, and the operator UI is gated on the same scopes the backend enforces instead of a divergent `platform_admin` boolean.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a new route handler is added and its author calls `common.requireMethod(...)` instead of copy-pasting the 4-line block; a visitor whose first PostHog load fails (ad-blocker hiccup, offline blip) still gets analytics once connectivity returns, instead of the tab going permanently dark; a correctly-scoped operator (holds `runner:enroll`) reaches the runners UI instead of being hidden behind a `platform_admin` boolean nobody set.
2. **Preserved user behaviour** — every route's actual method-allow/deny behavior is byte-for-byte unchanged (§1 is a pure refactor); PostHog's happy-path load is unchanged (§2 only changes the failure branch); the backend `requireScope` gate is untouched and remains authoritative (§4 only changes the UI's defence-in-depth check).
3. **Optimal-way check** — §1's optimal fix is exactly a shared boolean-returning helper matching the file's own existing convention; no larger routing-layer refactor is warranted for a DRY issue. §2's optimal fix is equally narrow — a `.catch`/reset. §4's is a scope-predicate swap at the existing gate call sites, no auth-model redesign.
4. **Rebuild-vs-iterate** — iterate on all three; each has a working sibling/reference (the app-package PostHog loader, the `requireUuidV7Id` convention, the backend `route_scopes.zig` scope map, the `settings/api-keys` action shape) to converge onto rather than a redesign.
5. **What we build** — `common.requireMethod` (single-method call sites migrate to it; multi-method `switch` sites are evaluated separately per §1's Discovery note); a `.catch`/retry-reset on `ensureLoader`'s `loadPromise` assignment in the website package; a scope-based operator-UI gate replacing `readPlatformAdminClaim`/`platform_admin` with per-surface session-`scopes` checks.
6. **What we do NOT build** — a `requireMethodOneOf` for the 10 multi-method `switch` sites (Discovery records why single- vs multi-method sites are treated differently, but building the multi-method helper is flagged as a follow-up, not blocking); a rewrite of PostHog's retry/backoff policy beyond "don't stay permanently wedged"; any change to the backend `requireScope` authority.
7. **Fit with existing features** — §1 must not change any route's actual method-allow set (regression risk is purely mechanical — a copy-paste-to-call-site error); §4 must not become the sole authority — the backend `requireScope` stays the real gate and the UI check is defence-in-depth only.
8. **Surface order** — API-first for §1 (no CLI/UI surface); UI-first for §2 (website analytics loader) and §4 (operator dashboard).
9. **Dashboard restraint** — §4 adds no new UI controls; it changes which claim gates the existing operator surfaces. §2 is an internal loader fix with no visible UI change.
10. **Confused-user next step** — §1's confused-user case is a future route author who doesn't know the helper exists — the PR itself (and this spec's Prior-Art pointer) is the self-serve doc; §2 has no direct end-user-facing confusion; §4's confused user is the operator who couldn't see the surface — the fix is exactly what unblocks them.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NLR (touch-it-fix-it — once `requireMethod` exists, migrate every existing call site in the 9-file family, not just new code going forward); RULE NDC (no dead code — `readPlatformAdminClaim`/`platform_admin` removed once no caller remains, §4).
- **`dispatch/write_zig.md`** — §1 touches `*.zig` across 9 files; file/function length caps (`route_table_invoke.zig` stays under 350 lines after the 27-site migration — verify, don't assume).
- **`dispatch/write_ts_adhere_bun.md`** — §2/§4 touch `*.ts`/`*.tsx`; const/import discipline, UI substitution / design-token gates (§4 changes gate logic, not visible UI — verify no raw-HTML/arbitrary-utility surface introduced).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §1 | cross-compile both linux targets after the `requireMethod` addition + single-method call-site migrations across the 9-file family. |
| PUB / Struct-Shape | yes — §1 | `requireMethod(res, method, expected) bool` matches `requireUuidV7Id`'s established shape exactly — verdict: no new struct, a plain function alongside its sibling in `common.zig`. |
| File & Function Length (≤350/≤50/≤70) | yes — §1 | `route_table_invoke.zig` is the largest file in the family (348 lines pre-migration per the research pass); each single-method site collapsing from 4 lines to 1 line should net-shrink it, but verify post-migration, don't assume. |
| UFS | no | no new repeated/semantic literal introduced; §1 removes duplication. |
| UI Substitution / DESIGN TOKEN | no | §2 is a loader-internals fix; §4 changes gate predicates, not visible markup/design-token surface — verify no raw-HTML/arbitrary-utility introduced. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | none of these findings touch logging, lifecycle, error-registry, or schema surfaces. |

---

## Overview

**Goal (testable):** a route handler needing a single-method check calls `common.requireMethod(res, req.method, .POST)` instead of a 4-line hand-copied block, at all 63 existing single-method sites across the 9-file `route_table_invoke*` family; a PostHog load failure in the website package resets `loadPromise` so a later `ensureLoader`/`track()` call retries instead of being permanently wedged; the operator UI (runners, admin models) gates on the session `scopes` claim matching the backend's per-route scope map, and the `platform_admin` boolean has zero remaining references.

**Problem:** 63 occurrences of an identical 4-line method-check pattern across 9 files, with no shared helper, means every future route risks a copy-paste drift. `ensureLoader`'s `loadPromise = loadPosthog(cfg);` has no `.catch`; once `loadPosthog` rejects (a blocked/offline chunk load), `loadPromise` stays set to the rejected promise forever, so every subsequent page interaction on that session silently stops delivering analytics — with an unhandled-rejection console error as the only symptom. The operator dashboard gates the runners and admin-models surfaces on a boolean `metadata.platform_admin === true` — a pre-M104 claim the scope migration never retired — while the backend gates the same routes on scopes; a correctly-scoped operator therefore still can't see the UI, and platform-admin has to be configured twice (boolean + scopes), which drifts.

**Solution summary:** Add `common.requireMethod` mirroring the file's own `requireUuidV7Id` convention and migrate the 63 single-method call sites (multi-method `switch` sites stay as-is per Discovery). Port the app package's `try/catch`-around-dynamic-import pattern into the website package's `ensureLoader`/`loadPosthog`, resetting `loadPromise` on failure so retry is possible. Replace the frontend `platform_admin` boolean gate with per-surface session-`scopes` predicates keyed to the scope each operator route enforces in `route_scopes.zig`, removing the divergent second source of truth.

---

## Prior-Art / Reference Implementations

- **API (§1)** → `src/agentsfleetd/http/handlers/common.zig:285-296`, `requireUuidV7Id`. **Alignment:** exact naming/shape convention (`require<X>`, side-effect + bool return), same file. **Divergence:** none for single-method sites; the 10 multi-method `switch` sites are explicitly not forced through this helper (see Decomposition).
- **UI (§2)** → `ui/packages/app/lib/analytics/posthog.ts:142-152`, `initAnalytics`'s `try/catch` around `await import("posthog-js")`. **Alignment:** same failure-isolation shape, ported from the sibling app package to the website package. **Divergence:** website's `ensureLoader` is a fire-and-forget sync function wrapping an async `loadPosthog`, not an async function itself — the `.catch` attaches to the promise assignment rather than wrapping an `await`, but the effect (isolate the failure, don't leave state permanently poisoned) is identical.
- **UI (§4)** → `ui/packages/app/app/settings/api-keys/actions.ts` + `ui/packages/app/lib/actions/with-token.ts` for the action/claim-read shape; `src/agentsfleetd/http/route_scopes.zig:120-127` for the authoritative per-route scope map. **Alignment:** the UI predicate mirrors the backend scope exactly, not a free choice. **Divergence:** the UI check is defence-in-depth; the backend `requireScope` stays the real gate.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/common.zig` | EDIT | add `requireMethod(res, method, expected) bool` alongside `requireUuidV7Id`. |
| `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | migrate its 17 single-method sites to `requireMethod`; 10 multi-method `switch` sites unchanged. |
| `src/agentsfleetd/http/route_table_invoke_runner.zig` | EDIT | migrate single-method sites (of 15 `respondMethodNotAllowed` occurrences). |
| `src/agentsfleetd/http/route_table_invoke_webhooks.zig` | EDIT | migrate single-method sites (of 6). |
| `src/agentsfleetd/http/route_table_invoke_connectors.zig` | EDIT | migrate single-method sites (of 4). |
| `src/agentsfleetd/http/route_table_invoke_events.zig` | EDIT | migrate single-method sites (of 3). |
| `src/agentsfleetd/http/route_table_invoke_approvals.zig` | EDIT | migrate single-method sites (of 3). |
| `src/agentsfleetd/http/route_table_invoke_templates.zig` | EDIT | migrate single-method sites (of 2). |
| `src/agentsfleetd/http/route_table_invoke_api_keys.zig` | EDIT | migrate single-method sites (of 2). |
| `src/agentsfleetd/http/route_table_invoke_fleet_bundles.zig` | EDIT | migrate its 1 single-method site. |
| `ui/packages/website/src/analytics/posthog.ts` | EDIT | `ensureLoader`'s `loadPromise` assignment gains failure handling that resets state for retry. |
| `ui/packages/app/lib/auth/platform.ts` | EDIT | replace the `platform_admin` boolean read with a session-`scopes` reader (`readSessionScopes`/`hasScope`); retire `readPlatformAdminClaim`. |
| `ui/packages/app/lib/auth/scopes.ts` | CREATE | client-safe scope-string constants (`SCOPE.*`) mirroring `route_scopes.zig`; importable from the client `Shell` (which can't import `platform.ts`'s server `auth`). |
| `ui/packages/app/lib/errors.ts` | EDIT | retire the dead `UZ-AUTH-021` code (`PLATFORM_ADMIN_REQUIRED`, absent from `src/` now) for `UZ-AUTH-022` (`INSUFFICIENT_SCOPE`) + operator-friendly copy. |
| `ui/packages/app/app/(dashboard)/layout.tsx` | EDIT | resolve the session scope set and pass it to `Shell` for per-surface nav gating. |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | nav prop `isPlatformAdmin` → `operatorScopes`; each platform nav item shows iff the session holds its read scope. |
| `ui/packages/app/app/(dashboard)/admin/runners/{page.tsx,actions.ts}` | EDIT | gate page on `runner:read`; actions on `runner:read`/`runner:enroll`/`runner:write`; fix the stale `UZ-AUTH-021` comment → `UZ-AUTH-022`. |
| `ui/packages/app/app/(dashboard)/admin/models/{page.tsx,actions.ts}` | EDIT | gate page/list on `model:read`; mutations + platform-default on `model:admin`. |
| `ui/packages/app/lib/auth/platform.test.ts` | EDIT | scope-reader coverage + the Invariant-3 production source scan (the one place the retired identifiers are named, to enforce their absence). |
| `ui/packages/app/tests/{runners-actions,admin-models-actions,runners-page,admin-models-page,runners-create-dialog,app-components}.test.ts` | EDIT | migrate the gate mocks from `readPlatformAdminClaim` to `hasScope`/`operatorScopes`; `UZ-AUTH-021` fixtures → `UZ-AUTH-022`. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three independent, small patches sharing a workstream; grouped because none conflicts with M109_001/002/003's files and each is a self-contained hygiene fix.
- **Alternatives considered:** for §1, forcing all 10 multi-method `switch` sites through a `requireMethodOneOf` helper was considered and rejected for this patch — the switch shape already reads clearly and collapsing it risks losing per-method routing clarity for a marginal DRY gain; flagged as an optional follow-up in Out of Scope, not built here.
- **§4 verdict:** **patch** — a scope-predicate swap at the existing gate call sites, no auth-model redesign; the backend `requireScope` gate is unchanged and remains authoritative (the UI gate is defence-in-depth). Removing the `platform_admin` boolean entirely (vs leaving it as a legacy alias) is chosen because a dual source of truth is exactly the drift that caused this.
- **Patch-vs-refactor verdict:** **patch**, all three.

---

## Sections (implementation slices)

### §1 — `requireMethod` helper, single-method call sites migrated

63 occurrences of an identical 4-line pattern across 9 files, no shared helper. **Implementation default:** add `requireMethod` matching `requireUuidV7Id`'s exact shape; migrate all 27 single-method sites in `route_table_invoke.zig` plus the remaining single-method sites in the other 8 files (the 10 multi-method `switch` sites in `route_table_invoke.zig` are explicitly excluded — see Decomposition).

- **Dimension 1.1** — every migrated route's method-allow/deny behavior is unchanged (a GET-only route still rejects POST, etc.) → Test `test_route_method_checks_unchanged_after_migration` (parametrized over each migrated route, reusing existing route-level integration tests rather than writing 63 new ones — the migration must not change what's asserted, only how it's expressed).
- **Dimension 1.2** — `requireMethod` itself returns `false` and calls `respondMethodNotAllowed` on a mismatch, `true` and no side effect on a match → Test `test_require_method_matches_and_rejects_correctly`.

### §2 — PostHog loader recovers from a failed load

`ensureLoader`'s unguarded `loadPromise = loadPosthog(cfg);` leaves `loadPromise` permanently set to a rejected promise on any dynamic-import/init failure, wedging analytics for the rest of the page session. **Implementation default:** port the app package's `try/catch`-around-`await import(...)` pattern (`initAnalytics`, lines 142-152) into the website package's `loadPosthog`/`ensureLoader`, resetting `loadPromise` to `null` on failure so a later call can retry — because that's the exact working pattern already in this codebase for the sibling loader.

- **Dimension 2.1 — DONE** — a failed `loadPosthog` call resets `loadPromise` (and leaves `posthogModule` null, since the client is bound only after `init()` succeeds) so a subsequent `ensureLoader`/`track()` retries instead of short-circuiting → Test `test_ensure_loader_retries_after_failed_load`.
- **Dimension 2.2 — DONE** — a failed load does not produce an unhandled promise rejection (the `.catch` handles it) → Test `test_failed_load_does_not_produce_unhandled_rejection`.

> **Implementation note (2.1):** the faithful port surfaced a second latent bug — `loadPosthog` set `posthogModule = mod.default` *before* `init()`, so a failed init left the module-level client non-null and `track()` would `capture()` against an uninitialised client instead of retrying. The fix binds `posthogModule` only after `init()` succeeds (exactly the app sibling's local-var-then-assign shape), so the retry actually reloads.

---

### §4 — Operator UI (runners, admin models) gates on scope, not the `platform_admin` boolean

`readPlatformAdminClaim()` (`ui/packages/app/lib/auth/platform.ts`) gates the operator nav, the `admin/runners` + `admin/models` pages, and their `asPlatformAdmin`-wrapped Server Actions on a boolean `metadata.platform_admin === true` — a pre-M104 claim the scope migration never retired. The backend gates the same routes on scopes (`route_scopes.zig:120-127`: enroll → `runner:enroll`, runner list → `runner:read`, model mutate → `model:admin`, model read → `model:read`), so a correctly-scoped operator still can't see the UI and platform-admin is configured twice, which drifts. **Implementation default:** read the session `scopes` claim (top-level, space-delimited) and gate each surface on the scope its backend route enforces — page/nav visibility on the read scope (`runner:read`, `model:read`), each mutating action on its write scope (`runner:enroll`, `runner:write`, `model:admin`); delete `readPlatformAdminClaim`/`platform_admin` once no caller remains, and correct the stale `UZ-AUTH-021` comment in `admin/runners/page.tsx` (the gate returns `UZ-AUTH-022`).

- **Dimension 4.1 — DONE** — the runner operator nav + page render iff the session `scopes` claim holds `runner:read`, and the enroll action is allowed iff it holds `runner:enroll`; the `platform_admin` boolean is never consulted → Test `test_runner_ui_gates_on_runner_scopes` (`tests/runners-actions.test.ts`, `tests/runners-page.test.ts`, `tests/app-components.test.ts`).
- **Dimension 4.2 — DONE** — the admin-models nav/page/actions gate on `model:read`/`model:admin`, and `readPlatformAdminClaim`/`platform_admin`/`isPlatformAdmin` have zero remaining references in production source → Test `test_admin_models_ui_gates_on_model_scope_no_boolean_left` (`tests/admin-models-actions.test.ts`, `lib/auth/platform.test.ts` source scan).

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| (existing) PostHog delivery itself | product | §2's fix restores delivery after a transient failure, rather than adding a new event | n/a — this fixes an existing pipeline, doesn't add one | n/a | `test_ensure_loader_retries_after_failed_load` |

No new event names in §1/§4 (§4 is an auth-gate reconciliation, no product signal). §2's fix is itself the analytics-reliability improvement — no separate event needed to prove it beyond the retry test; `not applicable` for any funnel/playbook change since no event shape changes.

---

## Interfaces

```
common.requireMethod(res: *httpz.Response, method: httpz.Method, expected: httpz.Method) bool
  -- true + no side effect on match; false + writes 405 via respondMethodNotAllowed on mismatch.

ensureLoader/loadPosthog: no exported signature change; internal loadPromise
state-machine gains a failure→null transition instead of failure→permanently-rejected.

platform.ts: readPlatformAdminClaim(): Promise<boolean> is removed; callers switch
to hasScope(scope): boolean over the session `scopes` claim. Per-surface predicate
map is fixed by route_scopes.zig (runner:read/runner:enroll, model:read/model:admin),
not free choice.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Migrated route's method check behaves differently | copy-paste error during the 63-site migration | Dimension 1.1's regression test catches any behavioral drift before merge. |
| PostHog load fails again after a retry | sustained outage, not transient | `loadPromise` resets again, next call retries again — no infinite-backoff/circuit-breaker added in this patch (Out of Scope), so repeated failures mean repeated retries, which is a strict improvement over "wedged forever" but not a rate-limited retry policy. |
| Operator UI shown without backend authority | §4 UI scope predicate diverges from the route's enforced scope | §4.1/4.2 assert the UI predicate matches `route_scopes.zig`; the backend `requireScope` still 403s (`UZ-AUTH-022`) as the real gate — the UI check is defence-in-depth only, never the sole authority. |

---

## Invariants

1. Every single-method route's allow/deny behavior is identical before and after the `requireMethod` migration — enforced by Dimension 1.1's per-route regression coverage.
2. `ensureLoader` never permanently stops retrying after a single failed load — enforced by Dimension 2.1's test asserting `loadPromise` is resettable.
3. No operator **production** surface is gated on the `platform_admin` boolean after §4 — enforced by Dimension 4.2's source scan over `lib/auth`, `app/(dashboard)/admin`, `components/layout` (test files excluded, since the guard test that asserts absence must itself name the retired identifiers).
4. The UI scope gate never becomes the sole authority — the backend `requireScope` (`route_scopes.zig`) stays authoritative; §4 is defence-in-depth only (Failure Modes row 3).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|-----------------------------------------------|
| 1.1 | integration | `test_route_method_checks_unchanged_after_migration` | for each migrated single-method route, the wrong-method request still gets 405 and the right-method request is unaffected (reuses/extends existing per-route integration tests). |
| 1.2 | unit | `test_require_method_matches_and_rejects_correctly` | `requireMethod(res, .GET, .POST)` → `false` + response body/status set to 405; `requireMethod(res, .POST, .POST)` → `true` + response untouched. |
| 2.1 | unit | `test_ensure_loader_retries_after_failed_load` | `loadPosthog` mocked to reject once → `ensureLoader` call after the rejection settles retries (calls `loadPosthog` again), not short-circuited by a stale `loadPromise`. |
| 2.2 | unit | `test_failed_load_does_not_produce_unhandled_rejection` | a rejected `loadPosthog` call is observably caught (test harness asserts no `unhandledrejection` event fires). |
| 4.1 | unit | `test_runner_ui_gates_on_runner_scopes` | session scopes `…runner:read runner:enroll…` → nav+page render and enroll allowed; scopes lacking them → hidden/redirect; the `platform_admin` boolean is absent from the decision. |
| 4.2 | unit | `test_admin_models_ui_gates_on_model_scope_no_boolean_left` | admin-models gate resolves via `model:read`/`model:admin`; the Invariant-3 source scan (production files, tests excluded) finds zero `platform_admin`/`readPlatformAdminClaim`/`isPlatformAdmin` references. |

Regression: Dimension 1.1 is the explicit regression case (§1 pure refactor). Idempotency/replay: N/A — none of these sections add retry-with-side-effects semantics (§2's retry is a pure re-attempt of an idempotent load).

---

## Acceptance Criteria

- [ ] `requireMethod` matches `requireUuidV7Id`'s convention, all 63 single-method sites migrated — verify: `zig build test --summary all` (Dimensions 1.1/1.2)
- [ ] PostHog loader retries after a failed load — verify: `bun test ui/packages/website` (Dimensions 2.1/2.2)
- [ ] Operator UI gates on scope, `platform_admin` fully removed from production — verify: `bun test ui/packages/app` + E9 sweep (production files, tests excluded) empty (Dimensions 4.1/4.2)
- [ ] `make lint` clean · `make test` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: Zig test suite
zig build test --summary all && echo "PASS" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -5
# E3: UI tests
cd ui/packages/website && bun test src/analytics/posthog.test.ts
cd ui/packages/app && bun test lib/auth/platform.test.ts
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweep — confirm no remaining hand-copied single-method check outside requireMethod
grep -rn "respondMethodNotAllowed" src/agentsfleetd/http/route_table_invoke*.zig | grep -v "requireMethod\|fn respondMethodNotAllowed\|switch (req.method)"
# E9: §4 zero-reference sweep — platform_admin boolean fully retired from PRODUCTION
# (test files excluded: the Invariant-3 guard test names the identifiers to enforce their absence)
grep -rn "platform_admin\|readPlatformAdminClaim\|isPlatformAdmin" ui/packages/app | grep -v node_modules | grep -vE '\.test\.|/tests/'
```

---

## Dead Code Sweep

`readPlatformAdminClaim` (`ui/packages/app/lib/auth/platform.ts`) and every `platform_admin` reference are removed once §4's scope predicates replace all callers (RULE NDC + Invariant 3; E9 above is the zero-reference proof). §1 replaces inline blocks with calls in place; §2 edits an existing function.

---

## Discovery (consult log)

- **§1 scope note:** the 10 multi-method `switch` sites in `route_table_invoke.zig` are deliberately not migrated to a new `requireMethodOneOf` helper in this patch (see Decomposition) — flagged as an optional follow-up, not blocking.
- **§3 drop (resolved before EXECUTE):** the original §3 (restore `ROLE_NAMESPACE_DEV`) was dropped — the static-role concept the namespaced claim fed has been removed from the product, so restoring the dev/prod namespace split reinstates plumbing for a claim nothing reads. Ack captured in the header Scope amendment. Any residual dead role-claim machinery in `cli/src/program/auth-token.ts` is a separate cleanup, not in this branch's scope.
- **§4 note:** the UI operator gate is defence-in-depth; the backend `requireScope` (`route_scopes.zig`) stays authoritative. The per-surface scope map (`runner:read`/`runner:enroll`/`runner:write`, `model:read`/`model:admin`) is taken from `route_scopes.zig:120-127`, not guessed. Live-verified this session: the dev session token now carries a top-level `scopes` claim, and `metadata.platform_admin` is absent — the boolean gate hides the surface even for a fully-scoped operator, which is the bug §4 fixes.
- **§4 PROD rollout dependency (load-bearing):** the top-level `scopes` claim is projected into the Clerk session token by a **Clerk-dashboard "Session Token Claims" config** — applied on DEV, **pending on PROD** (`docs/AUTH.md:626`, "DEV applied; PROD pending operator click"). No code change makes it appear. Because §4 removes the `platform_admin` boolean and gates purely on `scopes`, PROD operators are hidden until that PROD config lands AND the operator user carries the scopes on `public_metadata.scopes`. This is a **hard deploy prerequisite**, documented in the PR, `docs/AUTH.md`, and the docs repo. Ack: `> Indy (2026-07-02): "remove the platform_admin entirely, document the scope in PR, docs/AUTH.md, docs repo, and i will update it in the clerk those two permissions for nkishore@megam.io" — context: §4 PROD Clerk config is Indy's manual step; ship clean.`
- **§4 error-code reconciliation:** the UI minted `UZ-AUTH-021` (`PLATFORM_ADMIN_REQUIRED`) on a failed gate, but that code is retired backend-side — the operator routes return `UZ-AUTH-022` (insufficient scope) now (`grep UZ-AUTH-021 src/` → none; `error_entries.zig:79` defines only `022`). §4 switches the UI mint + `lib/errors.ts` copy to `UZ-AUTH-022` (`INSUFFICIENT_SCOPE`), removing the drift. Blast radius wider than the pre-spec Files-Changed table anticipated (LLM-drafted spec, per provenance): `lib/errors.ts`, `Shell.tsx`, `lib/auth/scopes.ts`, and six test files added — table reconciled above.
- **Metrics review:** no new events; §2 restores existing PostHog delivery reliability, no playbook update required (no funnel/event shape changes).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test --summary all` | {paste snippet} | |
| UI tests (website) | `bun test ui/packages/website` | {paste snippet} | |
| UI tests (app) | `bun test ui/packages/app` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Dead code sweep | E8 + E9 above | {paste snippet} | |

---

## Out of Scope

- A `requireMethodOneOf` helper for the 10 multi-method `switch` sites — flagged as an optional follow-up, not built here (see Decomposition).
- A rate-limited/backoff retry policy for the PostHog loader beyond "don't stay permanently wedged" — this patch restores retry-ability, not a full backoff design.
- Restoring the `ROLE_NAMESPACE_DEV`/`ROLE_NAMESPACE_COM` split, or any change to `cli/src/program/auth-token.ts` — dropped (see header Scope amendment); static roles are removed, so the namespaced role-claim probe is vestigial. Cleaning up any now-dead role-claim machinery is a separate follow-up, not this branch.
