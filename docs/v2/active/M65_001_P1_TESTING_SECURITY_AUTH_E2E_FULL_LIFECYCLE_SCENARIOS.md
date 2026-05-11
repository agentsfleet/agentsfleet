<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M65_001: Authenticated e2e — full lifecycle scenarios + vulnerability audit

**Prototype:** v2.0.0
**Milestone:** M65
**Workstream:** 001
**Date:** May 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — The auth harness (M64_005/006) ships eight specs covering individual lifecycle operations against a pre-seeded fixture, but no spec walks a real-user flow end-to-end (signup → install → observe → bill → halt). Captain wants two such flows on every Vercel `usezombie-app` Production deploy and against `api-dev`. The same audit pass surfaces and prices every hardening item the existing harness carries forward.
**Categories:** TESTING, SECURITY
**Batch:** B1 — no parallel workstreams in M65
**Branch:** `feat/m65-001-auth-e2e-lifecycle-scenarios`
**Depends on:** M64_006 (auth harness + post-deploy CI gates); **hard merge-gate:** vault items `op://ZMB_CD_DEV/e2e-fixtures-email/{regular,admin}` and `op://ZMB_CD_PROD/e2e-fixtures-email/{regular,admin}` AND wires the workflow `env:` blocks to consume them. The implementation PR MUST NOT merge while either fixture email still resolves to a `*@mailinator.com` default in CI — the Acceptance Criteria checkbox below makes this machine-verifiable.

**Canonical architecture:** `docs/AUTH.md` §"Test infrastructure — e2e fixture mint (admin path)" + §"PROD fixture identity carve-out".

---

## Implementing agent — read these first

1. `docs/AUTH.md` — token model, harness chain, "PROD fixture identity carve-out" section. The vulnerability table below depends on the carve-out invariants holding; if the harness changes the mint path the carve-out must move with it.
2. `docs/v2/done/M64_006_P1_TESTING_AUTH_E2E_CONTINUATION_AND_W3_CARRY_OVER.md` — most recent fixture-harness milestone, especially its Discovery section (events deferral, EventDetail dialog deferral, cross-tenant admin deferral). Several items here graduate into scenarios in this spec.
3. `ui/packages/app/tests/e2e/auth/fixtures/clerk-admin.ts` — `provisionUser`/`bootstrapTenant`/`attachJwt` 3-phase chain. Any change to the password-hardening posture (WS-A finding) lands here.
4. `ui/packages/app/tests/e2e/auth/global-setup.ts` — fixture identity resolution + JWT cache write. `freshPassword()`, `is_test_fixture` metadata, and the random-per-create posture all live here.
5. `ui/packages/app/tests/e2e/auth/install-zombie-cli.spec.ts` and `install-zombie-seed.spec.ts` — reference for the install path. The new lifecycle scenarios deliberately do NOT drive the CLI (overlaps `install-zombie-cli.spec.ts`) nor the power-user paste form. Both seed via the API through a new `seedPlatformOpsZombie` helper that reads the same `samples/platform-ops/` bundle the CLI consumes. Rationale lives in WS-C step 6.
6. `ui/packages/app/app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` — Stop/Resume/Kill state machine UI. Selector inventory for the new scenarios lives there (status `active` → Stop+Kill, `paused`/`stopped` → Resume+Kill, `killed`/`errored` → terminal disabled "Killed" indicator).
7. `ui/packages/app/app/(dashboard)/zombies/components/ZombiesList.tsx:liveStateOf` — canonical mapping from zombied status to dashboard `data-state` (`active→live`, `killed|errored→failed`, everything else→`parked`). New assertions key on this attribute.
8. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — the canonical bundle the new `seedPlatformOpsZombie` helper reads and POSTs to `/v1/workspaces/{ws}/zombies`. Same bundle `zombiectl install --from` would feed, byte-for-byte. M37_001 is the canonical spec for this skill.
9. `.github/workflows/deploy-dev.yml` (`auth-e2e-dev` job) and `.github/workflows/smoke-post-deploy.yml` (`auth-e2e-prod` job) — the deployment gates the new specs will plug into. Both already wire op:// secrets, Playwright browser cache, and artifact upload of `playwright-auth-report/` only.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Especially relevant: RULE TST-NAM (no milestone IDs in test names), RULE UFS (centralise repeat literals), RULE TGU (test-guard), RULE WAUTH (webhook-auth shape).
- `docs/BUN_RULES.md` — diff is JS/TS heavy; TS FILE SHAPE DECISION applies to any new fixture file.
- `docs/REST_API_DESIGN_GUIDELINES.md` — N/A; this spec adds NO new HTTP handlers. Existing handlers are exercised through the existing fixture client.
- `docs/AUTH.md` — load-bearing. Any change to the fixture provisioning chain (password disable, vault-resolved email, separate webhook secret) requires the carve-out section to move with it in the same commit.
- `docs/ZIG_RULES.md` — N/A unless WS-B item #3 (separate webhook test secret) graduates into the implementation PR, which it should not on this milestone — it is recorded as a deferred backend change.

---

## Anti-Patterns to Avoid (read this BEFORE drafting the spec)

Standard set from `docs/TEMPLATE.md` applies. Additionally for this spec:

- Do NOT inline Playwright assertion code in section bodies. The spec names selectors and behavioral claims; the implementing agent writes `await expect(...)` themselves.
- Do NOT propose teardown work. Captain has explicitly deferred fixture teardown design until the first PROD run has accumulated observable state (M64_006 Discovery).
- Do NOT propose adding a third CI job. PR-time gating is recorded as an open question, not a deliverable.

---

## Overview

