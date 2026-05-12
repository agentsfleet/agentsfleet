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

# M65_002: zombiectl e2e — full lifecycle scenarios against live DEV + PROD

**Prototype:** v2.0.0
**Milestone:** M65
**Workstream:** 002
**Date:** May 12, 2026
**Status:** PENDING
**Priority:** P1 — M65_001 ships dashboard acceptance gates on every deploy. The published CLI (`@usezombie/zombiectl`) has no equivalent: today's `zombiectl/test/**` files are unit + mock-API integration runs. A regression in the CLI's auth handoff, install, or lifecycle path against the real `api-dev`/`api` ships to npm undetected until a user reports it. This spec adds the same shape of gate for the CLI surface — DEV against the worktree binary on every backend deploy, PROD against the just-published npm tarball on every release.
**Categories:** TESTING, SECURITY
**Batch:** B1 — no parallel workstreams in M65_002. Sequenced after M65_001 because it consumes the same `op://VAULT/e2e-fixtures/{regular,admin}/email` vault items and the same `regular` Clerk fixture identity.
**Branch:** TBD — opens at CHORE(open) on the implementation milestone (this spec is the planning gate).
**Depends on:** M65_001 (vault-resolved fixture emails + persistent `regular` fixture in both Clerk DEV and Clerk PROD). **Hard merge-gate:** same as M65_001 — `op://VAULT/e2e-fixtures/{regular,admin}/email` MUST resolve to non-mailinator domains AND the workflow `env:` blocks MUST consume them. The CLI suite re-uses both invariants.

**Canonical architecture:** `docs/AUTH.md` §"Test infrastructure — e2e fixture mint (admin path)" + §"PROD fixture identity carve-out". Sibling spec: `docs/v2/pending/M65_001_P1_TESTING_SECURITY_AUTH_E2E_FULL_LIFECYCLE_SCENARIOS.md`.

---

## Implementing agent — read these first

