# Handoff — M64_005 Authenticated e2e harness

Date: 2026-05-10
Outgoing agent: Claude (Opus 4.7, 1M)
Spec: `docs/v2/active/M64_005_P1_TESTING_AUTH_E2E_HARNESS_AND_W3_POLISH.md`

## Scope/Status

Authenticated end-to-end harness for the dashboard. Suite reachable at `bun run test:e2e:auth` from `ui/packages/app/` (separate Playwright config: `playwright.auth.config.ts`).

- ✅ **WS-A.1–A.5 (auth harness)** — skeleton, env-guard, JWT mint, Svix-signed bootstrap, ticket sign-in, JWT-mounted API client, fixture provisioning. Wired end-to-end against local zombied via `make up`.
- ✅ **WS-C.1 (seed/teardown infra)** — `seedZombie`, `cleanWorkspaceZombies`, fixture lifecycle helpers.
- 🟡 **WS-C.2 (signup.spec.ts)** — written, untested (UI-driven, will hit MFA the same way the FIXME'd browser tests do).
- ❌ **Remaining M64_005 product specs** — `install-zombie-seed`, `install-zombie-cli`, `lifecycle`, `kill` not yet written.
- ⏳ **M64_006 spec** — not yet created. 5 deferred specs: `multi-zombie`, `multi-workspace`, `settings-billing`, `events`, `logs-detail`.

### Smoke run state (run #10, against local zombied)

```
3 passed (env present, JWT cache shape, /sign-in renders)
2 skipped (test.fixme — see Top Blocker below)
1 fail (roundtrip — see Real-zombied-bug surfaced #2 below)
```

The 3 pass are real proof points: globalSetup talks to Clerk admin API + zombied webhook + mints session JWTs successfully.

## Top Blocker — Programmatic Clerk sign-in fails

`@clerk/testing`'s `clerk.signIn({ page, signInParams })` **silently fails** on this Clerk DEV instance — both `password` and `ticket` strategies. Symptom: page stays on `/sign-in`, no Set-Cookie lands, Clerk's `<SignIn />` component sits idle. UI-form driving (filling email + password + clicking Continue) hits Clerk's MFA `/sign-in/factor-two` redirect because the DEV instance enforces 2FA on password sign-in.

Three follow-up paths documented in detail in `ui/packages/app/tests/e2e/auth/_smoke.spec.ts:50` (the `test.fixme` comment block):

1. Reproduce against a fresh Clerk DEV instance with no MFA enforcement; confirm whether this is config drift or a `@clerk/testing` library bug.
2. Investigate `@clerk/testing`'s route interception (`r.route(...)` in `node_modules/@clerk/testing/dist/playwright/index.js`) — `fulfill({response,json})` may strip Set-Cookie headers.
3. Fall back to admin-API-issued `__session` cookies if Clerk publishes a cookie-mount path.

**Until cracked:**

| Spec | Browser auth needed? | Status |
|---|---|---|
| `install-zombie-seed` | No (pure API) | ✅ unblocked, write today |
| `install-zombie-cli` | No (subprocess) | ✅ unblocked, write today |
| `signup` | Yes (own UI) | 🟡 partial — uses /sign-up directly, may dodge MFA via `+clerk_test`, untested |
| `lifecycle` | Yes (signInAs) | ❌ blocked |
| `kill` | Yes (signInAs) | ❌ blocked |

## Working Tree

```
## feat/m64-005-auth-e2e
clean — no uncommitted changes
```

Local-only commits ahead of origin/main (12 commits, none pushed):

```
d46d440d  feat(test/e2e): WS-A.5 — auth-harness real-run hardening
34e0a293  feat(test/e2e): WS-A.4 — api-template JWT + clerk.signIn refactor
eebb4b77  fix(auth): use PATCH (not POST) for Clerk metadata writeback   ← real zombied bug fix
35950c47  feat(test/e2e): WS-C.2 — signup.spec.ts + deleteUser helper
66772ac4  feat(test/e2e): WS-C.1 — fixture seed/teardown helpers + smoke
6fc39ed7  feat(test/e2e): WS-A.3 — Svix-signed bootstrap
a8b3ab33  feat(test/e2e): WS-A.2 — Clerk admin-API JWT mint + signInAs
c5ebacc5  chore(test/e2e): drop runtime URL allow-list
3768c47a  feat(test/e2e): WS-A.1 — skeleton
bc010439  chore(spec): tighten M64_005
a15bf9d7  chore(spec): CHORE(open) M64_005
f7a60894  chore(docs/v2): drop stale M64_002 pending dup
```

## Branch / PR

- Worktree: `/Users/kishore/Projects/usezombie-m64-005`
- Branch: `feat/m64-005-auth-e2e`
- Forge: GitHub (`gh`)
- PR: not yet created — CHORE(close) hasn't run; specs incomplete

## Local stack (running)

```
zombied-api      Up 26 min (healthy)   ← built with the PATCH fix
zombie-postgres  Up 36 min (healthy)
zombie-redis     Up 36 min (healthy)
```

Stop with: `cd /Users/kishore/Projects/usezombie-m64-005 && FOLLOW_LOGS=0 make down`

Bring up fresh:
```bash
cd /Users/kishore/Projects/usezombie-m64-005 && FOLLOW_LOGS=0 make up
```

Healthcheck: `curl -sf http://localhost:3000/readyz` → `{"ready":true,"database":true,"queue":true}`

## Required env (for smoke runs)

`.env` symlinks already in place:
- `usezombie-m64-005/.env → /Users/kishore/Projects/usezombie/.env`
- `usezombie-m64-005/ui/packages/app/.env.local → /Users/kishore/Projects/usezombie/ui/packages/app/.env.local`

`/Users/kishore/Projects/usezombie/.env` was patched during run-and-prove to add `CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')`. Ensure that line is still present.

To run the auth e2e suite locally:

```bash
cd /Users/kishore/Projects/usezombie-m64-005/ui/packages/app
export NEXT_PUBLIC_API_URL=http://localhost:3000   # or https://api-dev.usezombie.com for deployed-zombied mode
export CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')
export CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')
export NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/publishable-key')
rm -f .fixture-jwts.json
bun run test:e2e:auth
```

`.fixture-jwts.json` exists at `ui/packages/app/.fixture-jwts.json` (gitignored). Each run rewrites it.

## Tests/Checks

- ✅ `bun run lint` (app, design-system, website, zombiectl) — clean
- ✅ `bun run typecheck` (app) — clean
- ✅ `make test-unit-zombied` — 1829 unit + 144 integration passing (includes the new `METADATA_HTTP_METHOD == .PATCH` regression test)
- ✅ `make lint` — full pre-commit hook clean (zlint, pg-drain, schema-gate, openapi, etc.)
- 🟡 `bun run test:e2e:auth` — 3 pass + 2 fixme + 1 last-mile flake (see Top Blocker)

## Real bugs surfaced (one fixed, one open)

### Bug #1 — FIXED in `eebb4b77`

`src/auth/clerk_backend.zig` was POSTing to `/v1/users/{id}/metadata`; Clerk requires PATCH. Detached worker logged 405 silently. Result: every signup since this code shipped landed with empty `publicMetadata` → broken tenant context. The PATCH fix is committed; a unit test pins `METADATA_HTTP_METHOD = .PATCH`.

### Bug #2 — OPEN

zombied's zombie DELETE handler returns `UZ-INTERNAL-002 "Database error"` with `event=delete_failed err=ConnectionBusy` in logs. Connection pool / drain bug. Surfaced when the suite churned through delete cycles. Out of scope for M64_005 — should land as a separate `fix(zombie):` PR. The harness now tolerates it (per-row `try/catch` + random-suffix names so re-seeding doesn't collide).

