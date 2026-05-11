<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_006: Auth e2e continuation + W3 carry-over

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 006
**Date:** May 11, 2026
**Status:** PENDING
**Priority:** P1 — M64_005 landed the auth harness (admin-mint cookie-direct sign-in) plus the install-{seed,cli} specs. The five lifecycle/dashboard specs the original plan called for and the W3 polish carry-over (RadioGroup primitive + zombiectl/website coverage uplift) are blocked on one piece of plumbing: making Clerk's client-side SDK aware of the fixture session. Until that's solved, every dashboard-interactive spec stays fixme. P1 (not P0) because the harness itself ships in M64_005 with clear FIXMEs; this milestone unblocks the deferred coverage and ships the W3 polish.
**Categories:** TESTING
**Batch:** B1 — depends only on M64_005 merged. No earlier work in this milestone gates it.
**Branch:** TBD (set at CHORE(open))
**Depends on:** M64_005 DONE; the existing cookie-mount harness in `ui/packages/app/tests/e2e/auth/`.

**Canonical architecture:** `docs/AUTH.md` — Pattern 2 two-token model. M64_005 landed the SSR half; this milestone unblocks the client-interactive half.

---

## Implementing agent — read these first

1. `docs/v2/done/M64_005_P1_TESTING_AUTH_E2E_HARNESS_AND_W3_POLISH.md` — context for what shipped and the "What's deferred" amendment list.
2. `docs/AUTH.md` — Token A vs Token B framing; the May 11 coherence pass clarifies which lives where.
3. `ui/packages/app/tests/e2e/auth/fixtures/auth.ts` — the three-cookie mount that already works for SSR. The new specs build on this.
4. `ui/packages/app/tests/e2e/auth/lifecycle.spec.ts` + `kill.spec.ts` — the describe-block FIXMEs name the two unblock paths; pick one in PLAN.
5. `ui/packages/app/app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` — example client-component that calls `useClientToken().getToken()`. The path this milestone unblocks runs through this hook.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `docs/AUTH.md` — DOC READ GATE on every change to `tests/e2e/auth/**` or `lib/auth/**`.
- `docs/BUN_RULES.md` — TS file-shape, const/import discipline.
- File & Function Length Gate — file ≤ 350L, fn ≤ 50L, method ≤ 70L.
- Standard set otherwise.

---

## Overview

**Goal (testable):** The Playwright auth suite covers every dashboard lifecycle that M64_005 deferred (`lifecycle`, `kill`, `multi-zombie`, `multi-workspace`, `settings-billing`, `events`, `logs-detail`) plus `signup` driven through Clerk DEV's verification step. Every spec passes locally and against `api-dev`. The W3 polish (`<RadioGroup>` primitive + zombiectl/website 95% function coverage) ships in the same milestone.

**Problem:** M64_005's cookie-mount fools `clerkMiddleware` (server side) but Clerk's in-browser SDK still reports signed-out because `/v1/client` against FAPI doesn't see the mounted cookie. Any spec that exercises a client component dispatching API calls via `useClientToken().getToken()` short-circuits because the hook returns null. Today: `lifecycle`, `kill`, `signup` are fixme. The lifecycle gap also blocks `multi-zombie`/`multi-workspace`/`events`/`logs-detail`/`settings-billing` from being written meaningfully — they all need the client-token plumbing to dispatch their own actions or load their own state.

