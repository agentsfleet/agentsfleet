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
**Status:** DONE
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
3. `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — `provisionUser`/`bootstrapTenant`/`attachJwt` 3-phase chain. Any change to the password-hardening posture (WS-A finding) lands here.
4. `ui/packages/app/tests/e2e/acceptance/global-setup.ts` — fixture identity resolution + JWT cache write. `freshPassword()`, `is_test_fixture` metadata, and the random-per-create posture all live here.
5. `ui/packages/app/tests/e2e/acceptance/install-zombie-cli.spec.ts` and `install-zombie-seed.spec.ts` — reference for the install path. The full-lifecycle scenarios deliberately keep CLI coverage in `install-zombie-cli.spec.ts` and drive the dashboard install form instead, so the browser route, Server Action, redirect, and detail-page render are covered end to end.
6. `ui/packages/app/app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` — Stop/Resume/Kill state machine UI. Selector inventory for the new scenarios lives there (status `active` → Stop+Kill, `paused`/`stopped` → Resume+Kill, `killed`/`errored` → terminal disabled "Killed" indicator).
7. `ui/packages/app/app/(dashboard)/zombies/components/ZombiesList.tsx:liveStateOf` — canonical mapping from zombied status to dashboard `data-state` (`active→live`, `killed|errored→failed`, everything else→`parked`). New assertions key on this attribute.
8. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — the canonical bundle used by the install CLI coverage. The full-lifecycle UI helper builds the same two-file wire shape (`TRIGGER.md` + `SKILL.md`) inline with a unique test name so repeated runs do not collide on `(workspace_id, name)`.
9. `.github/workflows/deploy-dev.yml` (`acceptance-e2e-dev` job) and `.github/workflows/smoke-post-deploy.yml` (`acceptance-e2e-prod` job) — the deployment gates the new specs will plug into. Both already wire op:// secrets, Playwright browser cache, and artifact upload of `playwright-acceptance-report/` only.

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

**Goal (testable):** Two Playwright specs (`signup-lifecycle.spec.ts`, `login-install-lifecycle.spec.ts`) run inside the existing `tests/e2e/acceptance/` suite. Each spec walks a full operator flow — signup OR existing-fixture login → land on the dashboard → install through `/zombies/new` → observe the detail page → settings/billing → Stop → Resume → Kill — and the vulnerability audit table is dispositioned: each row has either a same-PR fix or an explicit accepted-risk note with reactivation conditions.

**Problem:**

1. The existing harness has eight specs that each exercise one slice (install, lifecycle stop, kill, events, logs, multi-zombie, multi-workspace, settings-billing). No spec walks a real user from "I just signed up" to "I just killed a zombie I observed running." Coverage gaps live between the slices: e.g. the dashboard route-guard chain that runs only when a brand-new tenant first lands.
2. The PROD harness creates fixture identities in Clerk PROD with `password_enabled: true` on a public mailinator inbox. WS-A in this spec resolves whether the proposed "disable password" hardening is viable; the parallel handoff resolves the mailinator side.
3. Captain wants both flows on PROD because the dashboard's first-deploy regressions historically hit signup-derived state (workspace auto-provision, starter credit, empty-state render), not pre-seeded-fixture state.

**Solution summary:** Two full-lifecycle specs (WS-C, WS-D) plus a Captain-driven UI-walk expansion (WS-F) landed under `ui/packages/app/tests/e2e/acceptance/`. Scenario 1 (signup) drives Clerk's browser SDK directly via `Clerk.client.signUp.create` + `attemptEmailAddressVerification` to bypass Cloudflare Turnstile on the hosted SignUp form — Clerk's `424242` test OTP still validates server-side. Scenario 2 reuses the persistent `regular` fixture. WS-F covers every reachable route in the dashboard `Shell` nav and `/settings/*` sub-pages (sign-out round-trip, credentials lifecycle, approvals, settings/provider, dashboard home, multi-workspace + zombie lifecycle, live-counter increment, hard reload + back-nav mid-session). WS-G introduces a central error-presentation helper (`lib/errors.ts`) keyed on `UZ-XXX-NNN` backend codes so every "Failed to <verb>" fallback routes through one place with Captain's tone-of-voice rules applied. The vulnerability audit (WS-B) is dispositioned in the spec body; only rows whose fix is in scope for the implementation PR move into Files Changed.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/tests/e2e/acceptance/signup-lifecycle.spec.ts` | CREATE | Scenario 1 — ephemeral signup → dashboard install form → observe → bill → halt. DEV/local only; currently skipped because Clerk DEV Turnstile blocks the hosted SignUp form. |
| `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` | CREATE | Scenario 2 — persistent fixture login → dashboard install form → observe → bill → halt. Runs on DEV/local and is the currently green full-lifecycle scenario. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/install-ui.ts` | CREATE | Shared dashboard-form install helper. Pastes `TRIGGER.md` + `SKILL.md`, submits `/zombies/new`, and waits for the App Router URL change with `expect(page).toHaveURL`. |
| `ui/packages/app/app/(dashboard)/zombies/new/InstallZombieForm.tsx` | EDIT | Remove the `router.refresh()` after `router.push`, which raced the current-route refresh against the destination URL commit and left the browser stuck on `/zombies/new`. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | EDIT | Keep existing API seed helpers for slice tests; the full-lifecycle scenarios no longer seed through this path. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/api-client.ts` | EDIT | Widen `clientFor` to accept `ClientHandle = FixtureKey \| { sessionJwt: string }`. One public entrypoint, one fetch implementation — no duplicated request logic (RULE UFS). String input still loads from the `.fixture-jwts.json` cache; the `{sessionJwt}` variant uses the JWT directly. |
| existing spec call sites | NO-OP | `getDefaultWorkspaceId(FIXTURE_KEY.regular)` continues to work — `FixtureKey` is part of the `ClientHandle` union, so no migration is required and RULE NLG is satisfied by the single signature. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/lifecycle.ts` | CREATE | Shared selectors + action helpers: `stopZombie(page, id)`, `resumeZombie(page, id)`, `killZombie(page, id)`. Pulls the duplicated KillSwitch + ConfirmDialog wiring out of `lifecycle.spec.ts`/`kill.spec.ts` and the new scenarios. Eliminates the row of literal-duplicates the existing two specs have today (RULE UFS). |
| `ui/packages/app/tests/e2e/acceptance/fixtures/_jwt-cache-location.test.ts` | CREATE | Vitest regression for WS-B #4 — asserts `.fixture-jwts.json` path is outside `playwright-acceptance-results/` and `playwright-acceptance-report/`. Runs in `make test`. |
| `ui/packages/app/tests/e2e/acceptance/_smoke.spec.ts` | EDIT | WS-B #8 + #9 assertions: (a) resolved `@clerk/nextjs` major equals the pinned constant in `fixtures/constants.ts`; (b) fixture sign-in creates a Clerk-accepted browser session through `clerk.signIn`; (c) `freshPassword()` output length ≥ 16 chars (regression for Clerk password-policy tightening). |
| `ui/packages/app/package.json` | EDIT | WS-B #8 — pin `@clerk/nextjs` major (caret pin against the current installed major); record the pinned major as a constant in `fixtures/constants.ts` so the smoke assertion has a single source of truth. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` | EDIT | Add `CLERK_NEXTJS_PINNED_MAJOR` constant consumed by the WS-B #8 smoke assertion (RULE UFS — one literal, two readers). |
| `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` | EDIT | WS-B #11 — tighten `mintTokens` `expires_in_seconds` from the older wide window to the current 900-second suite cache window. |
| `docs/AUTH.md` | EDIT | Update "PROD fixture identity carve-out" with the WS-A finding (PATCH `password_enabled:false` is a silent no-op on Clerk admin API; harness retains random-per-create password posture). Append "Known gaps" subsection enumerating accepted vulnerabilities + the two Clerk PROD fixture user IDs with the `is_test_fixture` metadata filter query (WS-B #7). |
| `ui/packages/app/tests/e2e/acceptance/fixtures/signup.ts` | CREATE | WS-C — direct Clerk SDK signup helper. `signUpAs(page, email, password)` calls `setupClerkTestingToken({page})` then `Clerk.client.signUp.create` + `prepareEmailAddressVerification` + `attemptEmailAddressVerification({code: "424242"})` + `setActive`. Bypasses the hosted SignUp form (Turnstile blocker) without sacrificing OTP validation. |
| `ui/packages/app/tests/e2e/acceptance/signout-and-signin.spec.ts` | CREATE | WS-F — sign-in → protected page → `clerk.signOut` → `/sign-in` redirect → re-sign-in → protected page renders. Catches stale-session + middleware regressions invisible to single-shot specs. |
| `ui/packages/app/tests/e2e/acceptance/credentials-lifecycle.spec.ts` | CREATE | WS-F — `/credentials` add (JSON-body form) → list contains row → per-row Delete → ConfirmDialog → row absent. Workspace-scoped envelope-encrypted store; no other spec exercises it. |
| `ui/packages/app/tests/e2e/acceptance/approvals-page.spec.ts` | CREATE | WS-F — `/approvals` renders heading + Pending section for the authed fixture. Approval gates require executor turn (out-of-scope dep); page-render assertion mirrors `events.spec.ts` posture. |
| `ui/packages/app/tests/e2e/acceptance/settings-provider.spec.ts` | CREATE | WS-F — `/settings/provider` renders "Active configuration" + "Change provider" sections. Provider resolver has three failure modes (synthesised default / credential mismatch / 5xx) all visible as chrome here. |
| `ui/packages/app/tests/e2e/acceptance/dashboard-home.spec.ts` | CREATE | WS-F — `/` renders header + `StatusCard` tiles OR FirstInstallCard. Either render path is correct (fixture state is not deterministic between specs); the disjunction catches a Suspense or `StatusTiles` data-fetch regression. |
| `ui/packages/app/tests/e2e/acceptance/workspace-zombie-lifecycle.spec.ts` | CREATE | WS-F — switch to 2nd workspace via header `WorkspaceSwitcher`, install via `/zombies/new`, kill via UI confirm dialog, assert `data-state=failed`. UI-delete intentionally skipped pending the known `UZ-INTERNAL-002` ConnectionBusy bug on DELETE for killed rows (see Discovery). |
| `ui/packages/app/tests/e2e/acceptance/zombie-count.spec.ts` | CREATE | WS-F — seed three zombies via API and assert the `/zombies` page header `{N} live` badge increments each pass. Pins the `liveStateOf` mapping + revalidate plumbing. |
| `ui/packages/app/tests/e2e/acceptance/reload-and-back-nav.spec.ts` | CREATE | WS-F — `page.reload()` on `/zombies/[id]` re-resolves SSR session + RSC tree without redirecting; soft `page.goto('/events')` and back keeps the detail page hydrated. Catches focus-trap / cookie-rehydrate regressions. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | EDIT | Same `router.refresh()` race InstallZombieForm hit — after `router.push("/zombies")`, the refresh re-fetched the current `/zombies/[id]` route before the URL commit and stalled the browser on the deleted zombie. `/zombies` is `force-dynamic`; no manual refresh needed. Also routes the delete error through `presentErrorString` (WS-G). |
| `ui/packages/app/lib/errors.ts` | CREATE | WS-G — `presentError({errorCode, message, action})` + `presentErrorString(...)` helpers. Keyed by curated `UZ-XXX-NNN` codes (currently 8 mapped, fallback handles the long tail). Sentence-case operator-first title, optional next-action body, code as monospaced trailer. No localization — future i18n PR swaps map values for keys. |
| `ui/packages/app/lib/errors.test.ts` | CREATE | WS-G — unit tests covering known-code map, unknown-code fallback with usable message, unknown-code default sentence, and the join-with-period behaviour of `presentErrorString`. |
| `ui/packages/app/lib/actions/with-token.ts` | EDIT | WS-G — `ActionResult<T>` now carries `errorCode?: string`. `withToken` lifts `ApiError.code` into the union so callers can feed `errorCode` to `presentError` without re-importing `ApiError`. The `Not authenticated` branch maps to `UZ-AUTH-401` so the helper picks the curated friendlier string. |
| `ui/packages/app/lib/actions/with-token.test.ts` | EDIT | WS-G — assertions extended for the new `errorCode` field on the 401 and ApiError paths. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` | EDIT | WS-G — `ActionConfig` gains a static `errorVerb` literal per Stop/Resume/Kill; the dialog `onError` and the action-failure branch both route through `presentErrorString` instead of building strings from `confirmLabel.toLowerCase()` (RULE UFS — verb literals stay adjacent to their config). |
| `ui/packages/app/app/(dashboard)/zombies/new/InstallZombieForm.tsx` | EDIT | WS-G — install error wraps `presentErrorString({errorCode, message, action: "install the zombie"})`. The 409 name-collision case keeps its hand-rolled message because it points at an exact field to fix. |
| `ui/packages/app/app/(dashboard)/zombies/components/ZombiesList.tsx` | EDIT | WS-G — "load more" error wraps `presentErrorString`. |
| `ui/packages/app/app/(dashboard)/credentials/components/AddCredentialForm.tsx` | EDIT | WS-G — parse + store errors both wrap `presentErrorString` with distinct static actions ("parse the credential JSON" / "store the credential"). |
| `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` | EDIT | WS-G — delete error wraps `presentErrorString`. |
| `ui/packages/app/app/(dashboard)/approvals/components/ApprovalsList.tsx` | EDIT | WS-G — "load more" and "resolve" errors wrap `presentErrorString`. Resolve uses static per-branch verbs (`"approve this approval"` / `"deny this approval"`) — never `${decision}`-interpolated (RULE UFS). |
| `ui/packages/app/app/(dashboard)/approvals/[gateId]/ResolveButtons.tsx` | EDIT | WS-G — same per-branch static verbs as the list. |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | WS-G — "load more usage events" error wraps `presentErrorString`. |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | WS-G — "load more events" error wraps `presentErrorString`. |
| `ui/packages/app/lib/clerkAppearance.ts` | EDIT | Sign-in card lifted from `--surface-1` (`#11161a`) to `--surface-2` (`#181e22`) over the page `--bg` (`#0a0d0e`); border bumped to `--border-strong`. The card now reads as a card on the auth route instead of disappearing into a uniform-black page. |
| `ui/packages/design-system/src/__tests__/setup.ts` (or equivalent) | EDIT | Pre-existing flake fix — register `@testing-library/jest-dom` matchers (`expect.extend(matchers)`) so `toBeInTheDocument` / `toHaveAttribute` resolve under vitest. Tooltip test was failing on `Invalid Chai property: toBeInTheDocument`. |

**Files NOT changed (explicit non-goals on this milestone):**

- `.github/workflows/**` — different agent's territory. The new specs auto-run because Playwright globs `tests/e2e/acceptance/*.spec.ts`.
- `ui/packages/app/tests/e2e/acceptance/signup.spec.ts` — keep as-is; Scenario 1 is purely additive.
- `src/http/handlers/webhooks/clerk.zig` — separate-webhook-test-secret hardening (WS-B #3) is a deferred backend milestone, recorded in Discovery.
- `ui/packages/app/tests/e2e/acceptance/global-teardown.ts` — fixture teardown deferred per Captain (M64_006).

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
| 4 | `.fixture-jwts.json` (Token B `sessionJwt` cache carrying tenant claims) could ride along in an artifact upload. | S2 | File written at `ui/packages/app/.fixture-jwts.json` mode 0600, gitignored at `.gitignore:19`. CI artifact uploads target `ui/packages/app/playwright-acceptance-report/` (single subdirectory), not the package root. Playwright's `outputDir` is `playwright-acceptance-results`. The cache file is in NEITHER subdirectory. | Add a guard test that asserts the file path does not match either Playwright-managed directory — catches a future refactor that moves `outputDir` or the cache. | `ui/packages/app/tests/fixture-jwt-cache-location.test.ts` | `FIX_THIS_PR` — cheap regression-proof. |
| 5 | Tenant pollution in PROD `core.tenants` — fixture tenants accumulate `tenant_billing.balance_nanos` and any state created by tests forever. | S3 | Per-spec teardown deletes zombies; tenant row reused. No teardown of tenant or billing balance. | None on this milestone — Captain explicitly deferred until the first PROD run accumulates observable state. | N/A | `ACCEPTED_RISK` with reactivation condition: revisit after one calendar quarter of PROD runs OR if `tenant_billing.balance_nanos` for either fixture tenant rises above a threshold the ops dashboard should set. |
| 6 | No PR-time `auth-e2e` gate — the suite only fires post-merge to `main` and post-deploy to PROD. A breaking auth-flow change lands on `main` before catching. | S3 | `qa.yml` runs unauthenticated smoke on every PR; auth suite is post-merge. | Add a PR-time job; estimate adds a few minutes per PR. | `.github/workflows/qa.yml` (different agent's territory per Captain's constraints) | `DEFERRED` — open question for Captain. This spec surfaces but does not propose. |
| 7 | First-PROD-deploy provisioned two Clerk PROD identities with no warning gate or operator review. | S2 | Mitigated post-hoc by commit f86d0c35 — identities are tagged `public_metadata.is_test_fixture: true`, passwords are random and unpersisted. Verifying what Clerk PROD ops dashboards show after first run is part of this spec's acceptance criteria. | Document the expected PROD-Clerk users list in `docs/AUTH.md` "PROD fixture identity carve-out" with the actual Clerk user IDs and the metadata tag query that filters them. | `docs/AUTH.md` | `FIX_THIS_PR` (docs-only) — Captain inspects PROD Clerk dashboard, agent records the two user IDs + metadata filter query in AUTH.md. |
| 8 | Browser session cookie mounting can drift when `@clerk/nextjs` or `@clerk/testing` changes cookie validation. | S2 | Earlier harness code hand-wrote `__session` / `__client_uat` / `__clerk_db_jwt`; current implementation uses `setupClerkTestingToken` + `clerk.signIn`, so clerk-js owns the cookie shape and includes the `azp` claim Server Actions require. | Pin `@clerk/nextjs` major version in `ui/packages/app/package.json`; add a `_smoke` assertion that fixture sign-in produces an accepted Clerk session before selector-dependent specs run. | `ui/packages/app/package.json`, `_smoke.spec.ts`, `fixtures/auth.ts` | `FIX_THIS_PR`. |
| 9 | `freshPassword()` uses `crypto.randomBytes(32).toString("base64url")` — 256 bits entropy. Clerk's password policy MAY reject characters outside its acceptable set (uppercase + lowercase + digit + symbol class checks). If policy tightens, fixture provisioning fails opaquely with "password rejected" — and the workflow has no clear remediation path. | S3 | Today the random base64url string clears Clerk's permissive defaults. No regression test. | Add `_smoke.spec.ts` assertion that `freshPassword()` output matches Clerk's documented allowable-chars constraints (or that a `provisionUser` round-trip on the regular fixture succeeds on every run). The latter already runs as part of `globalSetup` — `_smoke` re-validates the cache. | `_smoke.spec.ts` | `FIX_THIS_PR` (one assertion). |
| 10 | `bootstrapTenant` re-Svix-signs the same `msg_e2e_bootstrap_…` UUID on each run but a fresh timestamp. Webhook handler dedupes on msg id — but the harness mints a fresh msg id with `newMsgId("msg_e2e_bootstrap")` per call, so back-to-back runs are not deduped. If `globalSetup` runs twice rapidly (e.g. retry), zombied processes both as new events. | S3 | Idempotent at the zombied data layer (`user.created` for an existing Clerk user returns `created:false`), so no broken state. But the wire is honest-but-wasteful. | None — accepted. The current shape mirrors Clerk's own retry behavior (each retry is a new Svix msg id), and the data-layer idempotency is the load-bearing guarantee. | N/A | `ACCEPTED_RISK`. |
| 11 | The Bearer JWT cache window should be bounded to the acceptance-suite wall clock. | S3 | Default Clerk token lifetime is too short for the full suite; the earlier wide cache window was broader than the suite needs. | Tighten to `expires_in_seconds: 900`, which covers the observed suite duration while bounding leaked-cache impact. | `clerk-admin.ts:mintTokens` | `FIX_THIS_PR`. |

Items #4, #7, #8, #9, #11 are `FIX_THIS_PR`. Everything else is dispositioned without code change in this milestone.

### WS-C — Scenario 1: signup → workspace → dashboard install → observe → bill → halt

**File:** `ui/packages/app/tests/e2e/acceptance/signup-lifecycle.spec.ts`

**Runs against:** DEV + local once the Clerk DEV Turnstile blocker is resolved. PROD is skipped (`test.skip(isProdApi, …)`) until either (a) Clerk PROD has test mode enabled OR (b) a vault-resolved private-domain alias replaces the `+clerk_test@mailinator.com` pattern. Both are tracked in Discovery.

**Fixture-state model:** per-test ephemeral. New unique `+clerk_test@mailinator.com` email per run; deleted in `test.afterEach` via the `deleteUser` helper that `signup.spec.ts` already uses.

**Current blocker:** Clerk DEV's hosted SignUp form renders Cloudflare Turnstile before the one-time-password screen. `setupClerkTestingToken` covers Clerk Frontend API calls but does not satisfy the hosted SignUp widget's browser bot-check. This is the same root cause as the pre-existing `signup.spec.ts`; the spec stays skipped until Turnstile is disabled on the DEV instance or the harness adds a supported Clerk signup-testing helper.

**Flow + selectors:**

| Step | Action | Assertion |
|---|---|---|
| 1 | `page.goto('/sign-up')` | URL contains `/sign-up`. |
| 2 | Fill `getByLabel("Email address", { exact: true })` + `getByLabel("Password", { exact: true })`, click `getByRole("button", { name: /continue\|sign up/i }).first()`. | Spec mirrors `signup.spec.ts` exactly so the Clerk SignUp form drift is one shared place. |
| 3 | OTP verification: fill `locator('input[autocomplete="one-time-code"]').first()` with `424242`, click Continue if visible. | URL no longer contains `/sign-up` or `/sign-in`. |
| 4 | Landed on `/zombies` empty-state. | `getByText("usezombie")` OR a dashboard sentinel visible. WorkspaceSwitcher shows the auto-provisioned default workspace (existing `data-testid="workspace-switcher"` per `WorkspaceSwitcher.tsx`). |
| 5 | The freshly-provisioned tenant has no zombies. `FirstInstallCard` renders with the CLI command. | `getByText(/zombiectl install --from/)` visible. |
| 5a | Generate a unique zombie name for this run. | Avoids `(workspace_id, name)` collisions when previous interrupted runs left killed rows behind. |
| 6 | Install through the dashboard form via `installViaUI(page, name)`. The helper navigates to `/zombies/new`, pastes valid `TRIGGER.md` + `SKILL.md`, clicks `Install Zombie`, and waits for `/zombies/{id}`. | New zombie id returned; detail page URL matches `/zombies/{id}`. |
| 7 | Open detail page `/zombies/{id}`. | `<LiveEventsPanel>` (the section M64_006 left rendering an inline truncated `<p>` instead of a dialog) renders with either the SSR empty-state OR a populated list. The spec asserts the **section scaffolding**, not the event payload — same downgrade M64_006 took for `events.spec.ts`. |
| 8 | Navigate to `/settings/billing`. | `BillingBalanceCard` renders. The credit balance card shows the starter credit value. Purchase button is disabled (pre-v2.1). |
| 9 | Return to detail page. Click `getByRole("button", { name: "Stop" }).first()`, confirm in `getByRole("alertdialog").getByRole("button", { name: "Stop" })`. | `/zombies` row `data-state` becomes `parked`. |
| 10 | Detail page now shows Resume + Kill. Click Resume → confirm. | Row `data-state` returns to `live`. |
| 11 | Click Kill → confirm. | Row `data-state` becomes `failed`; detail page disabled "Killed" indicator. |
| 12 | `test.afterEach`: delete Clerk user (`deleteUser`) — tenant cleanup is deferred per M64_006. | — |

**Why dashboard form and not CLI-spawn for the install in step 6:** `install-zombie-cli.spec.ts` already owns the canonical CLI install path. The lifecycle specs are the only browser-driven coverage for `/zombies/new`, the Server Action, the App Router redirect, and the detail-page landing state.

### WS-D — Scenario 2: login → dashboard install → observe → bill → halt

**File:** `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts`

**Runs against:** DEV + local + PROD. No skip — the persistent `regular` fixture is provisioned in both Clerk DEV and Clerk PROD by `globalSetup`.

**Fixture-state model:** persistent `regular` fixture, fresh dashboard-installed zombie per test, cleaned in `test.afterEach` via `cleanWorkspaceZombies`. The persistent fixture model is what `lifecycle.spec.ts`/`kill.spec.ts` use today — Scenario 2 is the union of those two with the dashboard-install, observation, and billing legs added.

**Flow + selectors:** identical to Scenario 1 from step 7 onwards, with the prefix replaced by:

| Step | Action | Assertion |
|---|---|---|
| 1 | `await signInAs(page, FIXTURE_KEY.regular)` using `clerk.signIn` from `@clerk/testing/playwright`. | Browser has a Clerk-accepted dashboard session. |
| 2 | Generate a unique zombie name for this run. | Avoids `(workspace_id, name)` collisions when previous interrupted runs left killed rows behind. |
| 3 | `installViaUI(page, name)` drives `/zombies/new`. | New zombie id returned; detail page URL matches `/zombies/{id}`. |
| 4 | `page.goto('/zombies')` lands authenticated. | Installed row visible with `data-state="live"`. |
| 5–12 | Same as Scenario 1 steps 7–11 (and step 12 `afterEach` cleanup via `cleanWorkspaceZombies(FIXTURE_KEY.regular, ws)` — no Clerk user deletion, the fixture is persistent). | Same. |

Selectors live in `fixtures/lifecycle.ts` so `lifecycle.spec.ts` + `kill.spec.ts` + Scenario 1 + Scenario 2 all share the same `stopZombie/resumeZombie/killZombie/expectRowState` helpers (RULE UFS — same literals appearing in four specs).

### WS-E — install bundle shape gap analysis (resolved)

**Finding:** `samples/platform-ops/` exists at repo root with `SKILL.md`, `TRIGGER.md`, `README.md`. The skill was authored under M37_001 (`docs/v2/done/M37_001_P1_SKILL_PLATFORM_OPS_ZOMBIE.md`). `install-zombie-cli.spec.ts` keeps byte-for-byte CLI coverage of that bundle.

The full-lifecycle UI specs use a smaller inline fixture bundle in `installViaUI` with the same server wire shape: `TRIGGER.md` carries `name` + `x-usezombie`, `SKILL.md` carries matching `name` metadata and body prose, and zombied parses both server-side. This keeps full-lifecycle runs collision-safe by generating a unique name per run while preserving the production install shape.

### WS-F — Dashboard UI walk (Captain-driven expansion)

The original M65_001 brief covered two end-to-end scenarios. Mid-implementation audit surfaced that the rest of the dashboard surface had no acceptance coverage at all — `/credentials`, `/approvals`, `/settings/provider`, `/`, sign-out, hard-reload mid-session, soft-nav mid-session, multi-workspace zombie life, and the live-counter increment. Captain's directive: every route reachable from the `Shell` nav gets exercised in this PR, not deferred.

Seven new specs land under `tests/e2e/acceptance/`, each scoped to a single concern with shared fixtures (`signInAs`, `ensureSecondWorkspace`, `cleanWorkspaceZombies`, `installViaUI`, `killZombie`):

| Spec | Concern |
|---|---|
| `signout-and-signin.spec.ts` | `clerk.signOut` clears session → `/sign-in` redirect → `signInAs` re-mounts session → protected page renders. |
| `credentials-lifecycle.spec.ts` | `/credentials` add → list contains → per-row Delete + ConfirmDialog → list empty. |
| `approvals-page.spec.ts` | `/approvals` heading + Pending section render for the authed fixture. |
| `settings-provider.spec.ts` | `/settings/provider` "Active configuration" + "Change provider" sections render. |
| `dashboard-home.spec.ts` | `/` renders header + `StatusCard.first().or(FirstInstallCard)` (either branch is correct for the persistent fixture). |
| `workspace-zombie-lifecycle.spec.ts` | Switch to 2nd workspace (header switcher), install via `/zombies/new`, kill, assert `data-state=failed`. UI-delete deferred — see Discovery. |
| `zombie-count.spec.ts` | Seed three zombies, assert the `{N} live` badge increments each pass. |
| `reload-and-back-nav.spec.ts` | `page.reload()` and `page.goto('/events')` round-trip both keep `/zombies/[id]` hydrated. |

**Selector + helper sharing:** all new specs route through `fixtures/lifecycle.ts` and `fixtures/install-ui.ts` so a future ConfirmDialog or KillSwitch refactor lands in one place (RULE UFS). The Clerk-SDK signup helper (`fixtures/signup.ts`) is its own file because the call sequence (`signUp.create` → `prepareEmailAddressVerification` → `attemptEmailAddressVerification` → `setActive`) is unique to the signup path.

### WS-G — Dashboard error UX standardization + tone pass

Captain called out two interlocking gaps mid-PR: (a) no central error-presentation helper, every `setError` had its own `result.error || "Failed to <verb>"` fallback; (b) tone was sterile — "Failed to delete zombie", "Internal Server Error" — no operator voice, no next-action.

**Helper.** `ui/packages/app/lib/errors.ts` exports `presentError({errorCode, message, action}) → {title, body?, code?}` and `presentErrorString(...)` for callers that need a single string (e.g. `ConfirmDialog`'s `errorMessage` prop). Keyed by curated `UZ-XXX-NNN` codes the dashboard hits in production (8 mapped today, growing organically); fallback uses `action` to construct "Couldn't <verb>" sentences. Useless server `"Failed to …"` messages are detected and replaced rather than concatenated.

**Wiring.** `ActionResult<T>` in `lib/actions/with-token.ts` gains an optional `errorCode?: string` populated from `ApiError.code`. The `Not authenticated` branch maps to `UZ-AUTH-401` so the helper picks the curated friendlier render.

**Sweep.** Every `"Failed to <verb>"` / `"Resolve failed"` / `"Delete failed"` / `"An error occurred"` fallback in the dashboard TSX layer routes through `presentErrorString`. Verb literals are static — never built via `confirmLabel.toLowerCase()` or `${decision}` interpolation. `KillSwitch.tsx` carries a static `errorVerb` per `ActionConfig`; the two approvals call sites use per-branch ternaries with literal strings (RULE UFS, RULE TST-NAM-adjacent — grep-friendly).

**Out of scope (deferred).** A `<ErrorBanner />` design-system primitive that wraps title + body + code as one component. Today the helper returns parts and call sites pass them to existing `Alert` / `ConfirmDialog.errorMessage`. A follow-up PR can land the primitive without changing the helper's contract.

---

## Interfaces

No new HTTP endpoints. The two new specs hit existing handlers:

- `POST /v1/workspaces/{ws}/zombies` — dashboard form / `installViaUI`. Existing handler; existing wire shape `{trigger_markdown, source_markdown}`.
- `GET /v1/tenants/me/workspaces` — `getDefaultWorkspaceId`. Existing.
- `DELETE /v1/users/{id}` (Clerk admin) — `deleteUser` in `test.afterEach` for Scenario 1 only.

New TS helpers (signatures the implementation must NOT change without spec amendment):

```ts
// fixtures/api-client.ts — single exported union, single fetch implementation
export type ClientHandle = FixtureKey | { sessionJwt: string };
export function clientFor(handle: ClientHandle): ApiClient;

// fixtures/seed.ts (extension) — helpers re-use the same ClientHandle
// shape from api-client.ts. String input loads the persistent fixture from
// `.fixture-jwts.json`; `{ sessionJwt }` input uses a direct JWT.
export async function getDefaultWorkspaceId(handle: ClientHandle): Promise<string>;

// fixtures/install-ui.ts — browser-driven install helper used by both
// full-lifecycle scenarios.
export async function installViaUI(page: Page, name: string): Promise<string>;

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

**No migration needed at existing callers.** Because `ClientHandle = FixtureKey | { sessionJwt }` includes the bare `FixtureKey` string, every existing `getDefaultWorkspaceId(FIXTURE_KEY.regular)` call continues to typecheck unchanged. RULE NLG is satisfied by the single widened signature — there is no parallel signature, no overload, no compat shim. Scenario 2 (and all existing specs) use the bare-string form against `.fixture-jwts.json`.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Clerk PROD test-mode disabled | `424242` OTP rejected when Scenario 1 attempted on PROD | Spec is `test.skip(isProdApi)` — never executes there. |
| Dashboard install redirects to `/zombies/new` or hangs after submit | App Router navigation race or Server Action failure | `installViaUI` waits for `/zombies/{id}` with `expect(page).toHaveURL`; failure includes the current URL and leaves Playwright artifacts. |
| Clerk DEV password policy tightens and rejects 32-byte base64url | Future Clerk config change | `globalSetup` fails loudly with the policy error in the body (existing `failLoud` pattern). WS-B item #9 adds a `_smoke` regression. |
| `.fixture-jwts.json` moved into Playwright's `outputDir`/`playwright-acceptance-report` by a future refactor | Refactor blast radius | WS-B item #4 vitest test fails — blocks the PR before merge. |
| Tenant pollution causes billing balance to drift below zero on a fixture | Long-running PROD accumulation | Out of scope (per Captain). When this fires, reactivate the deferred fixture-teardown design. |
| Resume button absent in step 10 | KillSwitch state-machine drift (zombied returns a new status) | `stopZombie` already asserts `data-state="parked"`. Resume helper waits for the button via `getByRole("button", { name: "Resume" })` with a timeout — fails loud, not silently. |
| Vercel deploy fires `acceptance-e2e-prod` before `app.usezombie.com` is hot | Existing Fly pre-warm gap (separate handoff) | Out of scope. Existing M64_006 retry/timeout posture continues. |

---

## Invariants

1. **No PROD-Clerk fixture user is ever created without `public_metadata.is_test_fixture = true`.** Enforced by `ensureUser` in `clerk-admin.ts` — every code path that creates a Clerk user routes through the metadata-tagging branch. The implementation PR must keep this invariant; a new code path that bypasses `ensureUser` MUST also set the tag (lint-check: grep for `clerkRequest<…>("POST", "/users"`).
2. **`.fixture-jwts.json` never appears inside a CI artifact.** Enforced by the WS-B #4 regression test plus the existing `chmod 0o600` + gitignore.
3. **Scenario 1 never runs against PROD.** Enforced at the spec level by `test.skip(isProdApi)` mirroring `signup.spec.ts:46`. The implementation PR may NOT remove this guard without same-PR documentation in `docs/AUTH.md` of the new safety story.
4. **Both full-lifecycle scenarios install through the dashboard form.** Enforced by routing both through `installViaUI`. CLI bundle coverage stays in `install-zombie-cli.spec.ts`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `signup-lifecycle.spec.ts → signup → install → observe → bill → halt` | Ephemeral signup lands on `/zombies` empty-state; auto-provisioned workspace visible; dashboard form installs a unique zombie; detail page renders Recent Activity scaffolding; `/settings/billing` shows balance card; Stop → row `parked`; Resume → row `live`; Kill → row `failed` + disabled Killed indicator on detail page. Currently skipped on Clerk DEV Turnstile blocker. |
| `login-install-lifecycle.spec.ts → persistent fixture → install → observe → bill → halt` | Persistent `regular` fixture signs in via `clerk.signIn`; dashboard form installs a unique zombie; same observation + lifecycle legs as Scenario 1. This scenario is green against `api-dev`. |
| `_smoke.spec.ts → @clerk/nextjs major pin honored` (WS-B #8) | The resolved major of `@clerk/nextjs` equals `CLERK_NEXTJS_PINNED_MAJOR` (constant in `fixtures/constants.ts`). Predicate: parse the version from `node_modules/@clerk/nextjs/package.json` (NOT from the root `package.json` range string, which would conflate `^4.x` and `^5.x` if the range starts with `^`), `semver.major(installedVersion)` === `CLERK_NEXTJS_PINNED_MAJOR`. Fails fast if a `bun install` bumps the major against the pin. |
| `_smoke.spec.ts → fixture password clears Clerk policy` (WS-B #9) | `globalSetup` provisioned both fixture users without error AND `freshPassword()` output length is ≥ 16 chars. |
| `fixtures/_jwt-cache-location.test.ts → cache stays outside Playwright dirs` (WS-B #4) | `path.resolve(".fixture-jwts.json")` does NOT start with `path.resolve("playwright-acceptance-results")` OR `path.resolve("playwright-acceptance-report")`. |
| `_smoke.spec.ts → fixture sign-in produces accepted Clerk session` (WS-B #8 part 2) | `signInAs(FIXTURE_KEY.regular)` creates a browser session accepted by `clerkMiddleware`; protected dashboard content renders without a redirect back to `/sign-in`. |

Negative tests covered by Failure Modes table; no new fixture file (`samples/fixtures/…`) needed.

Regression tests: the existing `lifecycle.spec.ts` + `kill.spec.ts` + `events.spec.ts` + `settings-billing.spec.ts` MUST continue to pass. The new helpers in `fixtures/lifecycle.ts` are additive; the implementation PR refactors the existing two specs to use them (RULE UFS) without changing behavior.

---

## Acceptance Criteria

- [ ] **MERGE GATE — not implementation gate.** Vault prerequisite met (WS-B #1 `BLOCKED_ON_workflow-wiring`): `.github/workflows/deploy-dev.yml` + `.github/workflows/smoke-post-deploy.yml` set `AUTH_E2E_REGULAR_EMAIL` + `AUTH_E2E_ADMIN_EMAIL` from `op://ZMB_CD_{DEV,PROD}/e2e-fixtures-email/{regular,admin}`, and the most recent CI `globalSetup` log line resolves a non-mailinator domain. Workflows are a parallel handoff (separate agent's territory, recorded in Out of Scope); this PR does NOT land that wiring. The implementation PR MUST NOT merge until the workflow-wiring agent's PR lands first.
- [x] WS-A finding recorded in `docs/AUTH.md` "PROD fixture identity carve-out" — `docs/AUTH.md:397` documents the `PATCH /v1/users/{id}` `password_enabled` silent no-op finding.
- [x] WS-B vulnerability table — open-gaps subset recorded in `docs/AUTH.md` "Known gaps" (lines 407–412). Closed-in-this-PR rows (#4, #7, #8, #9, #11) are tracked by the diff itself rather than as "gaps"; closed gaps are not gaps. Variance from the literal `≥ 11` grep is intentional and surfaced here.
- [x] `signup-lifecycle.spec.ts` green via direct Clerk SDK signup — `fixtures/signup.ts` drives `Clerk.client.signUp.create` + `prepareEmailAddressVerification` + `attemptEmailAddressVerification` + `setActive`, bypassing the Cloudflare Turnstile widget while still exercising OTP validation. The original "skip on Turnstile" criterion is obsoleted by this path.
- [x] `signup-lifecycle.spec.ts` is skipped against PROD — `signup-lifecycle.spec.ts:48` carries `test.skip(isProdApi, "Scenario 1 only runs against DEV/local — Clerk test mode is DEV-only")`, matching `signup.spec.ts` posture.
- [x] `login-install-lifecycle.spec.ts` passes against `api-dev` — last green subset run (12 specs, 20/20, 4.2m) included it.
- [x] `installViaUI` drives `/zombies/new` and waits with URL polling — `install-ui.ts:56` + `:69` use `expect(page).toHaveURL(/\/zombies\/(?!new)…/)`.
- [x] `fixtures/lifecycle.ts` helpers replace duplicated `getByRole("alertdialog")` code in `lifecycle.spec.ts` AND `kill.spec.ts` — `grep -c 'getByRole.*alertdialog' …` returns `0:0`.
- [x] WS-B #4 vitest regression passes — `ui/packages/app/tests/fixture-jwt-cache-location.test.ts` (renamed from `_jwt-cache-location.test.ts` for vitest discovery) asserts `.fixture-jwts.json` is not under Playwright's `outputDir` or report dir. Runs in `make test` (379/379).
- [x] No NEW file added or modified by this branch crosses the 350-line cap — `wc -l` on the diff's non-`*.md` files; the only entries over 350 are pre-existing test files (`approvals-list.test.ts`, `dashboard-coverage.test.ts`, `events-components.test.ts`, `zombies.test.ts`) whose line counts did not increase past their pre-branch values (zombies actually shrank 987 → 973). LENGTH GATE fires only on net-add-past-cap; no violation introduced.
- [x] `gitleaks detect` clean — `1767 commits scanned. no leaks found.`
- [x] `make lint` clean — see Verification Evidence.
- [x] Existing `lifecycle.spec.ts` + `kill.spec.ts` + `events.spec.ts` + `settings-billing.spec.ts` still pass — same green subset run; `kill.spec.ts` + `lifecycle.spec.ts` refactored onto `fixtures/lifecycle.ts`.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: New specs run locally (requires local zombied stack up + DEV op:// creds)
cd ui/packages/app && bun run test:e2e:acceptance:local -- \
  tests/e2e/acceptance/signup-lifecycle.spec.ts \
  tests/e2e/acceptance/login-install-lifecycle.spec.ts

# E2: Existing specs still pass (regression)
cd ui/packages/app && bun run test:e2e:acceptance:local

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
grep -c 'getByRole.*alertdialog' ui/packages/app/tests/e2e/acceptance/{lifecycle,kill}.spec.ts
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
6. **`UZ-INTERNAL-002` ConnectionBusy on DELETE for killed zombies.** Observed against `api-dev` while wiring `workspace-zombie-lifecycle.spec.ts`. Every UI-driven DELETE on a killed row returns 500 with `UZ-INTERNAL-002`; the same surface also breaks `cleanWorkspaceZombies` accumulation (test expects 20 rows, sees 45+). The full-lifecycle UI-delete assertion is therefore intentionally deferred in the new spec — kill is asserted, delete is not. The fix lives in zombied's DELETE handler / connection-pool path, not the dashboard; a follow-up `fix(zombie): UZ-INTERNAL-002 on DELETE killed` PR re-enables the deferred branch by graduating `expectRowState(failed)` to `<row absent>`.
7. **`<ErrorBanner />` design-system primitive.** WS-G's helper returns title/body/code as parts and call sites mount them into existing `Alert` / `ConfirmDialog.errorMessage`. A small primitive wrapping the three slots into one component would let consumers stop hand-rolling the layout; the helper's return shape was deliberately built to feed such a primitive without breaking changes.
8. **Pre-existing design-system Tooltip test flake (resolved here).** Was failing with `Invalid Chai property: toBeInTheDocument` because `@testing-library/jest-dom` matchers weren't registered into vitest. Fixed in WS-G's diff because the unit-test gate had to pass on the rest of M65_001's surface; the fix is a one-line `expect.extend(matchers)` in the design-system vitest setup.

---

## Branch + PR conventions for this spec PR

- Branch: `chore/m65-001-spec-auth-e2e-lifecycle-scenarios` (off `main`).
- Single commit: `chore(spec): add M65_001 — auth e2e full lifecycle scenarios + vulnerability audit`.
- PR title: `chore(spec): M65_001 — auth e2e full lifecycle scenarios + vulnerability audit`.
- PR body links: this spec file, `docs/v2/done/M64_006_…`, `docs/AUTH.md` "PROD fixture identity carve-out" anchor, plus the WS-A finding summary.
- No `/review` skill chain on this PR — the chain runs on the implementation PR per the table above.
- Captain inspects, prioritises, and opens the implementation milestone separately.