**Goal (testable):** Two Playwright specs (`signup-platformops-lifecycle.spec.ts`, `login-existing-zombie-lifecycle.spec.ts`) run inside the existing `tests/e2e/auth/` suite, gate `auth-e2e-dev` on every `main` deploy, and gate `auth-e2e-prod` on every Vercel `usezombie-app` Production deploy. Each spec walks a full operator flow — signup OR existing-fixture login → land on the dashboard → install or observe a `platform-ops` zombie → see live events → settings/billing → Stop → Resume → Kill — and the vulnerability audit table is dispositioned: each row has either a same-PR fix or an explicit accepted-risk note with reactivation conditions.

**Problem:**

1. The existing harness has eight specs that each exercise one slice (install, lifecycle stop, kill, events, logs, multi-zombie, multi-workspace, settings-billing). No spec walks a real user from "I just signed up" to "I just killed a zombie I observed running." Coverage gaps live between the slices: e.g. the dashboard route-guard chain that runs only when a brand-new tenant first lands.
2. The PROD harness creates fixture identities in Clerk PROD with `password_enabled: true` on a public mailinator inbox. WS-A in this spec resolves whether the proposed "disable password" hardening is viable; the parallel handoff resolves the mailinator side.
3. Captain wants both flows on PROD because the dashboard's first-deploy regressions historically hit signup-derived state (workspace auto-provision, starter credit, empty-state render), not pre-seeded-fixture state.