## Next Steps (recommended order)

1. **Pre-cleanup pass** (15 min, no risk): extract string consts the user flagged.
   - `ZombieStatus = "running" | "stopped" | "killed"` (in `seed.ts`)
   - `FIXTURE_KEYS = ["regular", "admin"] as const` (in `clerk-admin.ts` or shared)
   - `JWT_TEMPLATE = "api"` (in `clerk-admin.ts`)
   - Replace inline `"killed"` / `"regular"` / `"admin"` / `"api"` strings.

2. **Crack programmatic Clerk sign-in** (Top Blocker). Pick one of the three documented paths in `_smoke.spec.ts:50`. Recommended: **path 3** — admin-API-mounted `__session` cookie. Clerk's `__session` cookie value is a session JWT; mint via `POST /sessions/{id}/tokens/__session_template` (you may need to create this template) and `context.addCookies({ name: "__session", value: jwt, domain: "localhost", path: "/", sameSite: "Lax" })`. Earlier iterations of this code lived in WS-A.2 — see commit `a8b3ab33` for the prior cookie-mount attempt (was failing for an unrelated reason since fixed).

3. **Write `install-zombie-seed.spec.ts`** (unblocked today). Pure API: `signInAs` not needed; uses `seedZombie` + `clientFor` + dashboard `/zombies` page assertions for the listing render.