**Solution summary:** Pick one of two unblock paths in PLAN, ship it, then write the seven specs against the now-working client-side path. In parallel, ship the W3 polish (`<RadioGroup>` primitive + the four targeted zombiectl unit tests).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` | REBUILD (if path 2 chosen) | Server-action refactor so token comes from `getServerToken` instead of `useAuth`. Out-of-scope if path 1 chosen. |
| `ui/packages/app/tests/e2e/auth/fixtures/auth.ts` | EDIT (if path 1 chosen) | Wire `@clerk/testing`'s `setupClerkTestingToken` + bot-protection bypass + reliable `clerk.signIn`. Out-of-scope if path 2 chosen. |
| `ui/packages/app/tests/e2e/auth/lifecycle.spec.ts` | EDIT | Drop `test.fixme`. Spec body already correct. |
| `ui/packages/app/tests/e2e/auth/kill.spec.ts` | EDIT | Drop `test.fixme`. Spec body already correct. |
| `ui/packages/app/tests/e2e/auth/signup.spec.ts` | EDIT | Drop `test.fixme`. Drive Clerk DEV's verification screen (read OTP from mailinator or use Clerk testing OTP "424242"). |
| `ui/packages/app/tests/e2e/auth/multi-zombie.spec.ts` | NEW | Seed 6 active zombies → exactly 5 animate (`data-live`) + 1 static glow + header reads "6 live" with single brand-pulse. The pulse-cap contract. |
| `ui/packages/app/tests/e2e/auth/multi-workspace.spec.ts` | NEW | Seed zombies in 2 workspaces (admin fixture has memberships in both) → WorkspaceSwitcher click → assert list re-renders → URL stays `/zombies` (workspace cookie change, not route). |
| `ui/packages/app/tests/e2e/auth/settings-billing.spec.ts` | NEW | `/settings/billing` → balance renders with `tabular-nums`. (Plan-tier badge dropped per M65_001.) |
| `ui/packages/app/tests/e2e/auth/events.spec.ts` | NEW | Trigger via `POST /v1/webhooks/{zombie_id}` → poll `/events` → row appears with timestamp + zombie name. |
| `ui/packages/app/tests/e2e/auth/logs-detail.spec.ts` | NEW | `/zombies/[id]` event-row click → `<Dialog>` opens with payload preview; `<WakePulse>` on active header still pulses. |
| `ui/packages/design-system/src/design-system/RadioGroup.tsx` | NEW (Workstream B) | Replace the raw `<input type="radio">` in `ModeRadio.tsx`. Wraps Radix RadioGroup with the design-system focus-ring + token map. |
| `ui/packages/design-system/src/design-system/RadioGroup.test.tsx` | NEW | Keyboard arrow nav, `data-state="checked"`, controlled + uncontrolled variants. |
| `ui/packages/design-system/src/index.ts` | EDIT | Export `RadioGroup` + `RadioGroupItem` + types. |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ModeRadio.tsx` | REBUILD | Consume the new `<RadioGroup>` primitive; drop raw `<input type="radio">`. |
| `zombiectl/test/cli-dispatch.unit.test.js` | NEW (Workstream D) | Cover `cli.js` route-dispatch arrow handlers — currently 50% function coverage. |
| `zombiectl/test/zombie-steer-fallback.unit.test.js` | NEW (Workstream D) | Cover `zombie_steer.js` poll-fallback + helpers. |
| `zombiectl/test/workspace-helpers.unit.test.js` | NEW (Workstream D) | Cover `workspace.js` uncovered branches. |
| `zombiectl/bunfig.toml` | EDIT (Workstream D) | Lift `coverageThreshold` to 95% line + 95% function after the Workstream-D writes land. |
| `.github/workflows/deploy-dev.yml` | EDIT (gated) | Plug `bun run test:e2e:auth` into the post-deploy gate. **CLAUDE.md gates `.github/workflows/**` behind explicit user approval — confirm before editing.** |

---

## Workstreams

### Workstream A — pick the client-token path (the blocker)

Two roads. PLAN picks one based on which is easier from current state; the chosen path is the prerequisite for every spec in WS-C.

**Path 1 — `@clerk/testing` clerk.signIn becomes reliable.** Requires either re-validating against a fresh Clerk DEV instance (rules out config drift) OR landing a patch upstream / monkey-patching the route interceptor in `chunk-RFJIERBJ.mjs` so Set-Cookie headers survive `route.fulfill`. Lower-risk on the dashboard side, higher-risk on dependency churn.

**Path 2 — KillSwitch + ZombieConfig + every other client-component token consumer move to server actions.** Token comes from `getServerToken()` (already api-template-aware after M64_005). `useClientToken` shrinks to clients that genuinely need a token in-browser (e.g., SSE event-stream subscribers). Touches more files but eliminates the Clerk-SDK-state dependency for the test surface.

PLAN picks one; the other workstreams can start as soon as the chosen path is green.

### Workstream B — RadioGroup primitive (W3 carry-over)

Independent of WS-A. Replaces the last raw `<input type="radio">` left in `ui/packages/app/**`. Wraps Radix RadioGroup with the design-system focus-ring + token map.

### Workstream C — seven specs

`lifecycle`, `kill`, `signup`, `multi-zombie`, `multi-workspace`, `settings-billing`, `events`, `logs-detail`. All depend on WS-A.

### Workstream D — zombiectl + website coverage uplift (W3 carry-over)

Independent of WS-A and WS-B. The three new zombiectl unit-test files lift `cli.js`/`zombie_steer.js`/`workspace.js` to ≥95% function. Then `bunfig.toml` threshold raises to match. Website coverage is already at 98%+ from M64_004 — this workstream verifies and adds a regression guard.

---

## Failure Modes & Invariants

