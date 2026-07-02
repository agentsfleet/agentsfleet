<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M109_004: Deduplicate route method-checks, fix a permanently-wedged PostHog loader, resolve a collapsed dev/prod claim namespace, and gate the operator UI on scope not the `platform_admin` boolean

**Prototype:** v2.0.0
**Milestone:** M109
**Workstream:** 004
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Test Baseline:** unit=2270 integration=243
**Priority:** P1 — the PostHog loader permanently stops delivering analytics after one transient failure for the rest of a page session (silent product-signal loss); the JWT role-claim namespace collapse is a real dev/prod distinction lost during a brand rename (auth-adjacent, though not itself a bypass); the operator dashboard (runners, admin models) gates on a legacy `platform_admin` boolean the M104 scope migration left behind, so a correctly-scoped operator cannot reach the surface and platform-admin must be configured twice (boolean + scopes), which drift; the route-method duplication is a maintainability/DRY issue at P2-grade severity kept in this workstream only because it shares no scope conflict with the others.
**Categories:** API CLI UI
**Batch:** B1 — independent of M109_001/002/003; no shared files.
**Branch:** feat/m109-004-cross-surface-hygiene
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, fleet-wide-refactor-audit `Workflow` run `wf_8ec169f4-8e4`, each finding independently re-verified against current source before this spec was drafted, Jul 02, 2026).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/AUTH.md` — §3's role-claim resolution (`extractRoleFromToken`) is CLI-side auth surface; per `AGENTS.md`'s auth-flow trigger, `docs/AUTH.md` is read before touching it.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/common.zig:285-303` — `requireUuidV7Id` (285-296) is the exact naming/shape convention `requireMethod` must follow (`require<X>(res, ...) bool`, side-effects a response on failure); `respondMethodNotAllowed` (300-303) is the primitive it wraps.
2. `ui/packages/app/lib/analytics/posthog.ts:142-152` — the sibling app-package PostHog loader already handles a failed dynamic import correctly (`try/catch` around `await import("posthog-js")`, flips a flag rather than leaving a permanently-rejected promise); this is the exact pattern §2 ports into the website package.
3. `cli/src/program/auth-token.ts:45-47,74-83` — `ROLE_NAMESPACE_DEV`/`ROLE_NAMESPACE_COM` and the `candidates` array probing both; `docs/AUTH.md` — read before touching any auth-flow file per `AGENTS.md`'s auth-flow trigger, to confirm the dev-instance namespace domain against the live Clerk dev app rather than guessing.
4. `docs/AUTH.md:210,782` — documents `iss=https://clerk.dev.agentsfleet.net` for the dev Clerk instance, the closest existing evidence for what `ROLE_NAMESPACE_DEV` should resolve to; this is a strong lead, not a confirmed live value (see §3's Discovery consult).
5. `ui/packages/app/lib/auth/platform.ts` + `src/agentsfleetd/http/route_scopes.zig:120-127` — the frontend `platform_admin` boolean gate to reconcile (§4) and the per-route scopes it must mirror (`runner:enroll` enroll, `runner:read` list, `model:read`/`model:admin` models); the session token now carries a top-level `scopes` claim the gate reads instead.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Add requireMethod helper, fix PostHog's permanently-wedged loader, restore the dev/prod role-claim namespace split
- **Intent (one sentence):** A malformed route no longer needs a hand-copied 4-line check at 63 call sites, a transient PostHog load failure no longer kills analytics for the rest of the page session, dev/prod JWT role claims are probed under their actual distinct namespaces again, and the operator UI is gated on the same scopes the backend enforces instead of a divergent `platform_admin` boolean.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a new route handler is added and its author calls `common.requireMethod(...)` instead of copy-pasting the 4-line block; a visitor whose first PostHog load fails (ad-blocker hiccup, offline blip) still gets analytics once connectivity returns, instead of the tab going permanently dark; a dev-environment API client's `role` claim under the dev Clerk instance's actual namespace resolves correctly instead of silently falling through to the (wrong) prod-collapsed string.
2. **Preserved user behaviour** — every route's actual method-allow/deny behavior is byte-for-byte unchanged (§1 is a pure refactor); PostHog's happy-path load is unchanged (§2 only changes the failure branch); prod role-claim resolution (`https://agentsfleet.net/role`) is unchanged (§3 only restores the dev-specific value, doesn't touch prod).
3. **Optimal-way check** — §1's optimal fix is exactly a shared boolean-returning helper matching the file's own existing convention; no larger routing-layer refactor is warranted for a DRY issue. §2/§3's optimal fixes are equally narrow — a `.catch`/reset and a corrected constant, respectively.
4. **Rebuild-vs-iterate** — iterate on all three; each has a working sibling/reference (the app-package PostHog loader, the `requireUuidV7Id` convention, the dev Clerk instance's actual issuer domain) to converge onto rather than a redesign.
5. **What we build** — `common.requireMethod` (single-method call sites migrate to it; multi-method `switch` sites are evaluated separately per §1's Discovery note); a `.catch`/retry-reset on `ensureLoader`'s `loadPromise` assignment in the website package; a corrected `ROLE_NAMESPACE_DEV` value confirmed against the live dev Clerk instance; a scope-based operator-UI gate replacing `readPlatformAdminClaim`/`platform_admin` with per-surface session-`scopes` checks.
6. **What we do NOT build** — a `requireMethodOneOf` for the 10 multi-method `switch` sites (Discovery records why single- vs multi-method sites are treated differently, but building the multi-method helper is flagged as a follow-up, not blocking); a rewrite of PostHog's retry/backoff policy beyond "don't stay permanently wedged"; any change to the prod role-claim namespace.
7. **Fit with existing features** — §1 must not change any route's actual method-allow set (regression risk is purely mechanical — a copy-paste-to-call-site error); §3 must not weaken prod auth in any way — this is an auth-flow file, `docs/AUTH.md` is read first per the gate, and the dev-namespace value is confirmed against the live Clerk dev instance before landing, not guessed from `docs/AUTH.md` alone.
8. **Surface order** — API-first for §1 (no CLI/UI surface); UI-first for §2 (website analytics loader); CLI-first for §3 (`agentsfleet` auth-token resolution).
9. **Dashboard restraint** — N/A, no new UI controls; §2 is an internal loader fix with no visible UI change.
10. **Confused-user next step** — §1's confused-user case is a future route author who doesn't know the helper exists — the PR itself (and this spec's Prior-Art pointer) is the self-serve doc; §2/§3 have no direct end-user-facing confusion (both are silent-failure classes whose "user" is the analytics pipeline / the JWT-role resolution path itself).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NLR (touch-it-fix-it — once `requireMethod` exists, migrate every existing call site in the 9-file family, not just new code going forward).
- **`dispatch/write_zig.md`** — §1 touches `*.zig` across 9 files; file/function length caps (`route_table_invoke.zig` stays under 350 lines after the 27-site migration — verify, don't assume).
- **`dispatch/write_ts_adhere_bun.md`** — §2/§3 touch `*.ts`; const/import discipline.
- **`docs/AUTH.md`** — §3 is an auth-flow file per `AGENTS.md`'s trigger table; read before EXECUTE, not just referenced.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §1 | cross-compile both linux targets after the `requireMethod` addition + 27+15+6+4+3+3+2+2+1 = 63 call-site migrations across the 9-file family. |
| PUB / Struct-Shape | yes — §1 | `requireMethod(res, method, expected) bool` matches `requireUuidV7Id`'s established shape exactly — verdict: no new struct, a plain function alongside its sibling in `common.zig`. |
| File & Function Length (≤350/≤50/≤70) | yes — §1 | `route_table_invoke.zig` is the largest file in the family (348 lines pre-migration per the research pass); each single-method site collapsing from 4 lines to 1 line should net-shrink it, but verify post-migration, don't assume. |
| UFS | no | no new repeated/semantic literal introduced; §1 removes duplication, §3 corrects one. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | none of these three findings touch logging, lifecycle, error-registry, or schema surfaces. |
| UI Substitution / DESIGN TOKEN | no | §2 is a loader-internals fix, no visible UI/design-token surface. |

---

## Overview

**Goal (testable):** a route handler needing a single-method check calls `common.requireMethod(res, req.method, .POST)` instead of a 4-line hand-copied block, at all 63 existing single-method sites across the 9-file `route_table_invoke*` family; a PostHog load failure in the website package resets `loadPromise` so a later `ensureLoader`/`track()` call retries instead of being permanently wedged; `ROLE_NAMESPACE_DEV` resolves to the dev Clerk instance's actual claim namespace, confirmed live, not the prod-collapsed string it currently duplicates.

**Problem:** 63 occurrences of an identical 4-line method-check pattern across 9 files, with no shared helper, means every future route risks a copy-paste drift. `ensureLoader`'s `loadPromise = loadPosthog(cfg);` has no `.catch`; once `loadPosthog` rejects (a blocked/offline chunk load), `loadPromise` stays set to the rejected promise forever, so every subsequent page interaction on that session silently stops delivering analytics — with an unhandled-rejection console error as the only symptom. `ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` are byte-identical strings today, but git history shows they were genuinely different (`usezombie.dev` vs `usezombie.com`) until a two-step brand-rename mechanically collapsed both onto the new prod domain — losing the dev/prod claim-namespace distinction as an unintended side effect, not a deliberate simplification.

**Solution summary:** Add `common.requireMethod` mirroring the file's own `requireUuidV7Id` convention and migrate the 63 single-method call sites (multi-method `switch` sites stay as-is per Discovery). Port the app package's `try/catch`-around-dynamic-import pattern into the website package's `ensureLoader`/`loadPosthog`, resetting `loadPromise` on failure so retry is possible. Confirm the dev Clerk instance's actual role-claim namespace domain against its live JWT template/session-token config, then set `ROLE_NAMESPACE_DEV` to that value (distinct from `ROLE_NAMESPACE_COM`). Replace the frontend `platform_admin` boolean gate with per-surface session-`scopes` predicates keyed to the scope each operator route enforces in `route_scopes.zig`, removing the divergent second source of truth.

---

## Prior-Art / Reference Implementations

- **API (§1)** → `src/agentsfleetd/http/handlers/common.zig:285-296`, `requireUuidV7Id`. **Alignment:** exact naming/shape convention (`require<X>`, side-effect + bool return), same file. **Divergence:** none for single-method sites; the 10 multi-method `switch` sites are explicitly not forced through this helper (see Decomposition).
- **UI (§2)** → `ui/packages/app/lib/analytics/posthog.ts:142-152`, `initAnalytics`'s `try/catch` around `await import("posthog-js")`. **Alignment:** same failure-isolation shape, ported from the sibling app package to the website package. **Divergence:** website's `ensureLoader` is a fire-and-forget sync function wrapping an async `loadPosthog`, not an async function itself — the `.catch` attaches to the promise assignment rather than wrapping an `await`, but the effect (isolate the failure, don't leave state permanently poisoned) is identical.
- **CLI (§3)** → `docs/AUTH.md:210,782` (dev Clerk instance issuer `clerk.dev.agentsfleet.net`) + the live Clerk dev-app dashboard (`dashboard.clerk.com` → `clerk-dev` app's JWT template / custom-claims config) as the actual source of truth for the dev namespace value — the doc is a strong lead, not a substitute for confirming the live config.

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
| `cli/src/program/auth-token.ts` | EDIT | `ROLE_NAMESPACE_DEV` set to the confirmed dev-instance namespace value, distinct from `ROLE_NAMESPACE_COM`. |
| `cli/test/auth-token.unit.test.ts` | EDIT | add a case asserting a dev-namespace claim resolves correctly (currently only the collapsed prod string is exercised). |
| `ui/packages/app/lib/auth/platform.ts` | EDIT | replace the `platform_admin` boolean read with a session-`scopes` predicate (`hasScope`); retire `readPlatformAdminClaim`. |
| `ui/packages/app/app/(dashboard)/layout.tsx` | EDIT | operator nav gating switches to per-surface scope checks. |
| `ui/packages/app/app/(dashboard)/admin/runners/{page.tsx,actions.ts}` | EDIT | gate on `runner:read` (view) / `runner:enroll` (enroll); fix the stale `UZ-AUTH-021` comment → `UZ-AUTH-022`. |
| `ui/packages/app/app/(dashboard)/admin/models/{page.tsx,actions.ts}` | EDIT | gate on `model:read` (view) / `model:admin` (mutate). |
| `ui/packages/app/lib/auth/platform.test.ts` | CREATE/EDIT | scope-predicate coverage + zero-remaining-`platform_admin`-reference assertion. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three independent, small patches sharing a workstream; grouped because none conflicts with M109_001/002/003's files and each is a self-contained hygiene fix.
- **Alternatives considered:** for §1, forcing all 10 multi-method `switch` sites through a `requireMethodOneOf` helper was considered and rejected for this patch — the switch shape already reads clearly and collapsing it risks losing per-method routing clarity for a marginal DRY gain; flagged as an optional follow-up in Out of Scope, not built here. For §3, deleting `ROLE_NAMESPACE_COM` entirely (since Clerk's default claim location might make custom namespace probing unnecessary) was considered and rejected without evidence — the spec fixes the confirmed regression (namespaces collapsed) without also removing a still-potentially-load-bearing probe.
- **§4 verdict:** **patch** — a scope-predicate swap at the existing gate call sites, no auth-model redesign; the backend `requireScope` gate is unchanged and remains authoritative (the UI gate is defence-in-depth). Removing the `platform_admin` boolean entirely (vs leaving it as a legacy alias) is chosen because a dual source of truth is exactly the drift that caused this.
- **Patch-vs-refactor verdict:** **patch**, all four.

---

## Sections (implementation slices)

### §1 — `requireMethod` helper, single-method call sites migrated

63 occurrences of an identical 4-line pattern across 9 files, no shared helper. **Implementation default:** add `requireMethod` matching `requireUuidV7Id`'s exact shape; migrate all 27 single-method sites in `route_table_invoke.zig` plus the remaining single-method sites in the other 8 files (the 10 multi-method `switch` sites in `route_table_invoke.zig` are explicitly excluded — see Decomposition).

- **Dimension 1.1** — every migrated route's method-allow/deny behavior is unchanged (a GET-only route still rejects POST, etc.) → Test `test_route_method_checks_unchanged_after_migration` (parametrized over each migrated route, reusing existing route-level integration tests rather than writing 63 new ones — the migration must not change what's asserted, only how it's expressed).
- **Dimension 1.2** — `requireMethod` itself returns `false` and calls `respondMethodNotAllowed` on a mismatch, `true` and no side effect on a match → Test `test_require_method_matches_and_rejects_correctly`.

### §2 — PostHog loader recovers from a failed load

`ensureLoader`'s unguarded `loadPromise = loadPosthog(cfg);` leaves `loadPromise` permanently set to a rejected promise on any dynamic-import/init failure, wedging analytics for the rest of the page session. **Implementation default:** port the app package's `try/catch`-around-`await import(...)` pattern (`initAnalytics`, lines 142-152) into the website package's `loadPosthog`/`ensureLoader`, resetting `loadPromise` to `null` on failure so a later call can retry — because that's the exact working pattern already in this codebase for the sibling loader.

- **Dimension 2.1** — a failed `loadPosthog` call resets state so a subsequent `ensureLoader` call retries instead of returning immediately due to a stale `loadPromise` → Test `test_ensure_loader_retries_after_failed_load`.
- **Dimension 2.2** — a failed load does not produce an unhandled promise rejection in the console → Test `test_failed_load_does_not_produce_unhandled_rejection`.

### §3 — Dev/prod role-claim namespace restored

`ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` are byte-identical today; git history confirms they were genuinely distinct (`usezombie.dev`/`usezombie.com`) before a brand-rename commit collapsed both. **Implementation default:** confirm the dev Clerk instance's actual role-claim namespace domain against its live JWT template/custom-claims configuration (not guessed from `docs/AUTH.md` alone — that doc is a lead, not a substitute for the live check), then set `ROLE_NAMESPACE_DEV` to that confirmed value.

- **Dimension 3.1** — `ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` are distinct string values, each resolving correctly against a token minted by its respective Clerk instance → Test `test_extract_role_resolves_dev_and_prod_namespaces_distinctly`.
- **Dimension 3.2** — a token with the prod namespace claim still resolves correctly (regression — prod behavior is unchanged) → Test `test_extract_role_resolves_prod_namespace_unchanged`.

---

### §4 — Operator UI (runners, admin models) gates on scope, not the `platform_admin` boolean

`readPlatformAdminClaim()` (`ui/packages/app/lib/auth/platform.ts`) gates the operator nav, the `admin/runners` + `admin/models` pages, and their `asPlatformAdmin`-wrapped Server Actions on a boolean `metadata.platform_admin === true` — a pre-M104 claim the scope migration never retired. The backend gates the same routes on scopes (`route_scopes.zig:120-127`: enroll → `runner:enroll`, runner list → `runner:read`, model mutate → `model:admin`, model read → `model:read`), so a correctly-scoped operator still can't see the UI and platform-admin is configured twice, which drifts. **Implementation default:** read the session `scopes` claim (top-level, space-delimited) and gate each surface on the scope its backend route enforces — page/nav visibility on the read scope (`runner:read`, `model:read`), each mutating action on its write scope (`runner:enroll`, `runner:write`, `model:admin`); delete `readPlatformAdminClaim`/`platform_admin` once no caller remains, and correct the stale `UZ-AUTH-021` comment in `admin/runners/page.tsx` (the gate returns `UZ-AUTH-022`).

- **Dimension 4.1** — the runner operator nav + page render iff the session `scopes` claim holds `runner:read`, and the enroll action is allowed iff it holds `runner:enroll`; the `platform_admin` boolean is never consulted → Test `test_runner_ui_gates_on_runner_scopes`.
- **Dimension 4.2** — the admin-models nav/page/actions gate on `model:read`/`model:admin`, and `readPlatformAdminClaim`/`platform_admin` have zero remaining references repo-wide → Test `test_admin_models_ui_gates_on_model_scope_no_boolean_left`.

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| (existing) PostHog delivery itself | product | §2's fix restores delivery after a transient failure, rather than adding a new event | n/a — this fixes an existing pipeline, doesn't add one | n/a | `test_ensure_loader_retries_after_failed_load` |

No new event names in §1/§3/§4 (§4 is an auth-gate reconciliation, no product signal). §2's fix is itself the analytics-reliability improvement — no separate event needed to prove it beyond the retry test; `not applicable` for any funnel/playbook change since no event shape changes.

---

## Interfaces

```
common.requireMethod(res: *httpz.Response, method: httpz.Method, expected: httpz.Method) bool
  -- true + no side effect on match; false + writes 405 via respondMethodNotAllowed on mismatch.

ensureLoader/loadPosthog: no exported signature change; internal loadPromise
state-machine gains a failure→null transition instead of failure→permanently-rejected.

ROLE_NAMESPACE_DEV: same `string` constant type, new confirmed value distinct
from ROLE_NAMESPACE_COM; extractRoleFromToken's candidates array unchanged in
shape (still 8 entries), only the DEV constant's value changes.

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
| Dev Clerk instance's actual namespace differs from `docs/AUTH.md`'s documented value | doc drift between what's written and the live Clerk config | §3's Discovery consult requires confirming against the live dashboard before landing, not trusting the doc alone. |
| Operator UI shown without backend authority | §4 UI scope predicate diverges from the route's enforced scope | §4.1/4.2 assert the UI predicate matches `route_scopes.zig`; the backend `requireScope` still 403s (`UZ-AUTH-022`) as the real gate — the UI check is defence-in-depth only, never the sole authority. |

---

## Invariants

1. Every single-method route's allow/deny behavior is identical before and after the `requireMethod` migration — enforced by Dimension 1.1's per-route regression coverage.
2. `ensureLoader` never permanently stops retrying after a single failed load — enforced by Dimension 2.1's test asserting `loadPromise` is resettable.
3. `ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` are never byte-identical after this fix — enforced by Dimension 3.1's test asserting distinct resolution.
4. No operator surface is gated on the `platform_admin` boolean after §4 — enforced by Dimension 4.2's repo-wide zero-reference grep for `platform_admin`/`readPlatformAdminClaim`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|-----------------------------------------------|
| 1.1 | integration | `test_route_method_checks_unchanged_after_migration` | for each migrated single-method route, the wrong-method request still gets 405 and the right-method request is unaffected (reuses/extends existing per-route integration tests). |
| 1.2 | unit | `test_require_method_matches_and_rejects_correctly` | `requireMethod(res, .GET, .POST)` → `false` + response body/status set to 405; `requireMethod(res, .POST, .POST)` → `true` + response untouched. |
| 2.1 | unit | `test_ensure_loader_retries_after_failed_load` | `loadPosthog` mocked to reject once → `ensureLoader` call after the rejection settles retries (calls `loadPosthog` again), not short-circuited by a stale `loadPromise`. |
| 2.2 | unit | `test_failed_load_does_not_produce_unhandled_rejection` | a rejected `loadPosthog` call is observably caught (test harness asserts no `unhandledrejection` event fires). |
| 3.1 | unit | `test_extract_role_resolves_dev_and_prod_namespaces_distinctly` | a token with the claim under the confirmed dev namespace resolves the role; a token with only the prod namespace claim present does not falsely match under the dev constant. |
| 3.2 | unit | `test_extract_role_resolves_prod_namespace_unchanged` | a token with the claim under `ROLE_NAMESPACE_COM` (`https://agentsfleet.net/role`) resolves correctly, unchanged from current behavior. |
| 4.1 | unit | `test_runner_ui_gates_on_runner_scopes` | session scopes `…runner:read runner:enroll…` → nav+page render and enroll allowed; scopes lacking them → hidden/redirect; the `platform_admin` boolean is absent from the decision. |
| 4.2 | unit | `test_admin_models_ui_gates_on_model_scope_no_boolean_left` | admin-models gate resolves via `model:read`/`model:admin`; `grep -rn "platform_admin\|readPlatformAdminClaim" ui/packages/app` → 0 matches. |

Regression: Dimension 1.1 and 3.2 are the explicit regression cases. Idempotency/replay: N/A — none of these three sections add retry-with-side-effects semantics (§2's retry is a pure re-attempt of an idempotent load).

---

## Acceptance Criteria

- [ ] `requireMethod` matches `requireUuidV7Id`'s convention, all 63 single-method sites migrated — verify: `zig build test --summary all` (Dimensions 1.1/1.2)
- [ ] PostHog loader retries after a failed load — verify: `bun test ui/packages/website` (Dimensions 2.1/2.2)
- [ ] Dev/prod role namespaces resolve distinctly — verify: `bun test cli/test/auth-token.unit.test.ts` (Dimensions 3.1/3.2)
- [ ] Operator UI gates on scope, `platform_admin` fully removed — verify: `bun test ui/packages/app` + `grep -rn "platform_admin\|readPlatformAdminClaim" ui/packages/app | grep -v node_modules` empty (Dimensions 4.1/4.2)
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
# E3: UI + CLI tests
cd ui/packages/website && bun test src/analytics/posthog.test.ts
cd cli && bun test test/auth-token.unit.test.ts
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
```

---

## Dead Code Sweep

N/A — no files deleted; §1 replaces inline blocks with calls in place, §2/§3 edit existing functions/constants.

---

## Discovery (consult log)

- **§3 consult (resolve before EXECUTE, auth-flow gate):** `docs/AUTH.md:210,782` documents `clerk.dev.agentsfleet.net` as the dev issuer, but the exact role-claim namespace *key* used in JWT custom claims must be confirmed against the live dev Clerk instance's JWT template/session-token config (`dashboard.clerk.com` → `clerk-dev` app), not assumed from the issuer domain alone. This line records the confirmed value once checked.
- **§1 scope note:** the 10 multi-method `switch` sites in `route_table_invoke.zig` are deliberately not migrated to a new `requireMethodOneOf` helper in this patch (see Decomposition) — flagged as an optional follow-up, not blocking.
- **§4 note:** the UI operator gate is defence-in-depth; the backend `requireScope` (`route_scopes.zig`) stays authoritative. The per-surface scope map (`runner:read`/`runner:enroll`, `model:read`/`model:admin`) is taken from `route_scopes.zig:120-127`, not guessed. Live-verified this session: the dev session token now carries a top-level `scopes` claim, and `metadata.platform_admin` is absent — the boolean gate hides the surface even for a fully-scoped operator, which is the bug §4 fixes.
- **Metrics review:** no new events; §2 restores existing PostHog delivery reliability, no playbook update required (no funnel/event shape changes).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, `docs/AUTH.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test --summary all` | {paste snippet} | |
| UI tests | `bun test ui/packages/website` | {paste snippet} | |
| CLI tests | `bun test cli/test/auth-token.unit.test.ts` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Dead code sweep | E8 above | {paste snippet} | |

---

## Out of Scope

- A `requireMethodOneOf` helper for the 10 multi-method `switch` sites — flagged as an optional follow-up, not built here (see Decomposition).
- A rate-limited/backoff retry policy for the PostHog loader beyond "don't stay permanently wedged" — this patch restores retry-ability, not a full backoff design.
- Any change to prod's role-claim namespace (`ROLE_NAMESPACE_COM` / `https://agentsfleet.net/role`) — only the collapsed dev value is corrected.