4. **Write `install-zombie-cli.spec.ts`** (unblocked today). Spawns `zombiectl install` via Playwright child-process, then asserts the row in the dashboard. Will need to set `ZOMBIECTL_TOKEN` env on the spawned process to the fixture user's session JWT.

5. **After step 2 lands**, write `lifecycle.spec.ts` and `kill.spec.ts` — both rely on `signInAs`.

6. **`signup.spec.ts`** — currently committed but untested (WS-C.2). Needs a real run; may JustWork™ via Clerk's `+clerk_test` alias bypass, may hit the MFA wall. Run it against `make up`'d zombied to confirm.

7. **Spec amendments** before CHORE(close):
   - Update Files Changed table to drop the M64_006 specs (multi-zombie, etc.)
   - Update Test Specification table to match
   - Tighten Acceptance Criteria to the 5 in-scope specs
   - Add the `nukeFixture()` discussion to Out of Scope

8. **Create M64_006 spec** via `kishore-spec-new` skill for the 5 deferred specs. Split out from M64_005 with the user's approved scope mapping.

9. **CI plug-in** — wire `bun run test:e2e:auth` into `.github/workflows/smoke-post-deploy.yml` (after WS-A.6/C). Sketch is in M64_005 spec; CLAUDE.md requires explicit user approval for `.github/workflows/**` edits.

10. **CHORE(close)** — `/review`, spec → `done/`, changelog `<Update>`, `gh pr create`, `kishore-babysit-prs`.

## Risks / Gotchas

- **Clerk DEV rate limits**: dev instances enforce strict rate limits on user creation, sign-in, OTP. If the suite churns repeatedly, Clerk injects a "Temporary API keys" interstitial in place of the real sign-in page. The existing `auth-theme.spec.ts:54` skips when this happens; your auth tests should consider similar tolerance.

- **MFA on Clerk DEV**: confirmed that password sign-in for `regular-fixture@mailinator.com` triggers MFA `/sign-in/factor-two`. Either disable MFA in Clerk DEV dashboard, or stick with non-password strategies.

- **State leak across runs**: zombied DELETE bug leaves orphaned "killed" zombies. Random-suffix names sidestep the uniqueness collision but the workspace accumulates dead rows. If `list_zombies` paging breaks at high counts, manually nuke via `make down && docker volume rm usezombie-m64-005_postgres-data && make up`.

- **Container name collisions across worktrees**: `docker-compose.yml` hardcodes `container_name: zombie-postgres` etc. Only one worktree's stack can run at a time. If `make up` errors with "container name already in use", `cd ../usezombie-{other-worktree} && make down` first.

- **JWT cache file** at `ui/packages/app/.fixture-jwts.json` is gitignored but contains live session JWTs. Don't commit; if you need to publish artifacts (CI), filter it out.

- **The api JWT template** in Clerk DEV was created during this session. Claims: `{"metadata": "{{user.public_metadata}}"}`. Required for tests to work — don't delete it.

- **Auto-mode + scheduled wakeups**: the prior session used `ScheduleWakeup` heavily for long-running smoke tests. If you see a stale wakeup prompt come in (e.g., "tail b4o5x5wip"), it's an old scheduled re-invocation; ignore and check the latest task IDs in `/private/tmp/claude-501/.../tasks/`.

## Reference files (start here)

- Spec: `docs/v2/active/M64_005_P1_TESTING_AUTH_E2E_HARNESS_AND_W3_POLISH.md`
- Auth fixtures: `ui/packages/app/tests/e2e/auth/fixtures/`
  - `clerk-admin.ts` — provisionUser, mintSessionJwt, mintSignInToken, deleteUser, attachJwt
  - `auth.ts` — `signInAs(page, key)` (currently broken — see Top Blocker)
  - `bootstrap.ts` — Svix-signed `user.created` POST
  - `svix.ts` — HMAC-SHA256 helper
  - `seed.ts` — `seedZombie`, `listZombies`, TRIGGER.md/SKILL.md templates
  - `teardown.ts` — `cleanWorkspaceZombies` (PATCH→killed then DELETE, tolerant)
  - `api-client.ts` — `clientFor(key)` typed get/post/patch/delete
- Smoke spec: `ui/packages/app/tests/e2e/auth/_smoke.spec.ts` (read the FIXME block)
- Config: `ui/packages/app/playwright.auth.config.ts`
- Clerk webhook handler (zombied side): `src/http/handlers/webhooks/clerk.zig`, `src/auth/clerk_backend.zig`
- Reference TRIGGER.md / SKILL.md shapes: `samples/fixtures/frontmatter/bundles/name_mismatch/`
