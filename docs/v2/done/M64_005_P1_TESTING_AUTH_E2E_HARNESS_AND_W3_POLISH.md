<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_005: Authenticated e2e harness + W3 polish

> **DESIGN AMENDMENT (May 09, 2026 — post-M65_001):** the `plan_tier` carry-over
> work scoped here is **dropped**. M65_001 ripped `PlanTier` (Hobby/Scale) out
> of the entire stack — schema, Zig billing facade, OpenAPI, UI, marketing site
> — in favour of a single per-event + per-stage rate plus a one-time starter
> credit. The acceptance criteria below that reference rendering a
> `plan_tier` badge on `BillingBalanceCard`, surfacing the field in
> `lib/api/tenant_billing.ts`, or asserting it in `settings-billing.spec.ts`
> are now obsolete and should NOT be implemented. The remaining e2e-harness
> + W3-polish scope (Clerk admin-API JWT mounting, `/sign-up` → `/zombies/[id]`
> drive, function-coverage gap) is unaffected and still in scope.

> **AMENDMENT (May 10, 2026 — at CHORE(open)):** three corrections folded in:
> (1) **Clerk credential** — the spec referenced `op://ZMB_CD_DEV/clerk-test-backend-token`,
> which does not exist. Canonical vault layout (per `playbooks/002_preflight/02_credentials.sh`
> and `playbooks/012_usezombie_admin_bootstrap`) is `op://ZMB_CD_DEV/clerk-dev/secret-key`;
> Clerk's secret key is the backend-API token used by `@clerk/testing`. Env var bound at
> suite start: `CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')`.
> (2) **Install path split** — `install-zombie.spec.ts` was scoped as a single API-seeded
> spec, which would never exercise the real user install path. Splitting into
> `install-zombie-cli.spec.ts` (canonical: drives `zombiectl install` against the fixture
> user's `api-dev` tenant) and `install-zombie-seed.spec.ts` (API-seed sanity, consumed
> as setup by every later spec). UI-driven install is intentionally out of scope —
> the dashboard's `<FirstInstallCard>` hands the user a CLI command; there is no UI button.
> (3) **Signup-bootstrap requirement** — admin-API JWT mounting bypasses Clerk's signup
> webhook entirely. Without explicit bootstrap, the fixture user has a JWT but no tenant
> row, no default workspace, no $5 starter credit; install would succeed at the API but
> dashboard assertions about workspace and balance would fail for the wrong reason.
> The fixture pool warmer (`global-setup.ts`) MUST invoke the same bootstrap path the
> Clerk signup webhook normally runs (POST the canonical tenant-bootstrap endpoint —
> exact path to be confirmed in PLAN by greppping `src/http/handlers/`; if no admin-callable
> surface exists, that is a blocker before WS-A can land). Idempotent: safe to re-run.