1. `docs/v2/pending/M65_001_P1_TESTING_SECURITY_AUTH_E2E_FULL_LIFECYCLE_SCENARIOS.md` — sister spec on the dashboard side. WS-A (password-disable viability), WS-B (vulnerability audit), and the PROD-fixture-identity carve-out are referenced; do not re-audit those rows.
2. `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` — the dashboard's full-lifecycle spec. The CLI scenario mirrors its post-auth flow: install → observe → bill → stop → resume → kill. Same persistent `regular` fixture, same `samples/platform-ops/{SKILL,TRIGGER}.md` bundle, same Clerk identity. Do NOT import from `ui/packages/app/tests/e2e/acceptance/` — that suite is owned by another agent. Read for reference, re-implement in JS for the CLI suite.
3. `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — the 3-phase admin-mint chain (`provisionUser` → `bootstrapTenant` → `attachJwt`). The CLI suite re-implements the **minimal** subset against the Clerk Backend API in plain JS. Use the same endpoints, the same `is_test_fixture` metadata tag, the same `expires_in_seconds` posture (carried forward from WS-B #11 in M65_001).
4. `zombiectl/src/cli.js` — env-var auth resolution: `resolvedToken = creds.token || env.ZOMBIE_TOKEN || null` (`src/cli.js:65`). The lifecycle spec injects a Clerk-minted session JWT via `ZOMBIE_TOKEN` and bypasses `zombiectl login` entirely. The login-flow spec drives the real `login` command end-to-end.
5. `zombiectl/test/helpers-cli-state.js` — `withFreshStateDir` + `withAuthedStateDir` patterns. The acceptance suite re-uses `withFreshStateDir` verbatim (per-test temp `ZOMBIE_STATE_DIR`); `withAuthedStateDir` is bypassed because the acceptance suite mints real JWTs instead of stubbing `credentials.json`.
6. `zombiectl/test/onboarding-flow.integration.test.js` — the canonical mock-API login test. The new `login-flow.spec.js` is its real-API sibling — same lifecycle (POST sessions → poll → write credentials.json) against live `api-dev` instead of a mock server.
7. `zombiectl/src/program/routes.js` — canonical command surface (`zombie.install` / `zombie.status` / `zombie.stop` / `zombie.resume` / `zombie.kill` / `zombie.logs`). Spec uses these route keys, not hand-spelled command strings.
8. `zombiectl/scripts/run-tests.mjs` — current test runner. The acceptance suite ships its own runner script (`scripts/run-acceptance.mjs`) gated on `ZOMBIE_ACCEPTANCE_TARGET`; the existing `bun run test` stays unit + integration only.
9. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — the canonical bundle the CLI suite hands to `zombiectl install --from`. Same bundle the dashboard suite POSTs through the API.
10. `.github/workflows/post-release.yml` — the release pipeline. `cli-acceptance-prod` lands here as a new job sequenced after the npm publish step (NOT in `smoke-post-deploy.yml` — npm is the source of truth for the prod CLI, not Vercel).
11. `.github/workflows/deploy-dev.yml` — `cli-acceptance-dev` lands here as a sibling of the existing `auth-e2e-dev` job.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Especially: RULE TST-NAM (no milestone IDs in test names), RULE UFS (centralise repeat literals), RULE TGU (test-guard), RULE WAUTH (webhook-auth shape).
- `docs/BUN_RULES.md` — diff is JS-heavy; TS FILE SHAPE DECISION applies to any new fixture file. JS files mirror the existing `zombiectl/test/**` style (default-export-free, `import { fn } from "./helpers.js"`).
- `docs/REST_API_DESIGN_GUIDELINES.md` — N/A; this spec adds NO new HTTP handlers. The CLI's existing handlers are exercised against live API.
- `docs/AUTH.md` — load-bearing. The CLI suite reads `ZOMBIE_TOKEN` from env exactly like a real operator with an exported token. Any change to that resolution path is out-of-scope here and gated by `docs/AUTH.md` carve-out.
- `docs/ZIG_RULES.md` — N/A; no Zig in this diff.
- `docs/LOGGING_STANDARD.md` — N/A; tests don't emit logs through the project logger.

---

## Anti-Patterns to Avoid (read this BEFORE drafting the spec)

Standard set from `docs/TEMPLATE.md` applies. Additionally for this spec:

- Do NOT import anything from `ui/packages/app/tests/e2e/acceptance/`. That suite is owned by another agent; cross-package imports would couple two suites and force lock-step releases.
- Do NOT inline `node:test` `it("…", () => { … })` bodies in section text. The spec names tests and asserts behavior; the implementing agent writes the assertions.
- Do NOT re-audit the dashboard-side vulnerability table from M65_001. WS-E adds rows ONLY for CLI-specific concerns surfaced by this spec.
- Do NOT propose teardown of the persistent `regular` fixture's tenant or billing balance. Same Captain deferral as M65_001.
- Do NOT propose a third CI job that runs the suite on every PR. PR-time gating is a known M65_001 deferral; this spec inherits the same disposition.

---

## Overview

**Goal (testable):** Two acceptance suites (`zombiectl/test/acceptance/login-install-lifecycle.spec.js` and `zombiectl/test/acceptance/login-flow.spec.js`) run inside the existing repo (not as a separate package), drive the published-shape `zombiectl` CLI surface against live API, and gate two new CI jobs: `cli-acceptance-dev` (post-deploy-dev, worktree binary, targets `api-dev`) and `cli-acceptance-prod` (post-release, globally-installed npm binary, targets `api`). The lifecycle scenario walks install → observe → bill → stop → resume → kill against the persistent `regular` fixture. The login-flow scenario drives the real `zombiectl login` browser-handoff end-to-end against `api-dev` only.

**Problem:**

1. `@usezombie/zombiectl` ships to npm with `bun run test` green, but `bun run test` is mock-API only. A regression in the real-network path (HTTP retry shape, auth header drift, JSON envelope drift) lands on npm undetected until a customer reports it.
2. The dashboard's acceptance suite (M65_001) proves the UI's auth handoff works against every deploy. The CLI has no equivalent — `zombiectl login` against live `api-dev` has never been exercised in CI. A future change to the dashboard's `/cli-auth/{session_id}` handoff page or the backend's `POST /v1/auth/sessions` shape could break the published CLI without showing up in any test.
3. Operators reach for the CLI on freshly-installed laptops via `npm i -g @usezombie/zombiectl`. The lifecycle of "global install → login → install zombie → observe → halt" is never run in any test today. The `postinstall.mjs` step, the auth state file mode bits, and the per-request retry posture are all proved only at the unit level against fakes.

**Solution summary:** Two new Node-test specs under `zombiectl/test/acceptance/`, opt-in via a new `bun run test:acceptance` script. The lifecycle spec mints a Clerk session JWT via the Clerk Backend API (mirroring `clerk-admin.ts`), injects it as `ZOMBIE_TOKEN`, and walks the CLI lifecycle. The login-flow spec spawns `zombiectl login --no-open` and drives the browser-handoff page via Playwright with a pre-mounted Clerk session cookie — proving the real auth handshake. Two new GH Actions jobs gate these on every DEV deploy + every npm publish. The vulnerability audit adds three CLI-specific rows; M65_001's table is referenced for the shared rows.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/test/acceptance/login-install-lifecycle.spec.js` | CREATE | Persistent-fixture login → install → observe → bill → halt against live API. Runs against DEV (worktree binary) AND PROD (npm-installed binary). |
| `zombiectl/test/acceptance/login-flow.spec.js` | CREATE | Drives `zombiectl login` end-to-end against live `api-dev` with a Playwright-driven browser handoff. DEV-only — skipped against PROD until the M65_001 vault + Clerk-PROD-test-mode conditions are met. |
| `zombiectl/test/acceptance/fixtures/clerk-admin.js` | CREATE | Minimal JS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts`. Implements `provisionUser`, `mintTokens`, `attachJwt` against the Clerk Backend API. Self-contained — no imports from the UI package. |
| `zombiectl/test/acceptance/fixtures/cli.js` | CREATE | `runZombiectl(args, { env, stdin, timeoutMs })` helper. Spawns `node ./bin/zombiectl.js` (DEV mode) or `zombiectl` from PATH (PROD mode) based on `ZOMBIE_ACCEPTANCE_BINARY` env. Per-spawn env scoping — never mutates `process.env`. |
| `zombiectl/test/acceptance/fixtures/seed.js` | CREATE | `installPlatformOpsZombie(name)` helper — reads `samples/platform-ops/{SKILL,TRIGGER}.md` from worktree root and shells out to `zombiectl install --from`. Returns parsed `{id, name, workspace_id}` from CLI's `--json` output. |
| `zombiectl/test/acceptance/fixtures/lifecycle.js` | CREATE | Shared action helpers: `stopZombie(id)`, `resumeZombie(id)`, `killZombie(id)`, `expectStatus(id, expected)`. All wrap `runZombiectl` and assert exit code + parsed JSON envelope. |
| `zombiectl/test/acceptance/fixtures/teardown.js` | CREATE | `cleanWorkspaceZombies(workspaceId)` — calls `zombiectl zombie list --json` + `zombiectl kill` for each non-terminal zombie. Used in `afterEach`. |
| `zombiectl/test/acceptance/fixtures/constants.js` | CREATE | Cross-runtime constants shared by every fixture file: `CLERK_API_BASE`, `IS_TEST_FIXTURE_METADATA_KEY`, `FIXTURE_EMAIL_VAULT_PATHS`, `PLATFORM_OPS_SAMPLE_DIR`, `LOGIN_POLL_MS`, `LOGIN_TIMEOUT_SEC`. RULE UFS — one literal, every reader. |
| `zombiectl/test/acceptance/fixtures/browser.js` | CREATE | Thin Playwright wrapper used only by `login-flow.spec.js`. Launches Chromium, mounts the `regular` fixture's Clerk session cookies, navigates to a given URL, clicks the CLI-auth approve button. Lifted from the dashboard suite's approach but re-implemented in JS — no `@playwright/test`, just `playwright`'s `chromium` API. |
| `zombiectl/test/acceptance/global-setup.js` | CREATE | Pre-suite hook: resolves Clerk admin secret + fixture vault paths, ensures the `regular` fixture user exists in the target Clerk instance, mints a session JWT, writes it to a temp file that each spec reads. Mirrors `ui/packages/app/tests/e2e/acceptance/global-setup.ts`'s shape but JS + CLI-only. |
| `zombiectl/scripts/run-acceptance.mjs` | CREATE | New runner. Iterates `test/acceptance/*.spec.js` files via `node --test`, gates on `ZOMBIE_ACCEPTANCE_TARGET` env being set (skips silently when unset so local `bun run test` is unaffected). |
| `zombiectl/package.json` | EDIT | Add `test:acceptance` script invoking `scripts/run-acceptance.mjs`. Add `playwright` as a `devDependencies` entry (browser-handoff is opt-in; only the login-flow spec imports it). |
| `zombiectl/bunfig.toml` | EDIT | Exclude `test/acceptance/` from the default `bun test` glob — acceptance tests run via `node --test` only, not as bun unit tests. |
| `.github/workflows/deploy-dev.yml` | EDIT | Add `cli-acceptance-dev` job sequenced after the existing `auth-e2e-dev`. Targets `api-dev`, runs the worktree binary, loads Clerk DEV credentials via op:// (same vault path conventions as `auth-e2e-dev`). **CLAUDE.md gates `.github/workflows/**` behind explicit user approval — the implementation PR carries the same Captain-authorization carve-out as M64_006.** |
| `.github/workflows/post-release.yml` | EDIT | Add `cli-acceptance-prod` job sequenced after the existing npm publish step. `npm i -g @usezombie/zombiectl@latest`, targets `https://api.usezombie.com`, loads Clerk PROD credentials via op://. Same `.github/workflows/**` gate. |
| `zombiectl/.gitignore` | EDIT | Add `test/acceptance/.fixture-jwt` (per-suite minted JWT file written by `global-setup.js`, mode 0600). |
| `docs/AUTH.md` | EDIT | Append a "CLI fixture identity carve-out" subsection mirroring "PROD fixture identity carve-out" — documents that `cli-acceptance-{dev,prod}` re-uses the dashboard fixture's Clerk identity, that `ZOMBIE_TOKEN` is the injection surface, and that the JWT TTL chosen here is the same one M65_001 lands. |

**Files NOT changed (explicit non-goals on this milestone):**

- `ui/packages/app/tests/e2e/acceptance/**` — another agent owns it. The CLI suite consumes the same shared fixture identity but does not import or modify anything in this tree.
- `zombiectl/src/**` — no CLI behavior changes. If the implementing agent finds a real bug while writing the suite, it lands in a separate PR.
- `zombiectl/test/*.test.js` (existing unit + integration tests) — untouched. The new acceptance suite is purely additive.
- `samples/platform-ops/**` — read-only for the suite. The CLI suite uses the same bundle the dashboard suite uses.
- `src/http/handlers/**` — N/A; CLI talks to existing handlers as-is.

---

## Sections (implementation slices)

### §1 — Acceptance harness scaffolding

Stands up the directory + runner + helpers without writing any spec body. Delivers a green `bun run test:acceptance` that exits 0 with "no specs" when `ZOMBIE_ACCEPTANCE_TARGET` is unset and exits 0 with an empty suite when it is set.

**Implementation default:** `node --test` as the runner (matches existing `*.test.js` files in `zombiectl/test/` that use `node:test`). Bun-test is rejected because the acceptance flow spawns long-lived browser processes the Bun test runner doesn't bound cleanly.

### §2 — `clerk-admin.js` JS twin

Minimal re-implementation of `provisionUser` + `mintTokens` + `attachJwt` against the Clerk Backend API. The TS source on the dashboard side stays the canonical reference; this twin lives in the CLI test tree to keep the two suites independently releasable. RULE UFS: constants (`CLERK_API_BASE`, `IS_TEST_FIXTURE_METADATA_KEY`) live in `fixtures/constants.js` and share identifiers verbatim with the dashboard suite's `fixtures/constants.ts`.

**Implementation default:** the JWT TTL the implementing agent sets in `mintTokens` MUST match whatever value M65_001's WS-B #11 lands on for the dashboard suite. If M65_001 has not yet merged, the implementation PR reads the most recent CI timing for the acceptance suites and picks 2× the observed p95 wall-clock.

### §3 — Lifecycle scenario (`login-install-lifecycle.spec.js`)

The post-auth walk. Mirrors `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` step-for-step, replacing each browser action with the equivalent CLI invocation:

| Step | Dashboard action (sibling spec) | CLI action (this spec) |
|---|---|---|
| 1 | `signInAs(page, regular)` — cookie-mount | `ZOMBIE_TOKEN=<minted JWT>` env-injection on each spawn |
| 2 | resolve default workspace via `getDefaultWorkspaceId` | `zombiectl workspace list --json` → pick first |
| 3 | `installViaUI(page, name)` — dashboard form drive | `zombiectl install --from samples/platform-ops --name <unique> --json` |
| 4 | `expect(page.getByLabel("Recent Activity")).toBeVisible()` | `zombiectl logs <id> --json --since 1m` returns a parseable envelope |
| 5 | `/zombies` row `data-state="live"` | `zombiectl status <id> --json` → `status == "active"` |
| 6 | `/settings/billing` → balance card visible | `zombiectl billing show --json` → `balance` field present |
| 7 | Stop → Resume → Kill via KillSwitch | `zombiectl stop <id>` → `zombiectl resume <id>` → `zombiectl kill <id>`, each asserting exit 0 + the next `status` |
| 8 | terminal "Killed" indicator on detail page | `zombiectl status <id> --json` → `status` ∈ `{killed, errored}`; second `zombiectl kill <id>` is idempotent (exit 0, no state change) |

**Implementation default:** unique zombie name per test via `crypto.randomBytes(4).toString("hex")` to avoid `(workspace_id, name)` collisions from interrupted prior runs (same rationale as `login-install-lifecycle.spec.ts:34`).

**Implementation default:** `cleanWorkspaceZombies(workspaceId)` in `afterEach` kills any non-terminal leftovers. Tenant + billing-balance teardown is OUT OF SCOPE (M65_001 Captain deferral inherited).

### §4 — Login-flow scenario (`login-flow.spec.js`)

The real auth handshake. DEV-only. Drives `zombiectl login --no-open --no-input --timeout-sec 60 --poll-ms 500`, parses the `login_url` from stdout, then spawns a Playwright Chromium context with the `regular` fixture's Clerk session cookies pre-mounted, navigates the browser to `login_url`, clicks the approve action on the dashboard's CLI-auth handoff page, and waits for the CLI subprocess to exit 0 with `credentials.json` written at mode 0600.

**Implementation default:** Playwright's `chromium` (not `@playwright/test`) — the spec already orchestrates the CLI subprocess; a parallel test-runner framework on top adds no value. The browser is one `chromium.launch()` per test.

**Implementation default:** the test uses `withFreshStateDir` (existing helper from `zombiectl/test/helpers-cli-state.js`) so the real Clerk JWT lands in a tmpdir-scoped `credentials.json`, never in `~/.config/zombie/`. The state dir + its contents are cleaned in `afterEach` regardless of test outcome.

### §5 — CI job wiring

`cli-acceptance-dev` (`.github/workflows/deploy-dev.yml`): sequenced after the existing `auth-e2e-dev`. Loads op:// secrets (Clerk DEV admin key + fixture-email vault items) AFTER the `npm install` / `bun install` step so a hostile postinstall has no secret context. Runs `node ./bin/zombiectl.js` against `ZOMBIE_API_URL=https://api-dev.usezombie.com`. Uploads any artifacts to `cli-acceptance-dev-${{ github.sha }}/` scoped to a `playwright-cli-report/` subdir — never the temp state dir.

`cli-acceptance-prod` (`.github/workflows/post-release.yml`): sequenced AFTER the existing npm publish step (job-level `needs:` on the publish job). Same op:// load-after-install posture. Runs `npm i -g @usezombie/zombiectl@latest` then calls `zombiectl` from PATH. Targets `ZOMBIE_API_URL=https://api.usezombie.com`. Mints a Clerk PROD session JWT for the `regular` fixture via the Clerk PROD admin key (op://-resolved).

**Implementation default:** `cli-acceptance-prod` does NOT run on every Vercel deploy; it runs on every npm release. Rationale: npm is the source of truth for the CLI surface a customer touches; Vercel deploys can ship without a CLI release and a CLI release can ship without a Vercel deploy.

**Implementation default:** add a daily cron trigger to `cli-acceptance-prod` so backend changes that ship without a CLI release still re-exercise the published CLI against live PROD once per day. Cron expression: `0 13 * * *` UTC (matches existing scheduled-run cadence in the repo's other workflows; implementing agent confirms by grepping `.github/workflows/`).

### §6 — Vulnerability audit (CLI-specific rows only)

See WS-E below.

---

## WS-E — CLI-specific vulnerability audit

M65_001's audit table (WS-B) is the canonical reference for shared rows (mailinator inbox, password-disable posture, webhook secret reuse, `.fixture-jwts.json` artifact risk, tenant pollution, PR-time gate, Clerk PROD identity carve-out, `@clerk/nextjs` major pin, `freshPassword` policy, Svix msg-id collision, JWT TTL). Those are inherited; do NOT re-disposition.

Three rows are CLI-specific and dispositioned here:

| # | Vulnerability | Sev | Current state | Proposed fix | Lands in | Disposition |
|---|---|---|---|---|---|---|
| C1 | `ZOMBIE_TOKEN` env-var lifetime in the spawned subprocess. The minted Clerk JWT is visible to `/proc/<pid>/environ` (Linux) and `ps eww` for the lifetime of every `zombiectl` child. If the spawn helper leaks the JWT into `process.env` of the test runner instead of scoping to the single `spawn` call, every subsequent spawn inherits it. If the CLI ever logs its env (it should not), the JWT lands in stdout/stderr which the test captures and CI artifacts upload. | S2 | No equivalent test today — first time a real Clerk JWT will live in CI subprocess env. | `runZombiectl(args, { env })` takes per-call env, never mutates `process.env`. Spec adds an assertion that captured stdout + stderr never contain the JWT value (substring check). CI workflow loads op:// secrets in a step AFTER `npm i` / `bun install` so postinstall scripts have no secret context. | `zombiectl/test/acceptance/fixtures/cli.js`, both spec files, `.github/workflows/{deploy-dev,post-release}.yml` | `FIX_THIS_PR`. |
| C2 | npm `postinstall` running unsandboxed during `cli-acceptance-prod`. `npm i -g @usezombie/zombiectl@latest` executes `scripts/postinstall.mjs` on a GH runner that may have Clerk PROD admin secrets loaded. Supply-chain compromise of `@usezombie/zombiectl` or `posthog-node` would execute arbitrary JS with secret context. | S2 | Today the prod CLI is never installed in CI. This spec adds the install path. | Workflow load-after-install: `op://` secrets resolve in a job step that runs AFTER `npm i -g`. Postinstall sees no Clerk admin key. Documented in `docs/AUTH.md` "CLI fixture identity carve-out" subsection. | `.github/workflows/post-release.yml`, `docs/AUTH.md` | `FIX_THIS_PR` (workflow structure) + `ACCEPTED_RISK` for any residual exposure (e.g. the GH token itself is present). |
| C3 | `credentials.json` mode + path written from a real auth flow. `login-flow.spec.js` is the first test in the codebase that drives `zombiectl login` to completion against live API; the resulting file holds a live Clerk session JWT. Risks: (a) test runner inheriting a developer's real `ZOMBIE_STATE_DIR` and overwriting their `credentials.json`; (b) the CLI's `0600` chmod is not regression-proved anywhere; (c) GH artifact uploads could include the temp state dir. | S2 | The CLI sets `0600` on save (`src/lib/state.js`). No test asserts this against the real flow. | `login-flow.spec.js` uses `withFreshStateDir` so `ZOMBIE_STATE_DIR` is scoped per-test. Spec asserts `(stat(credentials.json).mode & 0o777) === 0o600` AND that the token field parses as a 3-segment JWT. Workflow artifact `path:` is scoped to `playwright-cli-report/` only — never the temp state dir. | `zombiectl/test/acceptance/login-flow.spec.js`, `.github/workflows/deploy-dev.yml` | `FIX_THIS_PR`. |

All three rows are `FIX_THIS_PR`. No new deferred rows on this milestone.

---

## Interfaces

No new HTTP endpoints. The CLI suite exercises existing handlers via the existing CLI commands. Public surface the implementing agent must NOT change without spec amendment:

```js
// zombiectl/test/acceptance/fixtures/cli.js
export async function runZombiectl(args, opts) {
  // opts: { env?: Record<string,string>, stdin?: string|Readable, timeoutMs?: number,
  //         cwd?: string, binary?: "worktree" | "global" }
  // Returns: { code: number, stdout: string, stderr: string, durationMs: number }
  // Contract:
  //   - env is the COMPLETE child env (no merge with process.env). Caller composes.
  //   - binary defaults to env.ZOMBIE_ACCEPTANCE_BINARY (worktree | global).
  //   - Never mutates process.env.
  //   - Throws TimeoutError if the child hasn't exited by timeoutMs.
}

// zombiectl/test/acceptance/fixtures/clerk-admin.js
export async function provisionUser(clerkSecret, opts);
// opts: { email: string, password?: string, metadata?: object }
// Returns: { id, email_addresses, public_metadata }

export async function mintTokens(clerkSecret, clerkUserId, opts);
// opts: { ttlSeconds?: number }  (default: same as M65_001 WS-B #11 lands)
// Returns: { sessionJwt, cookieJwt, sessionId }

export async function attachJwt(clerkSecret, opts);
// opts: { email: string }
// Returns: { sessionJwt, cookieJwt, sessionId, clerkUserId }

// zombiectl/test/acceptance/fixtures/seed.js
export async function installPlatformOpsZombie(opts);
// opts: { env: Record<string,string>, workspaceId: string, name?: string }
// Returns: { id: string, name: string, workspace_id: string }

// zombiectl/test/acceptance/fixtures/lifecycle.js
export async function stopZombie(env, zombieId);
export async function resumeZombie(env, zombieId);
export async function killZombie(env, zombieId);
export async function expectStatus(env, zombieId, expected);
// expected: "active" | "paused" | "stopped" | "killed" | "errored"

// zombiectl/test/acceptance/fixtures/teardown.js
export async function cleanWorkspaceZombies(env, workspaceId);

// zombiectl/test/acceptance/fixtures/browser.js
export async function completeCliAuthHandoff(opts);
// opts: { loginUrl: string, clerkSessionCookies: Cookie[] }
// Returns: void (throws if the approve action fails or the page doesn't load)
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `ZOMBIE_TOKEN` missing in spawn env | Spec author forgot to thread env through `runZombiectl` | CLI's auth guard fires; child exits 1 with `not authenticated` on stderr. Test fails loud with the full stderr in the assertion message. |
| Clerk admin secret missing | Workflow op:// load step failed | `global-setup.js` fails the entire suite before any spec runs; clear error message names the missing vault path. |
| `api-dev` returns 503 mid-flight | Backend transient | CLI's built-in HTTP retry already covers transient 5xx. Test asserts on final outcome, not intermediate calls. If the entire suite times out, `cli-acceptance-dev` fails the job — the gate fires correctly. |
| `samples/platform-ops/SKILL.md` moved or renamed | Repo refactor | `installPlatformOpsZombie` throws with the resolved path in the error; suite fails fast at first test. |
| `zombiectl login` poll loop times out | `cli-acceptance-dev` browser leg failed to click approve | CLI exits 1 with `timed out` on stderr. Test asserts exit 0; failure is visible in the job log. |
| Login-flow spec leaks a real JWT into CI logs | Test author logged `credentials.json` contents | WS-E #C3 asserts that captured stdout + stderr never contain the JWT value substring (same posture as C1). |
| `cli-acceptance-prod` runs against an npm version older than expected | npm replication lag after publish | `runZombiectl(["--version"])` asserts the installed version equals `package.json`'s version. Job fails fast with a clear "stale npm" message. |
| Multi-run collisions on `(workspace_id, name)` | Prior run interrupted before teardown | Unique random name suffix per install (same as dashboard suite). `afterEach` calls `cleanWorkspaceZombies` even on test failure. |
| Tenant billing balance drift on PROD fixture | Long-running PROD accumulation across both suites | Out of scope (M65_001 Captain deferral inherited). Reactivation: see M65_001 WS-B #5 reactivation conditions. |

---

## Invariants

1. **`runZombiectl` never mutates `process.env`.** Enforced by the helper's signature — `env` is required, composed by the caller. Lint check: grep `zombiectl/test/acceptance/` for `process.env.ZOMBIE_TOKEN =` or any `delete process.env.ZOMBIE_TOKEN` → 0 matches.
2. **Real Clerk JWTs never appear in captured stdout/stderr.** Enforced by a per-test assertion in both specs: `expect(stdout + stderr).not.toContain(sessionJwt)`. The assertion runs on every `runZombiectl` call.
3. **`cli-acceptance-prod` always runs against the just-published version.** Enforced by `runZombiectl(["--version"])` asserting equality with `package.json`'s `version` field, executed as the first action in the prod suite.
4. **op:// secrets load AFTER `npm i` / `bun install` in every CLI acceptance workflow job.** Enforced by job-step ordering in the workflow YAML; reviewed by `/review` against this invariant.
5. **No spec in `zombiectl/test/acceptance/` imports anything from `ui/packages/`.** Enforced by `scripts/audit-runtime-imports.mjs` extension (or a one-line grep gate in CI): `grep -rn "from \"ui/packages" zombiectl/test/acceptance/` → 0 matches.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `login-install-lifecycle.spec.js → installs, observes, bills, and halts a platform-ops zombie` | Persistent `regular` fixture's minted JWT in `ZOMBIE_TOKEN`; `zombiectl workspace list --json` returns ≥1 workspace; `zombiectl install --from samples/platform-ops --name <unique> --json` exits 0 with parseable `{id}`; `zombiectl status <id> --json` returns `status == "active"`; `zombiectl logs <id> --json --since 1m` returns a parseable envelope (empty events array allowed); `zombiectl billing show --json` returns `balance` field; `zombiectl stop <id>` → status `paused` or `stopped`; `zombiectl resume <id>` → status `active`; `zombiectl kill <id>` → status `killed` or `errored`; second `zombiectl kill <id>` is idempotent (exit 0). |
| `login-install-lifecycle.spec.js → captured output never contains the minted JWT` | For every `runZombiectl` call in the lifecycle, `stdout + stderr` substring search for the minted JWT returns no match. WS-E #C1 regression. |
| `login-flow.spec.js → completes the real CLI auth handshake against api-dev` | `zombiectl login --no-open --no-input` emits a parseable `login_url`; browser leg navigates + clicks approve; CLI subprocess exits 0; `credentials.json` exists, mode `0600`, `token` is a 3-segment JWT. DEV-only (`test.skip` against PROD). |
| `login-flow.spec.js → credentials.json mode is 0600` | `fs.stat(credentialsPath).mode & 0o777 === 0o600`. WS-E #C3 regression. |
| `login-flow.spec.js → temp state dir scoping holds` | `ZOMBIE_STATE_DIR` is a tmpdir prefix; the resolved `credentials.json` path is inside it; `afterEach` removes the dir. Regression that protects against developer-config leaks. |
| `runZombiectl never mutates process.env` (assertion runs in-suite, not a separate test) | Before/after snapshot of `process.env.ZOMBIE_TOKEN` around every spawn is identical to the value the suite started with. WS-E #C1 regression. |
| `cli-acceptance-prod first action: zombiectl --version equals package.json version` | The just-installed CLI's `--version` output equals the `version` field in the published `package.json`. Catches npm replication lag. |

Negative tests — covered by the Failure Modes table; the implementing agent writes one negative test per row that is not already covered by the positive tests above.

Regression tests — every existing `zombiectl/test/*.test.js` MUST continue to pass. The new acceptance suite is additive; the existing `bun run test` glob excludes `test/acceptance/`.

---

## Acceptance Criteria

- [ ] M65_001 vault prerequisite met (inherited gate) — verify: `op read 'op://VAULT/e2e-fixtures/regular/email'` returns a non-mailinator domain.
- [ ] `bun run test:acceptance` against local `ZOMBIE_ACCEPTANCE_TARGET=https://api-dev.usezombie.com` runs both specs green — verify: paste the green run line.
- [ ] `cli-acceptance-dev` passes on `deploy-dev.yml` for the implementation PR's branch — verify: link the GH Actions run URL.
- [ ] `cli-acceptance-prod` passes on `post-release.yml` for the first release after this PR merges — verify: link the GH Actions run URL.
- [ ] `login-flow.spec.js` is skipped against PROD — verify: dry-run with `ZOMBIE_ACCEPTANCE_TARGET=https://api.usezombie.com` shows the spec as `skipped`.
- [ ] WS-E #C1 regression: captured stdout/stderr never contain the minted JWT substring — verify: `grep -c "expect.*not.toContain.*sessionJwt" zombiectl/test/acceptance/*.spec.js` ≥ 2.
- [ ] WS-E #C2 mitigation: op:// load step in both workflows is sequenced AFTER `npm i` / `bun install` — verify: `grep -n -A2 "1password/load-secrets-action" .github/workflows/{deploy-dev,post-release}.yml` shows it appearing later than the install step in YAML line order.
- [ ] WS-E #C3 regression: `credentials.json` mode 0600 assertion present — verify: `grep -n "0o600" zombiectl/test/acceptance/login-flow.spec.js`.
- [ ] No spec imports from `ui/packages/` — verify: `grep -rn "ui/packages" zombiectl/test/acceptance/` returns 0 matches.
- [ ] `docs/AUTH.md` carries a "CLI fixture identity carve-out" subsection — verify: `grep -n "CLI fixture identity carve-out" docs/AUTH.md`.
- [ ] No file added or modified exceeds 350 lines — verify: `git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l | awk '$1 > 350'`.
- [ ] `gitleaks detect` clean — verify: `gitleaks detect` output.
- [ ] `make lint` clean.
- [ ] Existing `zombiectl/test/*.test.js` still pass — verify: `cd zombiectl && bun run test`.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Acceptance suite passes locally against api-dev
cd zombiectl && ZOMBIE_ACCEPTANCE_TARGET=https://api-dev.usezombie.com \
  ZOMBIE_ACCEPTANCE_BINARY=worktree \
  CLERK_SECRET_KEY="$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')" \
  bun run test:acceptance

# E2: Existing unit + integration tests still pass
cd zombiectl && bun run test

# E3: Lint
make lint 2>&1 | tail -10

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3

# E5: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: No cross-package imports
grep -rn "ui/packages" zombiectl/test/acceptance/ ; echo "E6: empty above = pass"

# E7: No process.env mutation in acceptance helpers
grep -rnE "process\.env\.[A-Z_]+\s*=" zombiectl/test/acceptance/ ; echo "E7: empty above = pass"

# E8: AUTH.md captures CLI carve-out
grep -n "CLI fixture identity carve-out" docs/AUTH.md

# E9: WS-E #C1 regression in both specs
grep -c "not.toContain" zombiectl/test/acceptance/*.spec.js

# E10: WS-E #C3 mode assertion present
grep -n "0o600" zombiectl/test/acceptance/login-flow.spec.js
```

---

## Dead Code Sweep

N/A — no files deleted. The implementation PR adds the new acceptance tree, two workflow jobs, one `docs/AUTH.md` subsection, and edits to `zombiectl/package.json` + `bunfig.toml` + `.gitignore`. No symbols removed.

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

- **Editing `ui/packages/app/tests/e2e/acceptance/**`.** Another agent owns it. The CLI suite re-uses the shared fixture identity by reading the same op:// vault items, not by importing TS code.
- **Re-auditing M65_001's vulnerability table.** WS-E adds three CLI-specific rows; the shared rows stay dispositioned as M65_001 lands them.
- **Fixture teardown** (tenant + billing balance). Same Captain deferral as M65_001.
- **PR-time `cli-acceptance` gate.** Inherited deferral from M65_001 WS-B #6.
- **`login-flow.spec.js` on PROD.** Skipped until M65_001's vault + Clerk-PROD-test-mode conditions are met.
- **A separate JS twin for `bootstrap.ts` / `svix.ts`.** The CLI suite does NOT drive webhooks. `provisionUser`/`mintTokens`/`attachJwt` is the minimum sufficient surface; bootstrap is implicit because the dashboard suite's `globalSetup` already ran the `user.created` Svix post for the shared `regular` fixture before this suite ever fires.
- **CLI behavioral changes.** If the suite surfaces a real bug, it lands in a separate PR — this spec is test infrastructure only.
- **`~/Projects/docs/changelog.mdx` `<Update>`.** This PR is not user-visible; the implementation PR adds the changelog entry.

---

## Discovery (out-of-scope but adjacent observations the implementing agent SHOULD surface)

1. **Daily cron for `cli-acceptance-prod`.** Backend changes that ship without a CLI release still need to re-exercise the published CLI. A daily cron is the cheapest cover. If the implementing agent finds an existing daily cron job in `.github/workflows/`, they reuse its expression; otherwise they add a new one. Either way, surface in the PR.
2. **`zombiectl login` browser-handoff page selectors.** The dashboard's `/cli-auth/{session_id}` (or whatever path the app actually uses — implementing agent reads `ui/packages/app/app/` to confirm) is the click-target. If the page selector drifts, `login-flow.spec.js` fails loud. Worth documenting the page's selector contract in `docs/AUTH.md` alongside the CLI carve-out — but only if a stable test-id already exists. If the dashboard uses ad-hoc labels, surface as a follow-on.
3. **`scripts/audit-runtime-imports.mjs` extension.** That script already audits `src/`; extending it to also audit `test/acceptance/` for `ui/packages` imports closes the Invariant #5 loop deterministically (instead of relying on a one-line grep in CI). Worth a follow-on if it's a small change.
4. **Cross-runtime constants drift.** `zombiectl/test/acceptance/fixtures/constants.js` and `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` carry the same identifier set. RULE UFS calls for one literal, every reader — but across runtimes the literal must be duplicated. The implementing agent should consider whether a build-time generator (read one source, emit both files) is warranted, or whether a CI grep ("both files have `CLERK_API_BASE = ...`") is enough. Default: CI grep.
5. **`postinstall.mjs` scrutiny.** `cli-acceptance-prod` is the first job to run `@usezombie/zombiectl`'s postinstall under CI with secrets in scope. Worth a separate security pass on `scripts/postinstall.mjs` — what does it read, what does it write, what does it phone home about. Surface findings in the PR; landing a fix is a separate spec if needed.

---

## Branch + PR conventions for this spec PR

- Branch: `chore/m65-002-spec-zombiectl-e2e-lifecycle` (off `main`).
- Single commit: `chore(spec): add M65_002 — zombiectl e2e full lifecycle scenarios`.
- PR title: `chore(spec): M65_002 — zombiectl e2e full lifecycle scenarios`.
- PR body links: this spec file, `docs/v2/pending/M65_001_…`, `docs/AUTH.md` "PROD fixture identity carve-out" anchor.
- No `/review` skill chain on this PR — the chain runs on the implementation PR per the table above.
- Captain inspects, prioritises, and opens the implementation milestone separately.