**Solution summary:** Two new lifecycle specs added under `ui/packages/app/tests/e2e/auth/` reusing the existing fixture chain (`signInAs`, `seedZombie`, `cleanWorkspaceZombies`, `clientFor`). Scenario 1 creates a per-test ephemeral Clerk user (DEV only — skipped on PROD until vault-resolved private aliases exist OR Clerk PROD test mode is enabled). Scenario 2 reuses the persistent `regular` fixture and a freshly-seeded `platform-ops` zombie. Both walk the same observation + lifecycle leg. The vulnerability audit (WS-B) is dispositioned in the spec body; only rows whose fix is in scope for the implementation PR move into Files Changed.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/tests/e2e/auth/signup-platformops-lifecycle.spec.ts` | CREATE | Scenario 1 — ephemeral signup → install → observe → bill → halt. DEV-only via `test.skip(isProdApi)` mirror of `signup.spec.ts`. |
| `ui/packages/app/tests/e2e/auth/login-existing-zombie-lifecycle.spec.ts` | CREATE | Scenario 2 — persistent fixture login → seeded zombie → observe → bill → halt. Runs on both DEV and PROD. |
| `ui/packages/app/tests/e2e/auth/fixtures/seed.ts` | EDIT | Add `seedPlatformOpsZombie(auth, ws)` helper that reads `samples/platform-ops/{SKILL,TRIGGER}.md` from disk and seeds via the API. Widen `getDefaultWorkspaceId` + `seedPlatformOpsZombie` to accept `AuthHandle = { key: FixtureKey } \| { sessionJwt: string }` so Scenario 1's mid-test-minted JWT can drive the seed (the ephemeral signup user is NOT in `.fixture-jwts.json`). |
| `ui/packages/app/tests/e2e/auth/fixtures/api-client.ts` | EDIT | Add `clientForJwt(sessionJwt)` thin constructor that reuses the existing request implementation. Routes the `{sessionJwt}` variant of `AuthHandle` without duplicating fetch/error logic. Existing `clientFor(key)` remains as the cache-backed default for persistent fixtures. |
| `ui/packages/app/tests/e2e/auth/{_smoke,lifecycle,kill,multi-zombie,multi-workspace,settings-billing,events,logs-detail,install-zombie-seed,install-zombie-cli}.spec.ts` | EDIT | Migrate `getDefaultWorkspaceId(FIXTURE_KEY.regular)` call sites to `getDefaultWorkspaceId({ key: FIXTURE_KEY.regular })` in the same commit as the signature widening. RULE NLG — no parallel signature, no overload, no compat shim. |
| `ui/packages/app/tests/e2e/auth/fixtures/lifecycle.ts` | CREATE | Shared selectors + action helpers: `stopZombie(page, id)`, `resumeZombie(page, id)`, `killZombie(page, id)`. Pulls the duplicated KillSwitch + ConfirmDialog wiring out of `lifecycle.spec.ts`/`kill.spec.ts` and the new scenarios. Eliminates the row of literal-duplicates the existing two specs have today (RULE UFS). |
| `ui/packages/app/tests/e2e/auth/fixtures/_jwt-cache-location.test.ts` | CREATE | Vitest regression for WS-B #4 — asserts `.fixture-jwts.json` path is outside `playwright-auth-results/` and `playwright-auth-report/`. Runs in `make test`. |
| `ui/packages/app/tests/e2e/auth/_smoke.spec.ts` | EDIT | WS-B #8 + #9 assertions: (a) resolved `@clerk/nextjs` major equals the pinned constant in `fixtures/constants.ts`; (b) on PROD only, `__clerk_db_jwt` cookie value parses as a real 3-segment JWT; (c) `freshPassword()` output length ≥ 16 chars (regression for Clerk password-policy tightening). |
| `ui/packages/app/package.json` | EDIT | WS-B #8 — pin `@clerk/nextjs` major (caret pin against the current installed major); record the pinned major as a constant in `fixtures/constants.ts` so the smoke assertion has a single source of truth. |
| `ui/packages/app/tests/e2e/auth/fixtures/constants.ts` | EDIT | Add `CLERK_NEXTJS_PINNED_MAJOR` constant consumed by the WS-B #8 smoke assertion (RULE UFS — one literal, two readers). |
| `ui/packages/app/tests/e2e/auth/fixtures/clerk-admin.ts` | EDIT | WS-B #11 — tighten `mintTokens` `expires_in_seconds` from `3600` to a TTL that is 2× the observed p95 suite wall-clock (implementing agent reads most-recent CI timing). Add a `clientFor` expiry-guard that re-mints on detected expiry. |
| `docs/AUTH.md` | EDIT | Update "PROD fixture identity carve-out" with the WS-A finding (PATCH `password_enabled:false` is a silent no-op on Clerk admin API; harness retains random-per-create password posture). Append "Known gaps" subsection enumerating accepted vulnerabilities + the two Clerk PROD fixture user IDs with the `is_test_fixture` metadata filter query (WS-B #7). |

**Files NOT changed (explicit non-goals on this milestone):**

- `.github/workflows/**` — different agent's territory. The new specs auto-run because Playwright globs `tests/e2e/auth/*.spec.ts`.
- `ui/packages/app/tests/e2e/auth/signup.spec.ts` — keep as-is; Scenario 1 is purely additive.
- `src/http/handlers/webhooks/clerk.zig` — separate-webhook-test-secret hardening (WS-B #3) is a deferred backend milestone, recorded in Discovery.
- `ui/packages/app/tests/e2e/auth/global-teardown.ts` — fixture teardown deferred per Captain (M64_006).

---

## Workstreams

### WS-A — Password-disable viability (live DEV verification)

**Result already in hand** (live experiment run as part of this spec's authoring against `regular-fixture@mailinator.com` in Clerk DEV, May 11, 2026):

1. `PATCH /v1/users/{id}` with body `{"password_enabled": false}` → 200, response body returns `password_enabled: true`, no error. Clerk silently ignores the field.
2. The harness mint chain (`POST /v1/sessions` → `POST /v1/sessions/{id}/tokens/api` → `POST /v1/sessions/{id}/tokens`) was attempted on the same user with the user still password-enabled (because step 1 no-op'd). All three calls returned 200 with valid JWTs. The harness path itself is unaffected.
3. PATCH was reverted (no-op was already a no-op, idempotent).

**Conclusion:** the originally-proposed `disablePassword()` PATCH path is NOT viable. Clerk's Backend API does not expose `password_enabled` as a writable field on `PATCH /v1/users/{id}`. Whether some other endpoint (e.g. `DELETE /v1/users/{id}/password`, or an instance-level "password is optional" config flag combined with password removal at user-create time) achieves the same outcome is the **open follow-up question** for the implementation PR — but it is no longer blocking, because the more important vulnerability fix (private-domain email via `AUTH_E2E_*_EMAIL`) is already in flight and removes the public-mailinator attack surface that motivated password-disable in the first place.

**Implementation default:** treat password-disable as a research item, not a deliverable, on the implementation PR. If the implementing agent finds a working endpoint, add it to `clerk-admin.ts` as a fourth phase between `bootstrapTenant` and `attachJwt`; otherwise leave the random-per-create posture in place and record the finding in `docs/AUTH.md`.

**Revert command (idempotent, on the DEV fixture user):** the fixture user ID is not pinned in this spec — resolve at runtime from the email so a future fixture rotation does not invalidate the runbook.

```bash
CS=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')
EMAIL=$(op read 'op://ZMB_CD_DEV/e2e-fixtures-email/regular' 2>/dev/null || echo 'regular-fixture@mailinator.com')
CLERK_UID=$(curl -sS -H "Authorization: Bearer $CS" \
  "https://api.clerk.com/v1/users?email_address=$EMAIL" | jq -r '.[0].id')
curl -sS -X PATCH -H "Authorization: Bearer $CS" -H 'Content-Type: application/json' \
  -d '{"password_enabled": true}' "https://api.clerk.com/v1/users/$CLERK_UID" | jq '{password_enabled}'
```

(Run a second time it is still safe — Clerk treats the field as read-only on PATCH and returns the existing value.)

### WS-B — Vulnerability audit

Each row carries severity, current state, proposed fix, where the fix lands. "Severity" is informal (S0 = security incident risk, S1 = exploitable but contained, S2 = posture hardening, S3 = housekeeping). "Disposition" is one of `FIX_THIS_PR`, `ACCEPTED_RISK`, `DEFERRED_TO_<milestone>`, `BLOCKED_ON_<dep>`.

| # | Vulnerability | Sev | Current state | Proposed fix | Lands in | Disposition |
|---|---|---|---|---|---|---|
| 1 | Public mailinator inbox for fixture identity → anyone with the email can request a Clerk password-reset link and claim the account. | S1 | Vault items provisioned at `op://ZMB_CD_DEV/e2e-fixtures-email/{regular,admin}` (DEV) and `op://ZMB_CD_PROD/e2e-fixtures-email/{regular,admin}` (PROD); workflow `env:` wiring TBD. `AUTH_E2E_{REGULAR,ADMIN}_EMAIL` env override is read in `global-setup.ts`. | Flip the workflow `env:` blocks to resolve the vault items. CI job overrides defaults. Local DEV runs keep mailinator (accepted risk: local-only). | Parallel handoff (separate agent — workflows out of scope this milestone) | `BLOCKED_ON_workflow-wiring` — implementation PR may NOT merge until the workflow `env:` blocks consume the vault items. |
| 2 | Persistent fixture user has `password_enabled: true`; even with private email (#1), a Clerk hosted sign-in form could be driven by anyone who learns the email + password. | S2 | `freshPassword()` generates 256-bit random per `provisionUser` call and never persists. Compromise requires both the random password (only in process memory during one suite run) AND access to the user-password sign-in flow. | WS-A shows PATCH path is not viable. Implementation PR researches `DELETE /v1/users/{id}/password` or instance-level "passwordless required" config. If neither lands cheaply, accept the current posture. | `clerk-admin.ts` (only if viable endpoint found) | `ACCEPTED_RISK` for this milestone unless a viable endpoint is discovered during implementation. Captain's prior P0 ranking is downgraded based on WS-A. |
| 3 | `CLERK_WEBHOOK_SECRET` reuse — the harness uses the **production webhook secret** to Svix-sign synthetic `user.created` posts. Anyone with that secret can forge any Clerk webhook against zombied PROD. | S1 | The secret is op://-resolved per environment; the PROD-secret blast radius is bounded by 1Password access, not by the harness. But the harness adds a NEW party who needs the PROD-trust secret (CI service account) — and the secret cannot be rotated without coordinating with the test harness. | Add a backend feature: zombied accepts EITHER `CLERK_WEBHOOK_SECRET` OR a separate `CLERK_WEBHOOK_TEST_SECRET` whose tenant rows are flagged `is_test_fixture: true` and barred from billing real money. Harness in CI uses only the test secret. | New backend milestone (NOT this PR) | `DEFERRED_TO_backend-milestone` — recorded in Discovery for prioritisation. |
| 4 | `.fixture-jwts.json` (cookieJwt valid ~1h, sessionJwt carries tenant claims) could ride along in an artifact upload. | S2 | File written at `ui/packages/app/.fixture-jwts.json` mode 0600, gitignored at `.gitignore:19`. CI artifact uploads target `ui/packages/app/playwright-auth-report/` (single subdirectory), not the package root. Playwright's `outputDir` is `playwright-auth-results`. The cache file is in NEITHER subdirectory. | Add a one-line guard test that asserts the file path does not match either Playwright-managed directory — catches a future refactor that moves `outputDir` or the cache. | `tests/e2e/auth/fixtures/_jwt-cache-location.test.ts` (vitest, not Playwright — runs in `make test`) | `FIX_THIS_PR` — cheap regression-proof. |
| 5 | Tenant pollution in PROD `core.tenants` — fixture tenants accumulate `tenant_billing.balance_nanos` and any state created by tests forever. | S3 | Per-spec teardown deletes zombies; tenant row reused. No teardown of tenant or billing balance. | None on this milestone — Captain explicitly deferred until the first PROD run accumulates observable state. | N/A | `ACCEPTED_RISK` with reactivation condition: revisit after one calendar quarter of PROD runs OR if `tenant_billing.balance_nanos` for either fixture tenant rises above a threshold the ops dashboard should set. |
| 6 | No PR-time `auth-e2e` gate — the suite only fires post-merge to `main` and post-deploy to PROD. A breaking auth-flow change lands on `main` before catching. | S3 | `qa.yml` runs unauthenticated smoke on every PR; auth suite is post-merge. | Add a PR-time job; estimate adds a few minutes per PR. | `.github/workflows/qa.yml` (different agent's territory per Captain's constraints) | `DEFERRED` — open question for Captain. This spec surfaces but does not propose. |
| 7 | First-PROD-deploy provisioned two Clerk PROD identities with no warning gate or operator review. | S2 | Mitigated post-hoc by commit f86d0c35 — identities are tagged `public_metadata.is_test_fixture: true`, passwords are random and unpersisted. Verifying what Clerk PROD ops dashboards show after first run is part of this spec's acceptance criteria. | Document the expected PROD-Clerk users list in `docs/AUTH.md` "PROD fixture identity carve-out" with the actual Clerk user IDs and the metadata tag query that filters them. | `docs/AUTH.md` | `FIX_THIS_PR` (docs-only) — Captain inspects PROD Clerk dashboard, agent records the two user IDs + metadata filter query in AUTH.md. |
| 8 | `__clerk_db_jwt = "fixture-dev-browser"` literal in `auth.ts:91`. Today clerkMiddleware reads this cookie truthy-only with no signature verification. A future `@clerk/nextjs` hardening (real dev-browser-token requirement) would break the harness AND, more importantly, means the harness is not exercising whatever validation Clerk PROD eventually adds. | S2 | M64_006 notes this and pins to fail-fast in test 5 (`_smoke.spec.ts`). `package.json` does NOT currently pin `@clerk/nextjs` major. | Pin `@clerk/nextjs` major version in `ui/packages/app/package.json`; add a `_smoke` assertion that asserts the cookie value is a JWT (3 `.`-segments) when running against PROD — fails fast if clerkMiddleware ever enforces it on PROD without enforcing it on DEV. | `ui/packages/app/package.json`, `_smoke.spec.ts` | `FIX_THIS_PR`. |
| 9 | `freshPassword()` uses `crypto.randomBytes(32).toString("base64url")` — 256 bits entropy. Clerk's password policy MAY reject characters outside its acceptable set (uppercase + lowercase + digit + symbol class checks). If policy tightens, fixture provisioning fails opaquely with "password rejected" — and the workflow has no clear remediation path. | S3 | Today the random base64url string clears Clerk's permissive defaults. No regression test. | Add `_smoke.spec.ts` assertion that `freshPassword()` output matches Clerk's documented allowable-chars constraints (or that a `provisionUser` round-trip on the regular fixture succeeds on every run). The latter already runs as part of `globalSetup` — `_smoke` re-validates the cache. | `_smoke.spec.ts` | `FIX_THIS_PR` (one assertion). |
| 10 | `bootstrapTenant` re-Svix-signs the same `msg_e2e_bootstrap_…` UUID on each run but a fresh timestamp. Webhook handler dedupes on msg id — but the harness mints a fresh msg id with `newMsgId("msg_e2e_bootstrap")` per call, so back-to-back runs are not deduped. If `globalSetup` runs twice rapidly (e.g. retry), zombied processes both as new events. | S3 | Idempotent at the zombied data layer (`user.created` for an existing Clerk user returns `created:false`), so no broken state. But the wire is honest-but-wasteful. | None — accepted. The current shape mirrors Clerk's own retry behavior (each retry is a new Svix msg id), and the data-layer idempotency is the load-bearing guarantee. | N/A | `ACCEPTED_RISK`. |
| 11 | The Bearer JWT minted with `expires_in_seconds: 3600` (one hour). For a suite that runs in single-digit minutes, that's a wider valid window than needed. If `.fixture-jwts.json` leaked at minute 6, the attacker has 54 minutes of valid token. | S3 | Default Clerk token TTL is 60s — explicitly raised to 1h because the suite was OOMing on default. | Tighten to `expires_in_seconds: 900` (15 min) once the suite consistently completes under that wall clock (M64_006 reports ~5min on CI). Re-mint on detected expiry inside `clientFor` for safety. | `clerk-admin.ts:mintTokens` | `FIX_THIS_PR` — gated on observation: implementation PR must check most-recent CI run timings and choose a TTL that is 2× the observed p95. |

Items #4, #7, #8, #9, #11 are `FIX_THIS_PR`. Everything else is dispositioned without code change in this milestone.

### WS-C — Scenario 1: signup → workspace → install platform-ops → observe → bill → halt

**File:** `ui/packages/app/tests/e2e/auth/signup-platformops-lifecycle.spec.ts`

**Runs against:** DEV + local. PROD is skipped (`test.skip(isProdApi, …)`) until either (a) Clerk PROD has test mode enabled OR (b) a vault-resolved private-domain alias replaces the `+clerk_test@mailinator.com` pattern. Both are tracked in Discovery.

**Fixture-state model:** per-test ephemeral. New unique `+clerk_test@mailinator.com` email per run; deleted in `test.afterEach` via the `deleteUser` helper that `signup.spec.ts` already uses.

**Flow + selectors:**

| Step | Action | Assertion |
|---|---|---|
| 1 | `page.goto('/sign-up')` | URL contains `/sign-up`. |
| 2 | Fill `getByLabel("Email address", { exact: true })` + `getByLabel("Password", { exact: true })`, click `getByRole("button", { name: /continue\|sign up/i }).first()`. | Spec mirrors `signup.spec.ts` exactly so the Clerk SignUp form drift is one shared place. |
| 3 | OTP verification: fill `locator('input[autocomplete="one-time-code"]').first()` with `424242`, click Continue if visible. | URL no longer contains `/sign-up` or `/sign-in`. |
| 4 | Landed on `/zombies` empty-state. | `getByText("usezombie")` OR a dashboard sentinel visible. WorkspaceSwitcher shows the auto-provisioned default workspace (existing `data-testid="workspace-switcher"` per `WorkspaceSwitcher.tsx`). |
| 5 | The freshly-provisioned tenant has no zombies. `FirstInstallCard` renders with the CLI command. | `getByText(/zombiectl install --from/)` visible. |
| 5a | **Mint an api-template JWT for the freshly-signed-up user** (the in-test signup does NOT populate `.fixture-jwts.json`, so `FixtureKey`-based clients cannot reach this user). Look up the Clerk user by the test-generated email via `findUserIdByEmail`, then call `attachJwt` from `clerk-admin.ts` to mint session + cookie JWTs. Bootstrap is NOT needed — Clerk's hosted SignUp flow already triggered the real `user.created` webhook to zombied, so the tenant + default workspace exist server-side. | Returns `{sessionJwt, cookieJwt, clerkUserId, sessionId}`. Track `sessionId` for `globalTeardown`-style revocation in `afterEach`. |
| 5b | **Resolve the ephemeral user's default workspace ID** via `getDefaultWorkspaceId({sessionJwt})` (see Interfaces — the helper is widened to accept either a `FixtureKey` or a raw `{sessionJwt}`). | Non-empty `ws` returned; the dashboard's WorkspaceSwitcher in step 4 shows the same workspace. |
| 6 | Install the canonical `platform-ops` zombie via the API path (not the form, not the CLI — Scenario 1 asserts the **post-install dashboard state**, not the install mechanism, which is covered by `install-zombie-{cli,seed}.spec.ts`). Call `seedPlatformOpsZombie({sessionJwt}, ws)` — reads `samples/platform-ops/{SKILL,TRIGGER}.md` from disk and POSTs to `/v1/workspaces/{ws}/zombies` using the mid-test JWT. | New zombie id returned; `await page.goto('/zombies')` shows a row `data-state="live"`. |
| 7 | Open detail page `/zombies/{id}`. | `<LiveEventsPanel>` (the section M64_006 left rendering an inline truncated `<p>` instead of a dialog) renders with either the SSR empty-state OR a populated list. The spec asserts the **section scaffolding**, not the event payload — same downgrade M64_006 took for `events.spec.ts`. |
| 8 | Navigate to `/settings/billing`. | `BillingBalanceCard` renders. The credit balance card shows the starter credit value. Purchase button is disabled (pre-v2.1). |
| 9 | Return to detail page. Click `getByRole("button", { name: "Stop" }).first()`, confirm in `getByRole("alertdialog").getByRole("button", { name: "Stop" })`. | `/zombies` row `data-state` becomes `parked`. |
| 10 | Detail page now shows Resume + Kill. Click Resume → confirm. | Row `data-state` returns to `live`. |
| 11 | Click Kill → confirm. | Row `data-state` becomes `failed`; detail page disabled "Killed" indicator. |
| 12 | `test.afterEach`: delete Clerk user (`deleteUser`) — tenant cleanup is deferred per M64_006. | — |

**Why API-seed and not CLI-spawn for the install in step 6:** running the CLI spawn for every full-lifecycle run on every PROD deploy is fragile (network, CLI bundle path resolution, state-dir tmp) and overlaps `install-zombie-cli.spec.ts`. The new spec is about **the lifecycle after install**, not the install mechanism.

### WS-D — Scenario 2: login → existing platform-ops zombie → observe → bill → halt

**File:** `ui/packages/app/tests/e2e/auth/login-existing-zombie-lifecycle.spec.ts`

**Runs against:** DEV + local + PROD. No skip — the persistent `regular` fixture is provisioned in both Clerk DEV and Clerk PROD by `globalSetup`.

**Fixture-state model:** persistent `regular` fixture, fresh `platform-ops` zombie per test (created in `test.beforeEach`, cleaned in `test.afterEach` via `cleanWorkspaceZombies`). The persistent fixture model is what `lifecycle.spec.ts`/`kill.spec.ts` use today — Scenario 2 is the union of those two with the observation + billing legs added.

**Flow + selectors:** identical to Scenario 1 from step 7 onwards, with the prefix replaced by:

| Step | Action | Assertion |
|---|---|---|
| 1 | `beforeEach`: resolve workspace — `const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular)`. | Non-empty workspace id. |
| 2 | `beforeEach`: `seedPlatformOpsZombie(FIXTURE_KEY.regular, ws)` returns `{id, name}`. | — |
| 3 | `await signInAs(page, FIXTURE_KEY.regular)` (cookie-mount, no form). | — |
| 4 | `page.goto('/zombies')` lands authenticated. | Existing row visible with `data-state="live"`. |
| 5–12 | Same as Scenario 1 steps 7–11 (and step 12 `afterEach` cleanup via `cleanWorkspaceZombies(FIXTURE_KEY.regular, ws)` — no Clerk user deletion, the fixture is persistent). | Same. |

Selectors live in `fixtures/lifecycle.ts` so `lifecycle.spec.ts` + `kill.spec.ts` + Scenario 1 + Scenario 2 all share the same `stopZombie/resumeZombie/killZombie/expectRowState` helpers (RULE UFS — same literals appearing in four specs).

### WS-E — platform-ops template gap analysis (resolved)

**Finding:** `samples/platform-ops/` exists at repo root with `SKILL.md`, `TRIGGER.md`, `README.md`. The skill was authored under M37_001 (`docs/v2/done/M37_001_P1_SKILL_PLATFORM_OPS_ZOMBIE.md`). Test infrastructure at `tests/skill-evals/usezombie-install-platform-ops/substitute.js` already resolves the bundle from `samples/platform-ops`. **No blocker — both scenarios can use the existing bundle without authoring a new skill.**

The new `seedPlatformOpsZombie` helper reads `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` from disk at suite run time (Playwright fixture, not a build step) and POSTs to `/v1/workspaces/{ws}/zombies`. Worktree-root resolution mirrors `install-zombie-cli.spec.ts:WORKTREE_ROOT`.

---

## Interfaces

No new HTTP endpoints. The two new specs hit existing handlers:

- `POST /v1/workspaces/{ws}/zombies` — `seedPlatformOpsZombie`. Existing handler; existing wire shape.
- `GET /v1/tenants/me/workspaces` — `getDefaultWorkspaceId`. Existing.
- `DELETE /v1/users/{id}` (Clerk admin) — `deleteUser` in `test.afterEach` for Scenario 1 only.

New TS helpers (signatures the implementation must NOT change without spec amendment):

```ts
// fixtures/seed.ts (extension)

// Auth handle: either a cached fixture key (Scenario 2 / persistent fixtures)
// OR a mid-test-minted JWT (Scenario 1 ephemeral signup user, who is NOT in
// the `.fixture-jwts.json` cache because they were created in-test).
export type AuthHandle = { key: FixtureKey } | { sessionJwt: string };

// Both helpers accept the union; the extending implementation routes
// `key`-shaped handles through the existing `clientFor(key)` and
// `sessionJwt`-shaped handles through a new `clientForJwt(sessionJwt)` thin
// constructor in `api-client.ts`. No duplicated request logic.
export async function getDefaultWorkspaceId(auth: AuthHandle): Promise<string>;
export async function seedPlatformOpsZombie(
  auth: AuthHandle,
  workspaceId: string,
): Promise<Zombie>;

// fixtures/lifecycle.ts (new)
export async function stopZombie(page: Page, zombieId: string): Promise<void>;
export async function resumeZombie(page: Page, zombieId: string): Promise<void>;
export async function killZombie(page: Page, zombieId: string): Promise<void>;
export async function expectRowState(
  page: Page,
  zombieId: string,
  state: "live" | "parked" | "failed",
): Promise<void>;
```

**Backwards-compatibility shim for the existing callers** (RULE NLG — pre-v2.0.0, no compat layer). The existing `getDefaultWorkspaceId(FIXTURE_KEY.regular)` call sites in `fixtures/seed.ts` and the eight existing specs are migrated **in the same commit** as the signature widening: `getDefaultWorkspaceId(FIXTURE_KEY.regular)` → `getDefaultWorkspaceId({ key: FIXTURE_KEY.regular })`. No overload, no parallel signature.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Clerk PROD test-mode disabled | `424242` OTP rejected when Scenario 1 attempted on PROD | Spec is `test.skip(isProdApi)` — never executes there. |
| `samples/platform-ops/SKILL.md` moved or renamed | Repo refactor | `seedPlatformOpsZombie` throws with the resolved path in the error; suite fails fast at first test. |
| Clerk DEV password policy tightens and rejects 32-byte base64url | Future Clerk config change | `globalSetup` fails loudly with the policy error in the body (existing `failLoud` pattern). WS-B item #9 adds a `_smoke` regression. |
| `.fixture-jwts.json` moved into Playwright's `outputDir`/`playwright-auth-report` by a future refactor | Refactor blast radius | WS-B item #4 vitest test fails — blocks the PR before merge. |
| Tenant pollution causes billing balance to drift below zero on a fixture | Long-running PROD accumulation | Out of scope (per Captain). When this fires, reactivate the deferred fixture-teardown design. |
| Resume button absent in step 10 | KillSwitch state-machine drift (zombied returns a new status) | `stopZombie` already asserts `data-state="parked"`. Resume helper waits for the button via `getByRole("button", { name: "Resume" })` with a timeout — fails loud, not silently. |
| Vercel deploy fires `auth-e2e-prod` before `app.usezombie.com` is hot | Existing Fly pre-warm gap (separate handoff) | Out of scope. Existing M64_006 retry/timeout posture continues. |

---

## Invariants

1. **No PROD-Clerk fixture user is ever created without `public_metadata.is_test_fixture = true`.** Enforced by `ensureUser` in `clerk-admin.ts` — every code path that creates a Clerk user routes through the metadata-tagging branch. The implementation PR must keep this invariant; a new code path that bypasses `ensureUser` MUST also set the tag (lint-check: grep for `clerkRequest<…>("POST", "/users"`).
2. **`.fixture-jwts.json` never appears inside a CI artifact.** Enforced by the WS-B #4 regression test plus the existing `chmod 0o600` + gitignore.
3. **Scenario 1 never runs against PROD.** Enforced at the spec level by `test.skip(isProdApi)` mirroring `signup.spec.ts:46`. The implementation PR may NOT remove this guard without same-PR documentation in `docs/AUTH.md` of the new safety story.
4. **Both scenarios use the same `samples/platform-ops/{SKILL,TRIGGER}.md` bundle.** Enforced by routing both through `seedPlatformOpsZombie`. A spec that hand-rolls a different SKILL.md body is rejected at review.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `signup-platformops-lifecycle.spec.ts → signup → install → observe → bill → halt` | Ephemeral signup lands on `/zombies` empty-state; auto-provisioned workspace visible; `seedPlatformOpsZombie` adds a row with `data-state="live"`; detail page renders `<LiveEventsPanel>` scaffolding; `/settings/billing` shows balance card; Stop → row `parked`; Resume → row `live`; Kill → row `failed` + disabled Killed indicator on detail page. |
| `login-existing-zombie-lifecycle.spec.ts → persistent fixture → observe → bill → halt` | Persistent `regular` fixture signs in via cookie-mount; pre-seeded `platform-ops` zombie row visible with `data-state="live"`; same observation + lifecycle legs as Scenario 1. |
| `_smoke.spec.ts → @clerk/nextjs major pin honored` (WS-B #8) | The resolved major of `@clerk/nextjs` equals `CLERK_NEXTJS_PINNED_MAJOR` (constant in `fixtures/constants.ts`). Predicate: parse the version from `node_modules/@clerk/nextjs/package.json` (NOT from the root `package.json` range string, which would conflate `^4.x` and `^5.x` if the range starts with `^`), `semver.major(installedVersion)` === `CLERK_NEXTJS_PINNED_MAJOR`. Fails fast if a `bun install` bumps the major against the pin. |
| `_smoke.spec.ts → fixture password clears Clerk policy` (WS-B #9) | `globalSetup` provisioned both fixture users without error AND `freshPassword()` output length is ≥ 16 chars. |
| `fixtures/_jwt-cache-location.test.ts → cache stays outside Playwright dirs` (WS-B #4) | `path.resolve(".fixture-jwts.json")` does NOT start with `path.resolve("playwright-auth-results")` OR `path.resolve("playwright-auth-report")`. |
| `_smoke.spec.ts → __clerk_db_jwt is a real JWT on PROD` (WS-B #8 part 2) | On PROD only: the value placed into `__clerk_db_jwt` cookie has three `.`-separated segments. On DEV: skipped (literal `"fixture-dev-browser"` is accepted). |

Negative tests covered by Failure Modes table; no new fixture file (`samples/fixtures/…`) needed.

Regression tests: the existing `lifecycle.spec.ts` + `kill.spec.ts` + `events.spec.ts` + `settings-billing.spec.ts` MUST continue to pass. The new helpers in `fixtures/lifecycle.ts` are additive; the implementation PR refactors the existing two specs to use them (RULE UFS) without changing behavior.

---

## Acceptance Criteria

- [ ] Vault prerequisite met (WS-B #1 `BLOCKED_ON_workflow-wiring` gate cleared) — verify: `op read 'op://ZMB_CD_DEV/e2e-fixtures-email/regular'` AND `op read 'op://ZMB_CD_DEV/e2e-fixtures-email/admin'` AND the two PROD equivalents under `op://ZMB_CD_PROD/e2e-fixtures-email/{regular,admin}` BOTH return a non-mailinator domain; AND `.github/workflows/deploy-dev.yml` + `.github/workflows/smoke-post-deploy.yml` set `AUTH_E2E_REGULAR_EMAIL` + `AUTH_E2E_ADMIN_EMAIL` from those op:// paths; AND `globalSetup` log line for the most recent CI run contains the non-mailinator emails. If any check fails, the implementation PR MUST NOT merge.
- [ ] WS-A finding recorded in `docs/AUTH.md` "PROD fixture identity carve-out" — verify: `grep -n "password_enabled.*PATCH" docs/AUTH.md`
- [ ] WS-B vulnerability table copied (verbatim) into `docs/AUTH.md` "Known gaps" subsection — verify: `grep -c "FIX_THIS_PR\|ACCEPTED_RISK\|DEFERRED" docs/AUTH.md` ≥ 11
- [ ] `signup-platformops-lifecycle.spec.ts` passes locally (`bun run test:e2e:auth:local`) — verify: paste the green run line
- [ ] `signup-platformops-lifecycle.spec.ts` passes against `api-dev` in `auth-e2e-dev` job — verify: link the GH Actions run URL
- [ ] `signup-platformops-lifecycle.spec.ts` is skipped against PROD — verify: `bun run test:e2e:auth` against `https://api.usezombie.com` shows the spec as `skipped`
- [ ] `login-existing-zombie-lifecycle.spec.ts` passes locally AND against `api-dev` AND against `api` (PROD) — verify: paste three green run lines / CI URLs
- [ ] `seedPlatformOpsZombie` reads from `samples/platform-ops/` (worktree-root-relative) — verify: `grep -n "samples/platform-ops" ui/packages/app/tests/e2e/auth/fixtures/seed.ts`
- [ ] `fixtures/lifecycle.ts` helpers replace duplicated KillSwitch+ConfirmDialog code in `lifecycle.spec.ts` AND `kill.spec.ts` — verify: `grep -c 'getByRole.*alertdialog' ui/packages/app/tests/e2e/auth/{lifecycle,kill}.spec.ts` is 0
- [ ] WS-B #4 vitest regression passes — verify: `bun run test:coverage` includes `_jwt-cache-location.test.ts`
- [ ] No file added or modified exceeds 350 lines — verify: `git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l | awk '$1 > 350'`
- [ ] `gitleaks detect` clean — verify: `gitleaks detect` output
- [ ] `make lint` clean
- [ ] Existing `lifecycle.spec.ts` + `kill.spec.ts` + `events.spec.ts` + `settings-billing.spec.ts` still pass

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: New specs run locally (requires local zombied stack up + DEV op:// creds)
cd ui/packages/app && bun run test:e2e:auth:local -- \
  tests/e2e/auth/signup-platformops-lifecycle.spec.ts \
  tests/e2e/auth/login-existing-zombie-lifecycle.spec.ts

# E2: Existing specs still pass (regression)
cd ui/packages/app && bun run test:e2e:auth:local

# E3: Vitest regression for JWT cache location
cd ui/packages/app && bun run test:coverage

# E4: Lint
make lint 2>&1 | tail -10

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3

# E6: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E7: AUTH.md captures WS-A finding + WS-B table
grep -n "PATCH.*password_enabled\|silent no-op" docs/AUTH.md
grep -c "FIX_THIS_PR\|ACCEPTED_RISK\|DEFERRED" docs/AUTH.md

# E8: Helper consolidation (RULE UFS)
grep -c 'getByRole.*alertdialog' ui/packages/app/tests/e2e/auth/{lifecycle,kill}.spec.ts
# expect 0:0 — both refactored to use fixtures/lifecycle.ts helpers
```

---

## Dead Code Sweep

N/A — no files deleted. The implementation PR adds two new specs, extends two existing fixture files, adds one new fixture file, and edits `docs/AUTH.md`. No symbols removed.

If the implementation PR finds a viable password-disable endpoint during WS-A research and adds a `disablePassword()` helper, that helper is new code — still no deletions.

---

## Skill-Driven Review Chain (mandatory)

Per project standard (`/write-unit-test` → `/review` → `/review-pr` → `kishore-babysit-prs`). This spec's CHORE(close) is doc-only (no implementation in this PR); the chain runs in full on the implementation PR.

For THIS PR (spec-only):

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After spec lands in `docs/v2/pending/` | None | — | This PR is the planning gate. No skill chain. |

The implementation PR (separate milestone, separate branch) runs the full chain.

---

## Verification Evidence

Filled in by the implementation PR — not this spec PR.

---

## Out of Scope

- **Fixture teardown.** Captain deferred per M64_006. Do not propose tenant or billing-balance cleanup in this milestone.
- **PR-time `auth-e2e` gate.** WS-B item #6 — surfaced as an open question, not a deliverable.
- **Separate webhook test secret.** WS-B item #3 — separate backend milestone (zombied accepts `CLERK_WEBHOOK_TEST_SECRET` and quarantines fixture tenants).
- **Cross-tenant admin membership.** M64_006 Discovery item; not graduated into this spec.
- **EventDetail dialog.** M64_006 Discovery item; Scenario 1 step 7 asserts section scaffolding only (same downgrade as `events.spec.ts`).
- **Webhook-driven event seeding for the observation leg.** Both scenarios assert the observation **panel renders**, not that real ingested events appear. Real-event seeding remains M64_006 Discovery follow-on.
- **`.github/workflows/**` edits.** Different agent's territory per the handoff constraints.
- **`~/Projects/docs/changelog.mdx` `<Update>`.** This PR is not user-visible; the implementation PR adds the changelog entry.
- **PROD-Clerk test-mode enablement.** Out of scope here; Scenario 1 stays `test.skip(isProdApi)` until that flag lands separately.

---

## Discovery (out-of-scope but adjacent observations the implementing agent SHOULD surface)

1. **Clerk admin API documentation gap.** WS-A revealed that `PATCH /v1/users/{id}` silently ignores `password_enabled` with no error. Clerk's docs imply the field is writable; observed behavior contradicts. Worth opening a Clerk support ticket — both for confirmation and to learn the correct endpoint (likely `DELETE /v1/users/{id}/password` or instance-level config). If a confirmed-working path emerges, WS-B item #2 reopens as `FIX_THIS_PR` for a follow-up milestone.
2. **`@clerk/nextjs` major-pin hygiene.** WS-B #8 motivates pinning. Other usezombie Next.js deps (`next`, `react`) are also unpinned in `package.json` — a broader pinning sweep is its own minor hygiene milestone.
3. **`makeMsgId` reuse across `bootstrap` and `events`.** `fixtures/svix.ts` already centralises Svix signing. If a future spec drives webhook ingest (resolving the M64_006 events deferral), it should land its msg-id helper next to `newMsgId` in `svix.ts` not as a per-spec function. RULE UFS pre-emption.
4. **Fixture-billing observability gap.** WS-B #5 accepted risk depends on Captain having a way to **see** that fixture tenants are accumulating balance. The ops dashboard currently does not filter by `public_metadata.is_test_fixture`. A 1-line dashboard query addition would close the observability loop without committing to teardown design.
5. **Two `bootstrap` Svix msg ids per `globalSetup`.** Each `globalSetup` POSTs two `user.created` events (regular + admin) with `msg_e2e_bootstrap` prefix. Clerk's hosted Svix dashboard would show these as duplicate-prefix posts. Not a defect — informational; mention in `docs/AUTH.md` so an operator triaging the Clerk Svix log knows where they come from.

---

## Branch + PR conventions for this spec PR

- Branch: `chore/m65-001-spec-auth-e2e-lifecycle-scenarios` (off `main`).
- Single commit: `chore(spec): add M65_001 — auth e2e full lifecycle scenarios + vulnerability audit`.
- PR title: `chore(spec): M65_001 — auth e2e full lifecycle scenarios + vulnerability audit`.
- PR body links: this spec file, `docs/v2/done/M64_006_…`, `docs/AUTH.md` "PROD fixture identity carve-out" anchor, plus the WS-A finding summary.
- No `/review` skill chain on this PR — the chain runs on the implementation PR per the table above.
- Captain inspects, prioritises, and opens the implementation milestone separately.