> **AMENDMENT (May 11, 2026 — at CHORE(close)):** scope amendment to match what
> shipped.
>
> **Pivoted from `@clerk/testing` clerk.signIn to admin-mint cookie-direct sign-in.**
> Captain's prior spike found `clerk.signIn` (both password + ticket strategies)
> silently fails on this Clerk DEV instance because the library's FAPI route
> interceptor strips Set-Cookie headers. The harness now mints both tokens via
> Clerk's admin API (`api`-template JWT for Bearer; default session JWT for cookie),
> writes them to the fixture cache, and `signInAs(page, key)` mounts the three
> cookies clerkMiddleware needs (`__session`, `__client_uat`, `__clerk_db_jwt`).
> Documented in `docs/AUTH.md` "Test infrastructure — e2e fixture mint (admin path)".
>
> **What shipped in M64_005 (specs in this branch):**
> - WS-A.1–A.5: harness skeleton + admin-API JWT mint + Svix-signed bootstrap +
>   ticket sign-in (replaced by cookie-mount) + real-run hardening — done.
> - WS-C.1 (seed/teardown infra): seedZombie, listZombies, cleanWorkspaceZombies — done.
> - `_smoke.spec.ts` — 6/6 pass (env, JWT cache, /sign-in render, signInAs, dashboard
>   render, seed roundtrip with status=killed tolerance for the open zombied DELETE
>   bug).
> - `install-zombie-seed.spec.ts` — pass.
> - `install-zombie-cli.spec.ts` — pass (canonical install path: spawn `zombiectl install`).
> - `lifecycle.spec.ts` + `kill.spec.ts` — written, **`test.fixme`** with a clear
>   FIXME block in the describe header. KillSwitch is a client component that
>   dispatches PATCH /status via `useClientToken().getToken()`; that hook reads
>   Clerk's in-browser SDK state, which Playwright cookie-mount does not populate.
>   Two roads forward (whichever lands first), both **deferred to M64_006**:
>     1. `@clerk/testing` clerk.signIn becoming reliable on this DEV instance.
>     2. Refactor KillSwitch into a server action so the token comes from
>        `getServerToken()` instead of `useAuth`.
>   The PATCH contract itself is already covered by zombied's integration tests at
>   `src/http/handlers/zombies/*_integration_test.zig`, so the gap is only on the
>   dashboard's interactive surface.
> - `signup.spec.ts` — **`test.fixme`** (Captain's original open item). Clerk DEV
>   injects a verification step the spec does not drive. Unblockers documented
>   inline; same M64_006 cohort.
> - `getServerToken` / `useClientToken` patched to use `{template:"api"}` — bare
>   getToken returns the default session JWT (no metadata.tenant_id, no api `aud`),
>   which today happens to work locally only because zombied has `OIDC_AUDIENCE`
>   unset. The fix is in `lib/auth/{server,client}.ts` and aligns with
>   `docs/AUTH.md` "The two tokens at a glance".
> - `fix(http)`: schema-qualified `common.zig:authorizeWorkspace` queries
>   (`workspaces` → `core.workspaces`). Surfaced from the seed roundtrip 403;
>   root cause is `pool init → migration apply` ordering in
>   `make up` (with `MIGRATE_ON_START=true`). Production not affected; the
>   `pool-init-after-migrations` startup-order fix is a separate `fix(zombie)` PR.
>
> **Dropped from this milestone — moved to M64_006:**
> - `multi-zombie.spec.ts`, `multi-workspace.spec.ts`, `settings-billing.spec.ts`,
>   `events.spec.ts`, `logs-detail.spec.ts` — were planned for M64_005 but the
>   dashboard-interactive client-token gap above blocks any spec that exercises
>   a client component. M64_006 unblocks all of them together by resolving the
>   client-token issue (whichever path lands).
> - Workstream B (RadioGroup primitive + ModeRadio rebuild) — not started in this
>   branch; carries over to M64_006.
> - Workstream D (zombiectl + website coverage uplift) — not started in this
>   branch; carries over to M64_006.
> - CI plug-in (`bun run test:e2e:auth` in `deploy-dev.yml`) — deferred to
>   M64_006 because the CI wiring needs `.github/workflows/**` (which CLAUDE.md
>   gates behind explicit approval) and is best landed at the same time the
>   fixme'd specs go green so the workflow is meaningful from day one.

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 005
**Date:** May 08, 2026
**Status:** DONE
**Priority:** P1 — W3 shipped the visual surface and the unit suite proves component contracts, but no test today drives a real signed-in user from `/sign-up` through `/zombies/[id]` against `api-dev`. Until the e2e harness lands, every dashboard regression has to be caught by a human. P1 (not P0) because W3 itself is shipping with strong unit coverage; this milestone hardens the seam and closes the function-coverage gap left at the W3 cut.
**Categories:** TESTING
**Batch:** B4 — depends on M64_004 (W3) merged. Fixture pool consumes the dashboard surface as it shipped; nothing earlier is gated on it.
**Branch:** feat/m64-005-auth-e2e
**Depends on:** M64_004 DONE; `@clerk/testing` available on npm registry; the canonical Clerk DEV credentials in 1Password — `op://ZMB_CD_DEV/clerk-dev/secret-key` (backend-API token used by `@clerk/testing` to mint session JWTs) and `op://ZMB_CD_DEV/clerk-dev/webhook-secret` (Svix signing key used by the bootstrap step to mint a valid `user.created` payload signature). Both are provisioned by `playbooks/002_preflight/02_credentials.sh`. There is no separate "testing" Clerk credential; the secret key IS the backend-API token.

**Canonical architecture:** `docs/AUTH.md` — token-minting and principal flow doc; the e2e harness mounts JWTs via Clerk's admin API and never touches the `/sign-in` interactive flow except in one signup-only spec.

---

## Implementing agent — read these first

1. `docs/AUTH.md` — non-negotiable. The fixture pool and the seed routine cross the auth boundary; understand the principal contract before mocking it.
2. `docs/v2/done/M64_004_P0_UI_DESIGN_W3_APP_APPLY.md` Test Specification — the unit suite the e2e suite stacks on top of. Don't duplicate; complement.
3. `ui/packages/app/tests/e2e/` (if it exists) — current Playwright shape. If absent, mirror the website's `ui/packages/website/tests/e2e/` setup.
4. `@clerk/testing` upstream README — the `setupClerkTestingToken({ frontendApi })` helper is the spine of the harness. It mounts a session JWT for a fixture user via Clerk's backend-API admin token; no real OTP/email round-trip.
5. `bunfig.toml` (zombiectl) — the coverage-gate pattern this spec extends to the website's vitest config.
6. `coverage/lcov.info` from `ui/packages/app/`, `ui/packages/website/`, `zombiectl/` — the function-coverage gaps Workstream-D closes.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Specifically **WAUTH** (anything crossing the auth seam consults `docs/AUTH.md`), **TST-NAM** (no milestone IDs in test names or filenames), **NLR** (touch-it-fix-it on any test helper that drifts).
- `docs/AUTH.md` — doc-read gate fires on every change to `tests/e2e/fixtures/auth.ts` or any seed-route helper.
- `docs/BUN_RULES.md` — TS file-shape, const/import discipline.
- File & Function Length Gate — file ≤ 350L, fn ≤ 50L, method ≤ 70L.
- Standard set otherwise.

---

## Anti-Patterns to Avoid

N/A — spec authoring complete; the implementing agent reads sections below as goal contract, not pseudocode.

---

## Overview

**Goal (testable):** The authenticated dashboard's full lifecycle — signup → install zombie → see events → pause/stop/kill → multi-zombie pulse cap → multi-workspace switcher → settings/billing inspection — passes Playwright e2e end-to-end against `api-dev` with a fixture user pool that signs in via Clerk admin-API tokens (no real OTP/email round-trip). Suite runs from a clean fixture pool, leaves no fixture rows behind, and is gated on every PR that touches `ui/packages/app/**` or `src/http/handlers/**`.

**Problem:** W3 (M64_004) shipped the dashboard surface with a 348-test unit suite at 95.48% function coverage — strong unit contracts but no end-to-end driver. Today there is no automated path from "user clicks Sign up" to "zombie row pulses on dashboard." Every full-flow regression is caught by a human or — worse — by a customer. The polish gaps from W3 (zombiectl 91% → 95% function coverage; the missing `<RadioGroup>` design-system primitive that `ModeRadio` works around with raw `<input type="radio">`; website + zombiectl coverage scripts) sit in the same boat: known-good but unverified by automation.

**Solution summary:** Stand up Playwright fixture infrastructure (`@clerk/testing` + a fixture user pool gated to `api-dev`), write nine e2e specs covering the canonical lifecycle, close the W3 polish gaps in parallel. Bundle the lifecycle suite + polish into a single milestone PR so the next big push (W4 docs site, future M65) lands against a verified dashboard.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/package.json` | EDIT | Add `@clerk/testing` to `devDependencies`. |
| `ui/packages/app/playwright.config.ts` | EDIT | Add `globalSetup` hook; allow-list `NEXT_PUBLIC_API_URL` against `api-dev`; refuse to run against staging/prod. |
| `ui/packages/app/tests/e2e/fixtures/auth.ts` | NEW | `setupClerkTestingToken({ frontendApi })` wrapper; mounts session JWT for a fixture user via the backend-API admin token. |
| `ui/packages/app/tests/e2e/fixtures/seed.ts` | NEW | Idempotent fixture seeding against `api-dev` — creates test zombies/credentials/workspaces tagged with `x-test-fixture: true` for cleanup discrimination. |
| `ui/packages/app/tests/e2e/fixtures/teardown.ts` | NEW | Per-spec cleanup; deletes every row carrying the fixture-discrimination header. |
| `ui/packages/app/tests/e2e/global-setup.ts` | NEW | Pre-suite warm of fixture pool (2 fixture users created if missing, JWTs minted, base zombies cleared). |
| `ui/packages/app/tests/e2e/signup.spec.ts` | NEW | Real Clerk signup with the `+clerk_test` alias on `mailinator.com` (e.g. `signup-fixture+clerk_test@mailinator.com`). Mailinator is a public no-auth burner inbox — no real delivery happens because Clerk short-circuits OTP on the `+clerk_test` alias when the DEV instance has test mode enabled. Land on dashboard → assert zero-state visible + balance shows $5. The only spec that exercises the interactive sign-in flow. (A dedicated test-domain like `signup@test.usezombie.com` is a future polish, not a blocker.) |
| `ui/packages/app/tests/e2e/install-zombie-cli.spec.ts` | NEW (canonical) | Spawn `zombiectl install` via Playwright child-process API against `api-dev`, with the fixture user's session JWT bound to `ZOMBIECTL_TOKEN`. Within N seconds, reload `/zombies` → assert row appears with `data-state="live"` + animated `<WakePulse live>`. This is the real user install path; UI-driven install is intentionally absent (the dashboard hands users a CLI command, not a button). |
| `ui/packages/app/tests/e2e/install-zombie-seed.spec.ts` | NEW (sanity) | Signed-in fixture user POSTs `/v1/workspaces/{ws}/zombies` via API helper → reload `/zombies` → assert row appears. Validates the seed helper itself; consumed as setup by `lifecycle`, `kill`, `multi-zombie`, `multi-workspace`, `events`, `logs-detail`. |
| `ui/packages/app/tests/e2e/lifecycle.spec.ts` | NEW | Stop button → ConfirmDialog → confirm → row dot turns muted (`data-state="parked"`) → dashboard stat row updates Live count. |
| `ui/packages/app/tests/e2e/kill.spec.ts` | NEW | Kill button → ConfirmDialog → confirm → row removed from list. |
| `ui/packages/app/tests/e2e/multi-zombie.spec.ts` | NEW | Seed 6 active zombies → assert exactly 5 animate (`data-live`) + 1 static glow + header reads "6 live" with single brand-pulse — the consolidation cap. |
| `ui/packages/app/tests/e2e/multi-workspace.spec.ts` | NEW | Seed zombies in 2 workspaces → WorkspaceSwitcher click → assert different zombie list rendered → URL stays at `/zombies` (workspace cookie change, not route). |
| `ui/packages/app/tests/e2e/settings-billing.spec.ts` | NEW | Navigate `/settings/billing` → assert balance renders with `tabular-nums`. (Plan-tier badge dropped — see May 09 amendment.) |
| `ui/packages/app/tests/e2e/events.spec.ts` | NEW | Trigger a manual webhook against `/v1/webhooks/{zombie_id}` (no-op fixture handler) → poll `/events` → assert event row appears with correct timestamp + zombie name. |
| `ui/packages/app/tests/e2e/logs-detail.spec.ts` | NEW | Open `/zombies/[id]` → click event row → assert `<Dialog>` opens with payload preview; assert `<WakePulse>` on active-state header still pulses. |
| `ui/packages/design-system/src/design-system/RadioGroup.tsx` | NEW (Workstream-B) | Replace `ModeRadio.tsx`'s raw `<input type="radio">` — the only raw HTML primitive remaining in app source. Wraps Radix `RadioGroup`/`RadioGroupItem` with the design-system focus-ring + token map. |
| `ui/packages/design-system/src/design-system/RadioGroup.test.tsx` | NEW | Primitive contract tests — keyboard arrow nav, `data-state` attribute, controlled vs uncontrolled. |
| `ui/packages/design-system/src/index.ts` | EDIT | Export `RadioGroup` + `RadioGroupItem` + types. |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ModeRadio.tsx` | REBUILD | Consume the new `<RadioGroup>` primitive; drop raw `<input type="radio">`. |
| `ui/packages/website/vite.config.ts` | EDIT (Workstream-D) | Coverage thresholds locked at 95% (already lifted in W3 cut; this spec verifies and prevents regression). |
| `zombiectl/test/cli-dispatch.unit.test.js` | NEW (Workstream-D) | Cover `cli.js` route-dispatch arrow handlers — currently 50% function coverage. Targets: every `route.key` entry in the `handlers` map invoked once with mock deps. |
| `zombiectl/test/zombie-steer-fallback.unit.test.js` | NEW (Workstream-D) | Cover `zombie_steer.js` poll-fallback + `eventIdToSince` + `isTerminal` + `buildBearer` — currently 60% function coverage. |
| `zombiectl/test/workspace-helpers.unit.test.js` | NEW (Workstream-D) | Cover `workspace.js` lines 89-92, 118-119, 123-124, 199-200 (the four uncovered branches). |
| `zombiectl/bunfig.toml` | EDIT (Workstream-D) | Lift `coverageThreshold` to `{ line = 0.95, function = 0.95 }` once Workstream-D writes land. |

---

## Workstreams

### Workstream A — `@clerk/testing` integration

The spine of every authenticated spec. `tests/e2e/fixtures/auth.ts` exports a `signInAs(page, fixtureUserKey)` helper that mounts the session JWT before the page navigates. No interactive sign-in except in `signup.spec.ts`.

**Invariant:** environment selection is the workflow's responsibility, not the test code's. The harness fails fast at `globalSetup` if any of `NEXT_PUBLIC_API_URL`, `CLERK_SECRET_KEY`, or `CLERK_WEBHOOK_SECRET` is missing, with a copy-paste op-read recipe in the error body. The auth e2e suite is invoked only by `deploy-dev.yml` (and, in future, any other workflow that explicitly opts in by setting these env vars); existing project workflows already scope target URLs per environment (`deploy-dev.yml` only references `api-dev`; `release.yml` gates prod URLs behind dev-stage success). No runtime allow-list — relying on workflow + Vercel env scoping for the fixture-write/prod boundary.

### Workstream B — fixture user pool (`api-dev` only)

Two fixture users, each owning their own tenant:
- `regular@usezombie.test` — single default workspace, owner role.
- `admin@usezombie.test` — owns their own tenant; also a member (role=admin) of `regular`'s tenant. Drives the multi-workspace switcher spec.

JWTs minted at suite start via Clerk's backend API using the canonical Clerk DEV secret key (env: `CLERK_SECRET_KEY`, sourced from `op://ZMB_CD_DEV/clerk-dev/secret-key`). Per-spec cleanup hook deletes every zombie/credential/event row tagged with the `x-test-fixture: true` header.

**Signup-bootstrap step (mandatory).** Admin-API JWT mounting bypasses Clerk's signup webhook, so the fixture user has a JWT but no tenant row by default. After mint, `global-setup.ts` POSTs `/v1/webhooks/clerk` with a Svix-signed `user.created` payload — the same path Clerk hits in production. The Svix signature is computed locally using `op://ZMB_CD_DEV/clerk-dev/webhook-secret`; the handler at `src/http/handlers/webhooks/clerk.zig` verifies the signature and writes the tenant row, default workspace, and $5 starter credit. The integration test at `src/http/handlers/webhooks/clerk_integration_test.zig:119` is the reference shape for the payload + headers. Idempotent — replaying the same `user.created` returns `created:false` with no new rows (see `clerk_integration_test.zig:348`). Without this step, `install-zombie-cli` succeeds at the API auth layer but the dashboard renders no workspace or balance, producing misleading red tests.

### Workstream C — e2e specs (the nine in Files Changed)

Each spec is self-contained; each spec seeds its own state via the API helpers (no DOM-driven setup), drives the dashboard via Playwright, asserts on `data-*` attributes and accessible names (no class selectors).

### Workstream D — coverage uplift carry-over from W3

W3 set the floor at 93% for zombiectl (organic baseline post-test additions). This workstream:
1. Writes the four targeted test files above to lift `cli.js` 50→95%, `zombie_steer.js` 60→95%, `workspace.js` 78→95%, `sse.js` 60→95%.
2. Lifts `bunfig.toml` `coverageThreshold` to 95% line + 95% function once those tests pass.
3. Verifies website at 98%+ function coverage (already there; this is a regression guard, not a write task).

---

## Failure Modes & Invariants

| Mode | What goes wrong | How the harness catches it |
|------|-----------------|----------------------------|
| Fixture pollution | Test rows leak into the `api-dev` zombie list and contaminate manual QA | `x-test-fixture: true` discrimination header + per-spec teardown that deletes by header |
| Wrong-environment seed | Suite runs against staging/prod and creates test zombies in real tenants | `globalSetup` throws if `NEXT_PUBLIC_API_URL` doesn't exact-match the `api-dev` allow-list |
| Pulse-cap regression | A future change lifts the 5-simultaneous cap and the dashboard re-introduces the rave-mode anti-pattern | `multi-zombie.spec.ts` asserts exactly 5 `data-live="true"` rows and one static glow when 6 zombies are live |
| Auth seam drift | Token-minting changes silently break the fixture path | The `signInAs` helper re-mints on every spec; failure surfaces as a top-of-suite error, not a deep-page render failure |
| Coverage regression | A future PR drops a test file and the coverage drops back below the threshold | `bunfig.toml` `coverageThreshold` gates every CI run (zombiectl); `vite.config.ts` thresholds gate website + app |

**Architectural invariant:** every fixture row carries the `x-test-fixture: true` header. No production code reads this header — it exists solely so the cleanup routine can discriminate. The header is set by the seed helper; production handlers ignore it.

---

## Test Specification

| Test | Asserts |
|------|---------|
| signup.spec.ts | Real Clerk signup with `signup-fixture+clerk_test@mailinator.com` (or equivalent mailinator alias) → dashboard → zero-state `<FirstInstallCard>` visible → balance shows $5 |
| install-zombie-cli.spec.ts | `zombiectl install` spawned with fixture user's token → `/zombies` reload → row visible with `data-state="live"` + `<WakePulse live>` |
| install-zombie-seed.spec.ts | API-driven create (seed-helper sanity) → `/zombies` reload → row visible |
| lifecycle.spec.ts | Stop → ConfirmDialog confirm → `data-state="parked"` → dashboard stat-row Live count decremented by 1 |
| kill.spec.ts | Kill → ConfirmDialog confirm → row removed from `/zombies` list |
| multi-zombie.spec.ts | 6 active zombies seeded → exactly 5 `data-live="true"` rows + 1 static glow + header "6 live" with single brand-pulse |
| multi-workspace.spec.ts | Two workspaces with different zombies → switcher click → list re-renders → URL stays `/zombies` |
| settings-billing.spec.ts | `/settings/billing` → balance renders with `tabular-nums` |
| events.spec.ts | Manual webhook against `/v1/webhooks/{zombie_id}` → `/events` poll → row appears with correct timestamp + zombie name |
| logs-detail.spec.ts | `/zombies/[id]` event-row click → `<Dialog>` opens with payload → header `<WakePulse>` still pulses |
| RadioGroup.test.tsx (B) | Keyboard arrow nav cycles items, `data-state="checked"` on active item, controlled + uncontrolled variants |
| zombiectl cli-dispatch.unit | Every `handlers[route.key]` arrow callable with mock deps; route → handler arity guard |
| zombiectl zombie-steer-fallback | `eventIdToSince`, `isTerminal`, `buildBearer` cover all branches; `pollEventTerminal` covers timeout + match + error paths |
| zombiectl workspace-helpers | The 4 uncovered branches (lines 89-92, 118-119, 123-124, 199-200) |

---

## Acceptance Criteria

- `bun run test:e2e` (in `ui/packages/app`) passes locally against `api-dev` from a clean fixture pool.
- Same suite passes in CI (new `test-app-e2e` job in `.github/workflows/test.yml`, gated to PRs touching `ui/packages/app/**` or `src/http/handlers/**`).
- `bun test --coverage` (zombiectl) reports ≥95% function and ≥95% line; `bunfig.toml` threshold updated to match.
- `bun run test:coverage` (website) reports ≥95% function; `vite.config.ts` threshold confirmed.
- `bun run test:coverage` (app) still reports ≥95% function (no regression from W3's 95.48%).
- `<RadioGroup>` primitive ships in `@usezombie/design-system`; `ModeRadio` consumes it; no raw `<input type="radio">` left in `ui/packages/app/**`.
- Fixture cleanup runs every suite — `api-dev` zombie list is verifiably empty of test rows post-suite.
- Fixture pool warmer (`global-setup.ts`) invokes the tenant-bootstrap endpoint after JWT mint; every fixture user has a tenant + default workspace + $5 starter credit before any spec runs.

---

## Out of Scope

- Stripe purchase flow tests (deferred to v2.1; existing comment in `BillingBalanceCard` stands).
- Mobile e2e at 375px (Playwright mobile emulation works but adds suite time; defer to a perf milestone).
- Visual regression screenshots (Percy / Chromatic) — separate concern.
- Welcome email on signup — picked up by M64_006 (cross-repo) or M65 marketing-ops.

---

## Discovery (out-of-scope but adjacent observations the agent SHOULD surface)

- W3's coverage-fill test (`tests/loading-states.test.ts`) renders six `loading.tsx` files. If any of those skeleton components grows beyond simple `<Skeleton>` chrome, the test should grow assertions to match — not stay a smoke test.
- The fixture-discrimination header pattern (`x-test-fixture: true`) is reusable. If future test layers (load tests, soak tests) need similar discrimination, extract to `docs/architecture/test-fixture-discrimination.md`.
- The `setActiveWorkspace` server action is exercised by `multi-workspace.spec.ts` end-to-end for the first time. If the spec uncovers a race in cookie-set vs page-render, that's a real bug — file it, don't paper over.

---

## Implementation Notes

- The `@clerk/testing` integration consumes Clerk's DEV secret key as `CLERK_SECRET_KEY`. Resolve at suite start via `op read 'op://ZMB_CD_DEV/clerk-dev/secret-key'`; never paste inline. Same vault item is consumed by `playbooks/012_usezombie_admin_bootstrap` (lines 119–125), which mints session JWTs through Clerk's backend API the same way — pattern is already established.
- The bootstrap step in `global-setup.ts` POSTs `/v1/webhooks/clerk` with a Svix-signed `user.created` payload. Compute the signature locally using `op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret'`; never paste inline. The reference payload + headers are in `src/http/handlers/webhooks/clerk_integration_test.zig:82` (happy-path) and `:348` (replay idempotency).
- Clerk DEV instance must have **test mode** enabled in the dashboard for `+clerk_test` aliases to bypass OTP in `signup.spec.ts`. Verify in PLAN before WS-C lands the spec.
- `globalSetup` should warm the fixture pool but NOT clear it across runs — the per-spec teardown handles row-level cleanup. Pool warming is idempotent.
- The `cli.js` dispatch coverage gap is an architectural smell, not just a test gap. If the implementing agent finds a clean refactor (split route table into a registry module) while writing the test, that's preferred over force-covering the existing arrow handlers.