| Mode | What goes wrong | How the harness catches it |
|------|-----------------|----------------------------|
| Path-1 drift | `@clerk/testing` upstream regresses again | `_smoke.spec.ts:51` (signInAs cookie-mount path) keeps working independently; the new specs depend on path-1 *or* path-2 plumbing, whichever was chosen — re-run baseline + a single client-action spec on CI per PR. |
| Path-2 token-staleness | Server actions cache the api-template JWT past its 1h TTL | Token is re-derived inside each action; no module-level cache. Action's catch on 401 maps to a UI refresh prompt. |
| Pulse-cap regression | Future change lifts the 5-simultaneous cap and dashboard re-introduces rave-mode | `multi-zombie.spec.ts` asserts exactly 5 `data-live="true"` rows + one static glow when 6 zombies are live. |
| Coverage regression | Future PR drops a test file and zombiectl coverage falls back | `bunfig.toml` `coverageThreshold` gates every CI run. |

**Architectural invariant:** server actions never bypass the existing tenant-context middleware. The api-template JWT carries `metadata.tenant_id`; the action must surface that to zombied via Bearer, never via a cookie or a custom header.

---

## Test Specification

| Test | Asserts |
|------|---------|
| lifecycle.spec.ts | Stop → confirm → `/zombies` row data-state="parked", dashboard stat-row Live count decrements by 1 |
| kill.spec.ts | Kill → confirm → `/zombies` row data-state="failed", detail page shows disabled "Killed" indicator |
| signup.spec.ts | UI signup with mailinator alias OR Clerk testing OTP → dashboard → zero-state visible + balance $5 |
| multi-zombie.spec.ts | 6 active zombies → 5 `data-live="true"` + 1 static glow + "6 live" header pulse |
| multi-workspace.spec.ts | Admin fixture switches workspace → list re-renders → URL stays `/zombies` |
| settings-billing.spec.ts | `/settings/billing` → balance renders with `tabular-nums` |
| events.spec.ts | Manual webhook → `/events` poll → row appears with timestamp + zombie name |
| logs-detail.spec.ts | Event-row click → `<Dialog>` payload preview; header `<WakePulse>` still pulses |
| RadioGroup.test.tsx (B) | Keyboard arrow nav cycles, `data-state="checked"`, controlled + uncontrolled |
| zombiectl cli-dispatch.unit | Every `handlers[route.key]` arrow callable with mock deps; route → handler arity guard |
| zombiectl zombie-steer-fallback | `eventIdToSince`, `isTerminal`, `buildBearer` cover all branches; `pollEventTerminal` covers timeout + match + error paths |
| zombiectl workspace-helpers | The 4 uncovered branches (lines 89-92, 118-119, 123-124, 199-200) |

---

## Acceptance Criteria

- `bun run test:e2e:auth:local` passes locally with 0 fixme, all eight Workstream-C specs green.
- Same suite passes against `api-dev` from CI (the `.github/workflows/deploy-dev.yml` plug-in, gated by user approval).
- `bun test --coverage` (zombiectl) reports ≥95% function and ≥95% line; `bunfig.toml` threshold updated to match.
- `bun run test:coverage` (website) reports ≥95% function; `vite.config.ts` threshold confirmed.
- `<RadioGroup>` primitive ships in `@usezombie/design-system`; `ModeRadio` consumes it; no raw `<input type="radio">` left in `ui/packages/app/**`.
- M64_005's three `test.fixme` blocks (`lifecycle`, `kill`, `signup`) are gone.

---

## Out of Scope

- Stripe purchase flow tests (deferred to v2.1; existing comment in `BillingBalanceCard` stands).
- Mobile e2e at 375px (defer to a perf milestone).
- Visual regression screenshots (separate concern).
- The `fix(zombie)` pool-init-after-migrations startup-order tightening that M64_005 surfaced — separate PR, not gated on this milestone.
- The `fix(zombie)` DELETE ConnectionBusy bug — separate PR; `_smoke.spec.ts` test 6 already tolerates it via the "killed-OR-gone" assertion.

---

## Discovery (out-of-scope but adjacent observations the agent SHOULD surface)

- If WS-A path 2 (server actions) is chosen, the same refactor pattern applies to every `useAuth().getToken()` call site listed in M64_005's `fix(auth)` commit — picking off a few per PR keeps blast radius small.
- `setupClerkTestingToken` (path 1) intercepts FAPI requests at the Playwright route layer. If a future spec drives Clerk's hosted UI for OAuth (GitHub sign-in), the interceptor needs to allow-list those upstream calls or the OAuth dance breaks.

---

## Implementation Notes

- Path-1 / Path-2 decision must be the first paragraph of PLAN. The wrong call burns a workstream of effort.
- The `signup` driver should prefer Clerk's documented testing OTP (`424242`) over mailinator polling — faster, no DNS dependency.
- The `multi-workspace` spec uses the `admin` fixture (membership in both fixtures' tenants per M64_005 WS-B). If admin's memberships drift, the spec fails the same way every time — keep it as a smoke for the membership wiring.
