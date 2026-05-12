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
**Status:** IN_PROGRESS
**Priority:** P1 — M65_001 ships dashboard acceptance gates on every deploy. The published CLI (`@usezombie/zombiectl`) has no equivalent: today's `zombiectl/test/**` files are unit + mock-API integration runs. A regression in the CLI's auth handoff, install, or lifecycle path against the real `api-dev`/`api` ships to npm undetected until a user reports it. This spec adds the same shape of gate for the CLI surface — DEV against the worktree binary on every backend deploy, PROD against the just-published npm tarball on every release.
**Categories:** TESTING, SECURITY
**Batch:** B1 — no parallel workstreams in M65_002. Sequenced after M65_001 because it consumes the same `op://VAULT/e2e-fixtures/{regular,admin}/email` vault items and the same `regular` Clerk fixture identity.
**Branch:** chore/m65-002-spec-zombiectl-e2e-lifecycle — Captain authorized continuing on the spec branch in lieu of cutting a separate `feat/m65-002-zombiectl-cli-e2e` branch; the PR scope broadens from spec-only to spec+implementation.
**Depends on:** M65_001 (vault-resolved fixture emails + persistent `regular` fixture in both Clerk DEV and Clerk PROD). **Hard merge-gate:** same as M65_001 — `op://VAULT/e2e-fixtures/{regular,admin}/email` MUST resolve to non-mailinator domains AND the workflow `env:` blocks MUST consume them. The CLI suite re-uses both invariants.

**Canonical architecture:** `docs/AUTH.md` §"Test infrastructure — e2e fixture mint (admin path)" + §"PROD fixture identity carve-out". Sibling spec: `docs/v2/pending/M65_001_P1_TESTING_SECURITY_AUTH_E2E_FULL_LIFECYCLE_SCENARIOS.md`.

---

## Implementing agent — read these first

1. `docs/v2/pending/M65_001_P1_TESTING_SECURITY_AUTH_E2E_FULL_LIFECYCLE_SCENARIOS.md` — sister spec on the dashboard side. WS-A (password-disable viability), WS-B (vulnerability audit), and the PROD-fixture-identity carve-out are referenced; do not re-audit those rows.
2. `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` — the dashboard's full-lifecycle spec. The CLI scenario mirrors its post-auth flow: install → observe → bill → stop → resume → kill. Same persistent `regular` fixture, same `samples/platform-ops/{SKILL,TRIGGER}.md` bundle, same Clerk identity. Do NOT import from `ui/packages/app/tests/e2e/acceptance/` — that suite is owned by another agent. Read for reference, re-implement in JS for the CLI suite.
3. `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — the 3-phase admin-mint chain (`provisionUser` → `bootstrapTenant` → `attachJwt`). The CLI suite re-implements the **minimal** subset against the Clerk Backend API in plain JS. Use the same endpoints, the same `is_test_fixture` metadata tag, the same `expires_in_seconds` posture (carried forward from WS-B #11 in M65_001).
4. `zombiectl/src/cli.js` — env-var auth resolution: `resolvedToken = creds.token || env.ZOMBIE_TOKEN || null` (`src/cli.js:65`). The lifecycle spec injects a Clerk-minted session JWT via `ZOMBIE_TOKEN` and bypasses `zombiectl login` entirely. The login-flow spec drives the real `login` command end-to-end.
5. `zombiectl/test/helpers-cli-state.js` — `withFreshStateDir` + `withAuthedStateDir` patterns. The acceptance suite re-uses `withFreshStateDir` verbatim (per-test temp `ZOMBIE_STATE_DIR`); `withAuthedStateDir` is bypassed because the acceptance suite mints real JWTs instead of stubbing `credentials.json`.
6. `zombiectl/test/onboarding-flow.integration.test.js` — the canonical mock-API login test. The new `lifecycle-after-login.spec.js` is its real-API sibling — same lifecycle (POST sessions → poll → write credentials.json) against live `api-dev` instead of a mock server.
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

**Goal (testable):** Four acceptance suites (`help-and-errors.spec.js`, `lifecycle-with-token.spec.js`, `lifecycle-after-login.spec.js`, `flags-and-env.spec.js`) under `zombiectl/test/acceptance/` run inside the existing repo (not as a separate package), drive the published-shape `zombiectl` CLI surface against live API, and gate two new CI jobs: `cli-acceptance-dev` (post-deploy-dev, worktree binary, targets `api-dev`) and `cli-acceptance-prod` (post-release, globally-installed npm binary, targets `api`).

The first three suites cover three orthogonal auth contexts; the fourth covers cross-cutting configuration surface (global flags, env vars, signal handling):

1. **`help-and-errors.spec.js`** — no auth, no live API needed for most assertions. Help triplet (`zombiectl` / `zombiectl help` / `zombiectl -h`), version flag, invalid top-level command (`zombiectl pogo` → exit ≠ 0 with suggest-command), invalid subcommand for every command group (`zombiectl workspace pogo`, `zombiectl agent pogo`, etc.), missing required arg on every command that needs one (e.g. `zombiectl workspace use` with no id), and "auth-required command without credentials" → exit 1 with `not authenticated`. Live API only touched for the auth-guard assertion (and only to confirm the guard fires *before* any network call).
2. **`lifecycle-after-login.spec.js`** — drives real `zombiectl login` against `api-dev` with a Playwright browser handshake, asserts `credentials.json` mode `0600` + 3-segment JWT, then runs the full read+write happy path using ONLY the persisted credentials (no `ZOMBIE_TOKEN` env): `doctor`, `workspace list`, `workspace show`, `tenant provider show`, `billing show`, `agent list`, `grant list`, `zombie list`, install `platform-ops`, `zombie status <id>`, lifecycle Stop → Resume → Kill. DEV-only.
3. **`lifecycle-with-token.spec.js`** — Clerk-admin-minted JWT in `ZOMBIE_TOKEN`, no `credentials.json` on disk. Covers the lifecycle, the read-only sweep across every command surface that supports `list`/`show`, and the invalid-arg-value negative matrix (`zombiectl status nonexistent-id`, `zombiectl kill nonexistent-id`, `zombiectl workspace use nonexistent-id`, etc. — each must exit ≠ 0 with a clear error code mapped to a UZ-* registry entry). DEV + PROD.
4. **`flags-and-env.spec.js`** — global-flag matrix and precedence (`--api`, `--json`, `--no-input`, `--no-open`, `--version`, `--help`/`-h` including combinations like `--version --help` and `--version --json`), env-var matrix and documented overrides (`ZOMBIE_API_URL`, `ZOMBIE_TOKEN`, `ZOMBIE_API_KEY`, `ZOMBIE_STATE_DIR`, `NO_COLOR`), and signal handling — SIGINT on `zombiectl login` with and without `--no-open` exits non-zero and never persists a partial `credentials.json`. DEV + PROD.

**Problem:**

1. `@usezombie/zombiectl` ships to npm with `bun run test` green, but `bun run test` is mock-API only. A regression in the real-network path (HTTP retry shape, auth header drift, JSON envelope drift) lands on npm undetected until a customer reports it.
2. The dashboard's acceptance suite (M65_001) proves the UI's auth handoff works against every deploy. The CLI has no equivalent — `zombiectl login` against live `api-dev` has never been exercised in CI. A future change to the dashboard's `/cli-auth/{session_id}` handoff page or the backend's `POST /v1/auth/sessions` shape could break the published CLI without showing up in any test.
3. Operators reach for the CLI on freshly-installed laptops via `npm i -g @usezombie/zombiectl`. The lifecycle of "global install → login → install zombie → observe → halt" is never run in any test today. The `postinstall.mjs` step, the auth state file mode bits, and the per-request retry posture are all proved only at the unit level against fakes.

**Solution summary:** Three new Node-test specs under `zombiectl/test/acceptance/`, opt-in via a new `bun run test:acceptance` script. The unauth-surface spec proves every command's parse + help + auth-guard shape against the real binary (worktree-in-DEV, npm-global-in-PROD). The lifecycle spec mints a Clerk session JWT via the Clerk Backend API (mirroring `clerk-admin.ts`), injects it as `ZOMBIE_TOKEN`, and walks the full CLI surface (lifecycle + read-only sweep + invalid-arg-value negatives). The login-flow spec spawns `zombiectl login --no-open`, drives the browser-handoff page via Playwright with a pre-mounted Clerk session cookie, then runs the full read+write happy path using ONLY the resulting persisted `credentials.json` — proving the real auth handshake AND that the persisted credentials are end-to-end usable. Two new GH Actions jobs gate these on every DEV deploy + every npm publish. The vulnerability audit adds three CLI-specific rows; M65_001's table is referenced for the shared rows.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/test/acceptance/help-and-errors.spec.js` | CREATE | No-auth surface: help triplet, version, invalid top-level command, invalid subcommand on every group, missing required arg on every command, auth-guard fires before any network call. Runs against DEV (worktree binary) AND PROD (npm-installed binary). |
| `zombiectl/test/acceptance/flags-and-env.spec.js` | CREATE | Global-flag matrix (`--api`, `--json`, `--no-input`, `--no-open`, `--version`, `--help`/`-h`) including precedence combinations, env-var matrix (`ZOMBIE_API_URL`, `ZOMBIE_TOKEN`, `ZOMBIE_API_KEY`, `ZOMBIE_STATE_DIR`, `NO_COLOR`) including documented overrides, and signal handling (SIGINT on `zombiectl login` with and without `--no-open`). Runs against DEV + PROD. |
| `zombiectl/test/acceptance/lifecycle-with-token.spec.js` | CREATE | `ZOMBIE_TOKEN` env injection. Lifecycle (install → status → logs → billing → stop → resume → kill) + read-only sweep across every command that supports `list`/`show` + invalid-arg-value negative matrix. DEV + PROD. |
| `zombiectl/test/acceptance/lifecycle-after-login.spec.js` | CREATE | Drives `zombiectl login` end-to-end against live `api-dev` with a Playwright browser handshake. After handshake, runs the full read+write happy path (doctor, workspace list/show, tenant provider show, billing show, agent list, grant list, install platform-ops, status, Stop → Resume → Kill) using ONLY the persisted `credentials.json` — `ZOMBIE_TOKEN` is explicitly absent from every follow-on spawn. DEV-only; skipped against PROD until the M65_001 vault + Clerk-PROD-test-mode conditions are met. |
| `zombiectl/test/acceptance/fixtures/clerk-admin.js` | CREATE | Minimal JS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts`. Implements `provisionUser`, `mintTokens`, `attachJwt` against the Clerk Backend API. Self-contained — no imports from the UI package. |
| `zombiectl/test/acceptance/fixtures/cli.js` | CREATE | `runZombiectl(args, { env, stdin, timeoutMs })` helper. Spawns `node ./bin/zombiectl.js` (DEV mode) or `zombiectl` from PATH (PROD mode) based on `ZOMBIE_ACCEPTANCE_BINARY` env. Per-spawn env scoping — never mutates `process.env`. |
| `zombiectl/test/acceptance/fixtures/seed.js` | CREATE | `installPlatformOpsZombie(name)` helper — reads `samples/platform-ops/{SKILL,TRIGGER}.md` from worktree root and shells out to `zombiectl install --from`. Returns parsed `{id, name, workspace_id}` from CLI's `--json` output. |
| `zombiectl/test/acceptance/fixtures/lifecycle.js` | CREATE | Shared action helpers: `stopZombie(id)`, `resumeZombie(id)`, `killZombie(id)`, `expectStatus(id, expected)`. All wrap `runZombiectl` and assert exit code + parsed JSON envelope. |
| `zombiectl/test/acceptance/fixtures/negatives.js` | CREATE | Shared negative-path helpers: `expectInvalidSubcommand(group)` asserts exit ≠ 0 + suggest-command stem on stderr; `expectMissingArg(args)` asserts exit ≠ 0 + a `missing argument` stem; `expectInvalidArgValue(args, expectedErrorCode)` asserts exit ≠ 0 + a registered `UZ-*` code on stderr (or JSON envelope `error.code` when `--json` is set). |
| `zombiectl/test/acceptance/fixtures/command-matrix.js` | CREATE | Single source of truth for the command-group enumeration the unauth + ZOMBIE_TOKEN suites iterate over: `["workspace", "agent", "grant", "tenant", "billing", "zombie"]` plus, per group, the subcommands and their required positional args. Also exports `INVALID_ID_SAMPLES` (representative invalid-format strings for §4c2) and `EMPTY_LIST_CONVENTIONS` (per-command "no \<items\>" message stem + JSON-shape expectations for §4b'/§5b'). Read once, reused across specs (RULE UFS — every "list of commands" literal lives here, nowhere else). |
| `zombiectl/test/acceptance/fixtures/teardown.js` | CREATE | `cleanWorkspaceZombies(workspaceId)` — calls `zombiectl zombie list --json` + `zombiectl kill` for each non-terminal zombie. Used in `afterEach`. |
| `zombiectl/test/acceptance/fixtures/constants.js` | CREATE | Cross-runtime constants shared by every fixture file: `CLERK_API_BASE`, `IS_TEST_FIXTURE_METADATA_KEY`, `FIXTURE_EMAIL_VAULT_PATHS`, `PLATFORM_OPS_SAMPLE_DIR`, `LOGIN_POLL_MS`, `LOGIN_TIMEOUT_SEC`. RULE UFS — one literal, every reader. |
| `zombiectl/test/acceptance/fixtures/browser.js` | CREATE | Thin Playwright wrapper used only by `lifecycle-after-login.spec.js`. Launches Chromium, mounts the `regular` fixture's Clerk session cookies, navigates to a given URL, clicks the CLI-auth approve button. Lifted from the dashboard suite's approach but re-implemented in JS — no `@playwright/test`, just `playwright`'s `chromium` API. |
| `zombiectl/test/acceptance/global-setup.js` | CREATE | Pre-suite hook: resolves Clerk admin secret + fixture vault paths, ensures the `regular` fixture user exists in the target Clerk instance, mints a session JWT, writes it to a temp file that each spec reads. Mirrors `ui/packages/app/tests/e2e/acceptance/global-setup.ts`'s shape but JS + CLI-only. |
| `zombiectl/scripts/run-acceptance.mjs` | CREATE | New runner. Iterates `test/acceptance/*.spec.js` files via `node --test`, gates on `ZOMBIE_ACCEPTANCE_TARGET` env being set (skips silently when unset so local `bun run test` is unaffected). |
| `zombiectl/package.json` | EDIT | Add `test:acceptance` script invoking `scripts/run-acceptance.mjs`. Add `playwright` as a `devDependencies` entry (browser-handoff is opt-in; only the login-flow spec imports it). |
| `zombiectl/bunfig.toml` | EDIT | Exclude `test/acceptance/` from the default `bun test` glob — acceptance tests run via `node --test` only, not as bun unit tests. |
| `.github/workflows/deploy-dev.yml` | EDIT | Add `cli-acceptance-dev` job sequenced after the existing `auth-e2e-dev`. Targets `api-dev`, runs the worktree binary, loads Clerk DEV credentials via op:// (same vault path conventions as `auth-e2e-dev`). **CLAUDE.md gates `.github/workflows/**` behind explicit user approval — the implementation PR carries the same Captain-authorization carve-out as M64_006.** |
| `.github/workflows/post-release.yml` | EDIT | Add `cli-acceptance-prod` job sequenced after the existing npm publish step. `npm i -g @usezombie/zombiectl@latest`, targets `https://api.usezombie.com`, loads Clerk PROD credentials via op://. Same `.github/workflows/**` gate. |
| `zombiectl/src/program/routes.js` | EDIT | Add `help` as a real route so `zombiectl help` is equivalent to bare `zombiectl` (currently falls through to suggest-command). Three-line addition mirroring existing route shapes; dispatcher invokes the same `printHelp` path `cli.js:69` already takes for `global.help`. Scope justification: the spec's help-triplet test asserts the four forms are identical, and this is the smallest possible change to make that assertion true. |
| `zombiectl/src/program/validate.js` | EDIT (conditional) | If the implementing agent swaps to a uuidv7-aware library per §4c2's implementation default, this file becomes a thin wrapper over the library's validator. Error stem updated to `invalid <name>: expected uuidv7 format (e.g. 0192a3b4-c5d6-7e8f-9012-345678901234)`. RULE NLG: the old regex is removed in the same commit (no parallel validation paths). If the agent keeps the current regex (because no acceptable library exists), this row drops from Files Changed and a Discovery row records the mismatch. |
| `zombiectl/package.json` (additional) | EDIT (conditional) | If §4c2 lands the library swap: add the chosen uuidv7-aware package to `dependencies` (NOT `devDependencies` — runtime validation). Pin major version. Document the supply-chain rationale in the PR description. |
| `zombiectl/.gitignore` | EDIT | Add `test/acceptance/.fixture-jwt` (per-suite minted JWT file written by `global-setup.js`, mode 0600). |
| `docs/AUTH.md` | EDIT | Append a "CLI fixture identity carve-out" subsection mirroring "PROD fixture identity carve-out" — documents that `cli-acceptance-{dev,prod}` re-uses the dashboard fixture's Clerk identity, that `ZOMBIE_TOKEN` is the injection surface, and that the JWT TTL chosen here is the same one M65_001 lands. |

**Files NOT changed (explicit non-goals on this milestone):**

- `ui/packages/app/tests/e2e/acceptance/**` — another agent owns it. The CLI suite consumes the same shared fixture identity but does not import or modify anything in this tree.
- `zombiectl/src/**` — no CLI behavior changes EXCEPT two carve-outs called out in Files Changed: (a) the `help` subcommand route in `zombiectl/src/program/routes.js`, (b) an optional swap of `zombiectl/src/program/validate.js` to a uuidv7-aware library per the §4c2 implementation default. Both are the smallest possible changes to make the spec's assertions true and land in the same PR. Any other CLI bug surfaced while writing the suite lands in a separate PR.
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

### §3 — Unauth-surface scenario (`help-and-errors.spec.js`)

Proves the CLI's parse + help + auth-guard layer behaves correctly against the same binary that ships to prod (worktree-DEV / npm-global-PROD). No `ZOMBIE_TOKEN`, no `credentials.json`, no live API calls (the auth guard fires before any network I/O).

Behavioral claims covered:

- **Help triplet (item 1).** `zombiectl`, `zombiectl help`, `zombiectl -h`, `zombiectl --help` all exit 0 with stdout containing the help banner. The four outputs are byte-identical to each other (helps catch drift where one form prints a subtly different help body). Implementing agent picks the canonical stdout-comparison shape.
- **Version flag.** `zombiectl --version` and `zombiectl -v` exit 0 with stdout matching `package.json`'s `version` field.
- **Invalid top-level command (item 2, top-level).** `zombiectl pogo` exits ≠ 0; stderr contains the suggest-command stem (existing `did-you-mean.integration.test.js` already covers the symbol-distance algorithm — this spec proves the live binary still wires it up).
- **Invalid subcommand on every group (item 2, per group).** For each command group enumerated in `fixtures/command-matrix.js` (`workspace`, `agent`, `grant`, `tenant`, `billing`, `zombie`), assert `zombiectl <group> pogo` exits ≠ 0 with the group's dispatcher reporting "unknown action". Implementing agent enumerates by reading `command-matrix.js`, not by hard-coding in the spec.
- **Missing required positional arg (item 3).** For each subcommand that declares a required arg in `command-matrix.js` (e.g. `workspace use <workspace_id>`, `workspace delete <workspace_id>`, `agent delete <key_id>`, `grant delete <grant_id>`, `kill <zombie_id>`, `stop <zombie_id>`, `resume <zombie_id>`, `status <zombie_id>`, `logs <zombie_id>`), assert the empty-arg form exits ≠ 0 with stderr containing a `missing argument` stem.
- **Auth-required without credentials.** For a representative slice of auth-required commands (`doctor`, `workspace list`, `billing show`, `zombie list`), assert `zombiectl <cmd>` with no `ZOMBIE_TOKEN` and an empty `ZOMBIE_STATE_DIR` exits 1 with `not authenticated` on stderr — AND no network call fired (the spec captures `ZOMBIE_API_URL` set to an unroutable address like `http://127.0.0.1:1`; if the auth guard misfires and a fetch is attempted, it surfaces as a connection error instead of `not authenticated`, failing the test).
- **ID validator surface (item: agent/human-friendly error messages).** The CLI's `zombiectl/src/program/validate.js` exposes `validateRequiredId(value, name)` returning `{ok, message}`. Spec asserts the validator is reachable and its error message stem matches the documented shape (current stem: `invalid <name>: expected UUID format (e.g. 550e8400-…) or alphanumeric identifier (4-128 chars)` — implementing agent reads the live string from `validate.js` and uses it as the test's source of truth, not hardcoded in the spec). The actual "invalid-id-rejected client-side" behavior requires auth and is exercised in §4c2 — `help-and-errors.spec.js` only asserts the MESSAGE SHAPE the validator emits when it does fire. RULE UFS: the validator's error stem is the single source of truth; both this spec and §4c2 read it from the live module.

**Implementation default:** the help-triplet byte-equality assertion strips trailing whitespace and ANSI color codes before comparison (the CLI's color path depends on TTY detection; tests spawn without a TTY, but strip defensively).

**Implementation default:** `zombiectl workspace show` without a workspace context is treated as a read-only "show current" command and is covered in §4/§5 (with auth), NOT in §3 — it's not a missing-arg failure case.

### §4 — Lifecycle scenario (`lifecycle-with-token.spec.js`) — `ZOMBIE_TOKEN` injection

Three behavioral blocks under the same auth context (Clerk-admin-minted JWT in `ZOMBIE_TOKEN`, no `credentials.json`):

**§4a — Lifecycle walk (item 7).** Mirrors `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` step-for-step, replacing each browser action with the equivalent CLI invocation:

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

**§4b — Read-only sweep (items 4, 8).** With the same `ZOMBIE_TOKEN`, exercise every read-only command the CLI exposes — proving each one returns a parseable JSON envelope against live API. Implementing agent enumerates from `fixtures/command-matrix.js`'s `readOnly` table. Expected coverage at minimum: `doctor`, `workspace list --json`, `workspace show --json`, `agent list --json`, `grant list --json`, `tenant provider show --json`, `billing show --json`, `zombie list --json`. Each invocation exits 0; the JSON parse succeeds; the schema's required top-level keys are present (the schemas live in the existing `output/` formatters — spec does NOT pin them).

**§4b' — Empty-list standard message.** For every `list` command in `READ_ONLY_COMMANDS`, run the invocation against a freshly-seeded workspace state where the queried collection is empty (e.g. immediately after `cleanWorkspaceZombies` empties zombies). Assertions:

- Non-JSON mode (`zombiectl <cmd>` without `--json`): stdout contains a standard "no \<items\>" message (current pattern: `ui.info("no workspaces")` at `workspace.js:95` — implementing agent confirms the same convention exists for `zombie list`, `agent list`, `grant list`, and surfaces any gap as a follow-on bug in Discovery, NOT a CLI fix in this PR).
- JSON mode (`zombiectl <cmd> --json`): stdout parses as JSON with `items: []` (empty array) AND `total: 0`. NOT `null`, NOT an exit code, NOT a non-zero status.

The spec asserts the shape; the implementing agent enumerates command-by-command from the matrix.

**§4c1 — Invalid-arg-value matrix, nonexistent-but-valid-format (item 10).** With the same `ZOMBIE_TOKEN`, hit every auth-required command that takes a positional identifier with a syntactically-VALID identifier that does not exist server-side. Use a freshly-generated uuidv7 (via `crypto.randomUUID()` is uuidv4 — implementing agent uses a uuidv7-shaped string, OR generates a random uuidv7 via the same algorithm `src/types/id_format.zig` uses, OR reads a "known-nonexistent" id constant from `command-matrix.js`). Expected coverage at minimum: `zombiectl status <random-uuidv7>`, `kill <random-uuidv7>`, `stop`, `resume`, `logs`, `workspace use`, `workspace delete`, `agent delete`, `grant delete`. Each exits ≠ 0; the JSON-mode form returns `{error: {code, message}}` with `code` registered in `src/errors/error_registry.zig` (UZ-* surface — RULE ERROR REGISTRY GATE). One real network call per assertion.

**§4c2 — Invalid-arg-value matrix, invalid-format identifier (DO NOT stress the API).** With the same `ZOMBIE_TOKEN`, send each ID-taking command a string that fails `validateRequiredId` (e.g. `"not-a-uuid"`, `"foo"`, `"---"`, `"abc def"` — three characters too short, contains spaces, etc.). Assertions:

- Exit code ≠ 0.
- `--json` stdout returns `{error: {code, message}}` where `code` is a registered UZ-* validation code AND `message` matches the stem emitted by `validate.js → validateRequiredId(value, name)` for the offending arg.
- **Critical invariant: NO network call fires.** Captured via setting `ZOMBIE_API_URL=http://127.0.0.1:1` for the duration of these assertions — if the CLI ever sends an invalid-format ID to the backend (stressing the API), the test surfaces it as `ECONNREFUSED` instead of the validation stem. Test fails loud; the CLI either validates client-side (correct) or it doesn't (bug).
- For every `requiresIdentifier` row in `command-matrix.js`, this sub-test runs at least one invalid-format case.

The spec does NOT pin a specific list of invalid strings; the implementing agent picks a small representative set (one obviously-wrong, one nearly-right). The invariant is: validator catches client-side, no network, message stem matches `validate.js`.

**Implementation default — validation library preference.** The current `validate.js` hand-rolls a permissive UUID regex (any-version-UUID OR 4-128 char alphanumeric). The backend generates strict uuidv7 (`src/types/id_format.zig`). Mismatch is real — the CLI accepts strings the backend would reject. Captain prefers swapping `validate.js` to a published library that knows uuidv7 specifically (e.g. the `uuid` npm package's `uuid.validate(str)` + `uuid.version(str) === 7`, or a uuidv7-specific package) over carrying a hand-rolled regex. Trade-offs the implementing agent weighs:

- **For library**: matches backend invariant, smaller hand-maintained surface, vetted format coverage.
- **Against library**: adds a runtime dependency to the published CLI (supply-chain surface — see WS-E #C2), bundle-size impact (negligible for `uuid` at ~5KB), postinstall behavior of the chosen library must be vetted.

Decision rule: IF a uuidv7-aware library exists with an acceptable supply-chain posture (single-author maintained packages are rejected; `uuid` from npm passes), swap `validate.js` to use it. Update the error stem to name uuidv7 explicitly: `invalid <name>: expected uuidv7 format (e.g. 0192a3b4-c5d6-7e8f-9012-345678901234)`. Otherwise keep the current regex with a Discovery row flagging the mismatch. Either way, the §4c2 tests pass — they assert behavior, not implementation.

If the library swap lands: the swap belongs in the SAME implementation PR as the spec acceptance — RULE NLG forbids carrying both code paths during the transition (pre-v2.0.0).

**Implementation default:** unique zombie name per test via `crypto.randomBytes(4).toString("hex")` to avoid `(workspace_id, name)` collisions from interrupted prior runs (same rationale as `login-install-lifecycle.spec.ts:34`).

**Implementation default:** `cleanWorkspaceZombies(workspaceId)` in `afterEach` kills any non-terminal leftovers. Tenant + billing-balance teardown is OUT OF SCOPE (M65_001 Captain deferral inherited).

**Implementation default:** §4b and §4c iterate `command-matrix.js` rather than hardcoding the command list in the spec body. If a new command lands in `src/commands/`, the implementing agent of THAT change adds a row to `command-matrix.js` and these sweeps pick it up automatically.

### §5 — Login-flow scenario (`lifecycle-after-login.spec.js`) — real handshake + persisted-credentials happy path

Three behavioral blocks under the same auth context (real `zombiectl login` → `credentials.json` on disk, `ZOMBIE_TOKEN` explicitly absent from every follow-on spawn). DEV-only.

**§5a — Handshake (item 5).** Drives `zombiectl login --no-open --no-input --timeout-sec 60 --poll-ms 500`, parses the `login_url` from stdout, spawns a Playwright Chromium context with the `regular` fixture's Clerk session cookies pre-mounted, navigates the browser to `login_url`, clicks the approve action on the dashboard's CLI-auth handoff page, waits for the CLI subprocess to exit 0. Asserts `credentials.json` exists at mode `0600` inside the tmpdir-scoped `ZOMBIE_STATE_DIR`, and the `token` field is a 3-segment JWT.

**§5b — Persisted-credentials read-only sweep (item 6 expanded).** With `ZOMBIE_TOKEN` **explicitly absent** from the spawn env (only `ZOMBIE_STATE_DIR` set), run the same read-only sweep §4b runs but proving `credentials.json` is the load-bearing auth source: `doctor`, `workspace list --json`, `workspace show --json`, `tenant provider show --json`, `billing show --json`, `agent list --json`, `grant list --json`, `zombie list --json`. Each exits 0 with a parseable JSON envelope. The contrast with §4b is the test: if the CLI ever regressed to require `ZOMBIE_TOKEN` even when `credentials.json` is present, §5b fails while §4b passes — and the failure points squarely at the credential-loader regression.

**§5b' — Empty-list standard message (persisted-creds variant).** Same shape as §4b' — every `list` command that returns an empty collection emits the standard "no \<items\>" message in non-JSON mode AND `{items: [], total: 0}` in `--json` mode. Asserted in the persisted-credentials auth context. If §4b' passes but §5b' fails, the regression is in the credential-resolution path (the list query went to a different tenant). If both fail on the same command, the regression is in the empty-list formatter.

**§5c — Persisted-credentials install + lifecycle (item 9).** Same auth context as §5b. Walks the full install + lifecycle: `zombiectl install --from samples/platform-ops --name <unique> --json` → `zombie list --json` shows the new id → `status <id> --json` → `stop <id>` → `resume <id>` → `kill <id>`. Proves the persisted credentials are usable for write operations, not just reads.

**Implementation default:** Playwright's `chromium` (not `@playwright/test`) — the spec already orchestrates the CLI subprocess; a parallel test-runner framework on top adds no value. The browser is one `chromium.launch()` per test.

**Implementation default:** `withFreshStateDir` (existing helper from `zombiectl/test/helpers-cli-state.js`) so the real Clerk JWT lands in a tmpdir-scoped `credentials.json`, never in `~/.config/zombie/`. The state dir + its contents are cleaned in `afterEach` regardless of test outcome.

**Implementation default:** every spawn in §5b/§5c constructs the child env explicitly — `ZOMBIE_STATE_DIR` is included, `ZOMBIE_TOKEN` is NOT (even if the parent test process has it set for some other reason). The negatives assertion in WS-E #C1 fires here too.

### §6 — Global-flag and environment-variable matrix (`flags-and-env.spec.js`)

Four behavioral blocks covering the cross-cutting configuration surface the README documents.

**§6a — Global-flag matrix and precedence (no auth needed).**

| Invocation | Asserts |
|---|---|
| `zombiectl --version` | Exit 0; stdout token equals `package.json` `version` |
| `zombiectl -v` | Equivalent to `--version` (same stdout) |
| `zombiectl --version --json` | Exit 0; stdout parses as JSON with `version` key matching `package.json` |
| `zombiectl --help` | Exit 0; stdout contains the help banner |
| `zombiectl -h` | Equivalent to `--help` |
| `zombiectl --help --json` | Exit 0; stdout parses as JSON (existing `printHelp` honors `jsonMode` — `cli.js:71-76`) |
| `zombiectl --version --help` | Exit 0; `--version` wins because it short-circuits first (`cli.js:54-61`). Stdout matches version, NOT help body. **Regression test for the documented precedence.** |
| `zombiectl --help --version` | Same as `--version --help` — order on the command line does NOT change precedence because the parser collects flags before dispatch. |

**§6b — `--api` flag and `ZOMBIE_API_URL` env precedence (with auth, needs `ZOMBIE_TOKEN`).**

Per `cli.js:80` (`apiUrl: normalizeApiUrl(global.apiUrl || creds.api_url || DEFAULT_API_URL)`), the order is: `--api` > `creds.api_url` (from `credentials.json`) > `DEFAULT_API_URL`. `ZOMBIE_API_URL` enters via `parseGlobalArgs` (which reads env). Three precedence assertions:

| Invocation | Asserts |
|---|---|
| `ZOMBIE_API_URL=http://127.0.0.1:1 zombiectl --api https://api-dev.usezombie.com workspace list --json` | Exit 0 (hits the real API, NOT the unroutable env URL). Proves `--api` wins over `ZOMBIE_API_URL`. |
| `ZOMBIE_API_URL=https://api-dev.usezombie.com zombiectl workspace list --json` (no `--api`) | Exit 0; proves `ZOMBIE_API_URL` is honored when `--api` is absent. |
| `zombiectl workspace list --json` (no `--api`, no `ZOMBIE_API_URL`, but `credentials.json` carries `api_url`) | Exit 0; proves `creds.api_url` is honored as the third tier. (Sequenced after `lifecycle-after-login.spec.js §5a` — re-uses the same persisted creds.) |

**§6c — Environment variable precedence and behavior.**

| Invocation | Asserts |
|---|---|
| `ZOMBIE_TOKEN=<jwt> zombiectl workspace list --json` (no `credentials.json`) | Exit 0; proves `ZOMBIE_TOKEN` is honored when no creds on disk. |
| `ZOMBIE_TOKEN=garbage` + `credentials.json` carrying a real token | Exit 0; CLI uses `credentials.json`'s token (per `cli.js:65`: `creds.token \|\| env.ZOMBIE_TOKEN`). README: "ZOMBIE_TOKEN — Auth token (overridden by login)". **Regression test for that documented override.** |
| `ZOMBIE_API_KEY=<admin-key> zombiectl workspace list --json` (no `ZOMBIE_TOKEN`, no creds) | Exit 0 if a valid admin key is provisioned in vault; resolved as `apiKey` per `cli.js:66`, role `admin` per `cli.js:67`. (Skipped if the vault item is absent — surfaces in `global-setup.js` log.) |
| `NO_COLOR=1 zombiectl --help` | Exit 0; stdout contains zero ANSI escape sequences (regex `/\x1b\[/` produces no matches). Per no-color.org spec, any non-empty value disables color (`cli.js:50`). |
| `NO_COLOR= zombiectl --help` (empty string) | Stdout MAY contain ANSI codes — empty value means "color allowed" per the spec. |
| `ZOMBIE_STATE_DIR=<tmpdir> zombiectl login --no-open --no-input --timeout-sec 60` | Resolved `credentials.json` written inside `<tmpdir>`, not `~/.config/zombie/`. Already used by every other test — this is the dedicated assertion. |

**§6d — `--no-input` and `--no-open` behavior on `zombiectl login`.**

| Invocation | Asserts |
|---|---|
| `zombiectl login --no-open --no-input --timeout-sec 60 --poll-ms 500` | Exit 0 after browser leg approves; stdout contains a parseable `login_url`; **the spec asserts NO browser was spawned** — verified by inspecting the child process tree before the test's own browser leg fires (no `chromium`/`Chrome`/`Safari` PIDs descended from the CLI's PID). |
| `zombiectl login --no-input --timeout-sec 1 --poll-ms 100` (NO `--no-open`) | CLI attempts to spawn a browser via `openUrl`. On a headless CI runner the spawn fails silently — the test only asserts that the CLI itself reaches the polling loop (stdout contains the URL) and exits non-zero on the 1-second timeout. Not asserting browser spawn semantics — they're OS-dependent. |

**§6e — SIGINT handling on `zombiectl login`.**

Two scenarios, both DEV-only:

| Scenario | Steps | Asserts |
|---|---|---|
| Ctrl+C during `--no-open` poll | Spawn `zombiectl login --no-open --no-input --timeout-sec 60 --poll-ms 500`; wait for first poll line on stdout; send SIGINT to the child PID; await exit | Exit code is non-zero (130 if the CLI emits the conventional SIGINT-derived code, OR any non-zero if it catches the signal and exits cleanly — spec asserts ≠ 0, not the specific value); `credentials.json` does NOT exist in the tmpdir-scoped `ZOMBIE_STATE_DIR`. |
| Ctrl+C without `--no-open` | Spawn `zombiectl login --no-input --timeout-sec 60 --poll-ms 500`; wait for first poll line; send SIGINT | Same assertions: non-zero exit, no `credentials.json`. Any browser process the CLI spawned is allowed to remain open (cleanup is OS-dependent and out-of-scope). |

**Implementation default:** the SIGINT tests use `child.kill("SIGINT")` from `node:child_process` instead of literal `^C` keystrokes. Same wire result, deterministic timing.

**Implementation default:** if the CLI does NOT currently handle SIGINT gracefully (and instead exits via the default Node behavior), §6e exposes the gap — the test failure is the bug report, and a follow-on PR adds a signal handler. The spec asserts the EXPECTED behavior (clean exit + no `credentials.json`), not whatever the CLI does today.

### §7 — CI job wiring

`cli-acceptance-dev` (`.github/workflows/deploy-dev.yml`): sequenced after the existing `auth-e2e-dev`. Loads op:// secrets (Clerk DEV admin key + fixture-email vault items) AFTER the `npm install` / `bun install` step so a hostile postinstall has no secret context. Runs `node ./bin/zombiectl.js` against `ZOMBIE_API_URL=https://api-dev.usezombie.com`. Uploads any artifacts to `cli-acceptance-dev-${{ github.sha }}/` scoped to a `playwright-cli-report/` subdir — never the temp state dir.

`cli-acceptance-prod` (`.github/workflows/post-release.yml`): sequenced AFTER the existing npm publish step (job-level `needs:` on the publish job). Same op:// load-after-install posture. Runs `npm i -g @usezombie/zombiectl@latest` then calls `zombiectl` from PATH. Targets `ZOMBIE_API_URL=https://api.usezombie.com`. Mints a Clerk PROD session JWT for the `regular` fixture via the Clerk PROD admin key (op://-resolved).

**Implementation default:** `cli-acceptance-prod` does NOT run on every Vercel deploy; it runs on every npm release. Rationale: npm is the source of truth for the CLI surface a customer touches; Vercel deploys can ship without a CLI release and a CLI release can ship without a Vercel deploy.

**Implementation default:** add a daily cron trigger to `cli-acceptance-prod` so backend changes that ship without a CLI release still re-exercise the published CLI against live PROD once per day. Cron expression: `0 13 * * *` UTC (matches existing scheduled-run cadence in the repo's other workflows; implementing agent confirms by grepping `.github/workflows/`).

### §8 — Vulnerability audit (CLI-specific rows only)

See WS-E below.

---

## WS-E — CLI-specific vulnerability audit

M65_001's audit table (WS-B) is the canonical reference for shared rows (mailinator inbox, password-disable posture, webhook secret reuse, `.fixture-jwts.json` artifact risk, tenant pollution, PR-time gate, Clerk PROD identity carve-out, `@clerk/nextjs` major pin, `freshPassword` policy, Svix msg-id collision, JWT TTL). Those are inherited; do NOT re-disposition.

Three rows are CLI-specific and dispositioned here:

| # | Vulnerability | Sev | Current state | Proposed fix | Lands in | Disposition |
|---|---|---|---|---|---|---|
| C1 | `ZOMBIE_TOKEN` env-var lifetime in the spawned subprocess. The minted Clerk JWT is visible to `/proc/<pid>/environ` (Linux) and `ps eww` for the lifetime of every `zombiectl` child. If the spawn helper leaks the JWT into `process.env` of the test runner instead of scoping to the single `spawn` call, every subsequent spawn inherits it. If the CLI ever logs its env (it should not), the JWT lands in stdout/stderr which the test captures and CI artifacts upload. | S2 | No equivalent test today — first time a real Clerk JWT will live in CI subprocess env. | `runZombiectl(args, { env })` takes per-call env, never mutates `process.env`. Spec adds an assertion that captured stdout + stderr never contain the JWT value (substring check). CI workflow loads op:// secrets in a step AFTER `npm i` / `bun install` so postinstall scripts have no secret context. | `zombiectl/test/acceptance/fixtures/cli.js`, both spec files, `.github/workflows/{deploy-dev,post-release}.yml` | `FIX_THIS_PR`. |
| C2 | npm `postinstall` running unsandboxed during `cli-acceptance-prod`. `npm i -g @usezombie/zombiectl@latest` executes `scripts/postinstall.mjs` on a GH runner that may have Clerk PROD admin secrets loaded. Supply-chain compromise of `@usezombie/zombiectl` or `posthog-node` would execute arbitrary JS with secret context. | S2 | Today the prod CLI is never installed in CI. This spec adds the install path. | Workflow load-after-install: `op://` secrets resolve in a job step that runs AFTER `npm i -g`. Postinstall sees no Clerk admin key. Documented in `docs/AUTH.md` "CLI fixture identity carve-out" subsection. | `.github/workflows/post-release.yml`, `docs/AUTH.md` | `FIX_THIS_PR` (workflow structure) + `ACCEPTED_RISK` for any residual exposure (e.g. the GH token itself is present). |
| C3 | `credentials.json` mode + path written from a real auth flow. `lifecycle-after-login.spec.js` is the first test in the codebase that drives `zombiectl login` to completion against live API; the resulting file holds a live Clerk session JWT. Risks: (a) test runner inheriting a developer's real `ZOMBIE_STATE_DIR` and overwriting their `credentials.json`; (b) the CLI's `0600` chmod is not regression-proved anywhere; (c) GH artifact uploads could include the temp state dir. | S2 | The CLI sets `0600` on save (`src/lib/state.js`). No test asserts this against the real flow. | `lifecycle-after-login.spec.js` uses `withFreshStateDir` so `ZOMBIE_STATE_DIR` is scoped per-test. Spec asserts `(stat(credentials.json).mode & 0o777) === 0o600` AND that the token field parses as a 3-segment JWT. Workflow artifact `path:` is scoped to `playwright-cli-report/` only — never the temp state dir. | `zombiectl/test/acceptance/lifecycle-after-login.spec.js`, `.github/workflows/deploy-dev.yml` | `FIX_THIS_PR`. |

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

// zombiectl/test/acceptance/fixtures/negatives.js
export async function expectInvalidSubcommand(group, env);
// Asserts: exit ≠ 0; stderr contains "unknown action" or the suggest-command stem.

export async function expectMissingArg(args, env);
// Asserts: exit ≠ 0; stderr contains "missing argument" or "required".

export async function expectInvalidArgValue(args, env, expectedErrorCode);
// Asserts: exit ≠ 0; when --json is in args, the parsed envelope has
// { error: { code: expectedErrorCode, ... } } with code registered in the
// UZ-* registry.

// zombiectl/test/acceptance/fixtures/command-matrix.js
export const COMMAND_GROUPS = ["workspace", "agent", "grant", "tenant", "billing", "zombie"];

export const READ_ONLY_COMMANDS = [
  { args: ["doctor"] },
  { args: ["workspace", "list", "--json"], jsonShape: { items: "array" } },
  { args: ["workspace", "show", "--json"], jsonShape: { workspace_id: "string" } },
  // ...etc — implementing agent extends as new read-only commands land
];

export const REQUIRES_IDENTIFIER = [
  { args: ["status", "<id>"], idPrefix: "zmb_", expectedErrorCode: "UZ-ZOMBIE-…" },
  { args: ["kill", "<id>"], idPrefix: "zmb_", expectedErrorCode: "UZ-ZOMBIE-…" },
  // ...etc
];

export const REQUIRES_POSITIONAL_ARG = [
  // For §3 missing-arg test: `zombiectl workspace use` with no id, etc.
  { args: ["workspace", "use"], missingArgName: "workspace_id" },
  // ...etc
];
```

The exact rows of `READ_ONLY_COMMANDS`, `REQUIRES_IDENTIFIER`, `REQUIRES_POSITIONAL_ARG` are owned by `command-matrix.js`, not pinned in this spec — they grow as the CLI surface grows. Spec invariant: every command surface that supports `list`/`show` is reachable from `READ_ONLY_COMMANDS`; every command that takes a positional identifier is reachable from `REQUIRES_IDENTIFIER`.

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
| Auth guard misfires and a network call is attempted in `help-and-errors.spec.js` | A future refactor moves the auth guard after route dispatch | Test sets `ZOMBIE_API_URL=http://127.0.0.1:1` (unroutable) — a leaked fetch surfaces as `ECONNREFUSED` on stderr instead of `not authenticated`. Test asserts on the latter. |
| New command surface lands in `src/commands/` without being added to `command-matrix.js` | Implementing agent of the new command forgot to extend the matrix | A separate audit (`scripts/audit-command-matrix.sh`, future enhancement — surfaced in Discovery) greps `src/commands/` for route keys not present in `command-matrix.js`. Not blocking on this milestone; surfaced as a Discovery item. Until then, code review catches it. |
| `command-matrix.js` drifts and references a removed command | Refactor removed a command without removing the matrix row | Matrix-driven sweep fails because the spawn returns "unknown action" instead of the expected output. Test fails loud; agent removes the stale row. |

---

## Invariants

1. **`runZombiectl` never mutates `process.env`.** Enforced by the helper's signature — `env` is required, composed by the caller. Lint check: grep `zombiectl/test/acceptance/` for `process.env.ZOMBIE_TOKEN =` or any `delete process.env.ZOMBIE_TOKEN` → 0 matches.
2. **Real Clerk JWTs never appear in captured stdout/stderr.** Enforced by a per-test assertion in both specs: `expect(stdout + stderr).not.toContain(sessionJwt)`. The assertion runs on every `runZombiectl` call.
3. **`cli-acceptance-prod` always runs against the just-published version.** Enforced by `runZombiectl(["--version"])` asserting equality with `package.json`'s `version` field, executed as the first action in the prod suite.
4. **op:// secrets load AFTER `npm i` / `bun install` in every CLI acceptance workflow job.** Enforced by job-step ordering in the workflow YAML; reviewed by `/review` against this invariant.
5. **No spec in `zombiectl/test/acceptance/` imports anything from `ui/packages/`.** Enforced by `scripts/audit-runtime-imports.mjs` extension (or a one-line grep gate in CI): `grep -rn "from \"ui/packages" zombiectl/test/acceptance/` → 0 matches.
6. **Every command-group enumerated in `command-matrix.js → COMMAND_GROUPS` has at least one invalid-subcommand assertion in `help-and-errors.spec.js`.** Enforced by the spec iterating `COMMAND_GROUPS` directly — if a group is added or removed, the test count changes deterministically.
7. **Every read-only command listed in `command-matrix.js → READ_ONLY_COMMANDS` is exercised in BOTH `lifecycle-with-token.spec.js §4b` AND `lifecycle-after-login.spec.js §5b`.** Enforced by both specs iterating the same exported list. A regression to one auth path but not the other surfaces as exactly one of the two specs failing on the same matrix row.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `help-and-errors.spec.js → help triplet prints identical body` | `zombiectl`, `zombiectl help`, `zombiectl -h`, `zombiectl --help` all exit 0; stripped-of-ANSI stdout is byte-identical across the four invocations and contains the help banner. (Item 1.) |
| `help-and-errors.spec.js → --version matches package.json` | `zombiectl --version` and `zombiectl -v` exit 0; stdout token equals the `version` field in `package.json`. |
| `help-and-errors.spec.js → unknown top-level command suggests` | `zombiectl pogo` exits ≠ 0; stderr contains the suggest-command stem. (Item 2 top-level.) |
| `help-and-errors.spec.js → unknown subcommand on every group` | For every group in `COMMAND_GROUPS`, `zombiectl <group> pogo` exits ≠ 0; stderr contains an "unknown action" or suggest stem. Matrix-driven — one Node `subtest` per group. (Item 2.) |
| `help-and-errors.spec.js → missing required positional arg` | For every row in `REQUIRES_POSITIONAL_ARG`, the empty-arg invocation exits ≠ 0; stderr contains a "missing argument" stem. Matrix-driven. (Item 3.) |
| `help-and-errors.spec.js → auth-required command without credentials short-circuits before any network call` | With `ZOMBIE_API_URL=http://127.0.0.1:1` and no `ZOMBIE_TOKEN`/`ZOMBIE_STATE_DIR` creds: `zombiectl doctor`, `zombiectl workspace list`, `zombiectl billing show`, `zombiectl zombie list` each exit 1 with stderr containing `not authenticated` — NOT `ECONNREFUSED`. |
| `lifecycle-with-token.spec.js §4a → installs, observes, bills, and halts a platform-ops zombie` | Persistent `regular` fixture's minted JWT in `ZOMBIE_TOKEN`; `zombiectl workspace list --json` returns ≥1 workspace; `zombiectl install --from samples/platform-ops --name <unique> --json` exits 0 with parseable `{id}`; `zombiectl status <id> --json` returns `status == "active"`; `zombiectl logs <id> --json --since 1m` returns a parseable envelope; `zombiectl billing show --json` returns `balance` field; `zombiectl stop <id>` → status `paused` or `stopped`; `zombiectl resume <id>` → status `active`; `zombiectl kill <id>` → status `killed` or `errored`; second `zombiectl kill <id>` is idempotent. (Item 7.) |
| `lifecycle-with-token.spec.js §4b → read-only sweep with ZOMBIE_TOKEN` | For every row in `READ_ONLY_COMMANDS`, `runZombiectl(row.args, { env: { ZOMBIE_TOKEN, ZOMBIE_API_URL } })` exits 0; stdout parses as JSON; the shape-check (when `row.jsonShape` is set) passes. Matrix-driven. (Items 4, 8.) |
| `lifecycle-with-token.spec.js §4b' → empty list emits standard "no <items>" message and {items:[], total:0}` | For every `list` command in `READ_ONLY_COMMANDS`: with the queried collection empty (post `cleanWorkspaceZombies` or equivalent), the non-JSON form emits the standard stem from `EMPTY_LIST_CONVENTIONS`; the `--json` form returns `{items: [], total: 0}`. |
| `lifecycle-with-token.spec.js §4c1 → invalid-arg-value matrix with valid-format nonexistent ID` | For every row in `REQUIRES_IDENTIFIER`, `runZombiectl([...row.args, "<random-uuidv7>", "--json"], { env: { ZOMBIE_TOKEN, ZOMBIE_API_URL } })` exits ≠ 0; JSON envelope on stdout has `error.code` matching the row's `expectedErrorCode` AND the code is registered in `src/errors/error_registry.zig`. Matrix-driven. (Item 10, valid-format slice.) |
| `lifecycle-with-token.spec.js §4c2 → invalid-format ID rejected client-side without any network call` | For every row in `REQUIRES_IDENTIFIER`, at least one invocation from `INVALID_ID_SAMPLES` (e.g. `"not-a-uuid"`, `"foo"`, `"abc def"`) exits ≠ 0 with **`ZOMBIE_API_URL=http://127.0.0.1:1`**; the JSON envelope's `error.message` matches the stem emitted by `validate.js → validateRequiredId`; captured stderr does NOT contain `ECONNREFUSED` (proves no network call fired). The validator catches client-side; the API is not stressed. (Item 10, invalid-format slice.) |
| `lifecycle-with-token.spec.js → captured output never contains the minted JWT` | For every `runZombiectl` call across §4a/§4b/§4c, `stdout + stderr` substring search for the minted JWT returns no match. WS-E #C1 regression. |
| `lifecycle-after-login.spec.js §5a → completes the real CLI auth handshake against api-dev` | `zombiectl login --no-open --no-input` emits a parseable `login_url`; browser leg navigates + clicks approve; CLI subprocess exits 0; `credentials.json` exists, mode `0600`, `token` is a 3-segment JWT. DEV-only. (Item 5.) |
| `lifecycle-after-login.spec.js §5b → persisted-credentials read-only sweep without ZOMBIE_TOKEN` | After §5a completes, for every row in `READ_ONLY_COMMANDS`, `runZombiectl(row.args, { env: { ZOMBIE_STATE_DIR } })` (NOTE: no `ZOMBIE_TOKEN` in env) exits 0; same JSON shape assertions as §4b. Proves `credentials.json` is the load-bearing auth source. (Item 6 expanded.) |
| `lifecycle-after-login.spec.js §5c → persisted-credentials install + lifecycle without ZOMBIE_TOKEN` | After §5a completes, with `ZOMBIE_TOKEN` absent from env: `zombiectl install --from samples/platform-ops --name <unique> --json` exits 0; subsequent `zombie list --json` includes the new id; `status` → `stop` → `resume` → `kill` walks the same states as §4a. (Item 9.) |
| `lifecycle-after-login.spec.js → credentials.json mode is 0600` | `fs.stat(credentialsPath).mode & 0o777 === 0o600`. WS-E #C3 regression. |
| `lifecycle-after-login.spec.js → temp state dir scoping holds` | `ZOMBIE_STATE_DIR` is a tmpdir prefix; the resolved `credentials.json` path is inside it; `afterEach` removes the dir. |
| `lifecycle-after-login.spec.js → every spawn after login has no ZOMBIE_TOKEN in env` | For every `runZombiectl` call in §5b/§5c, the constructed child env object MUST NOT contain a `ZOMBIE_TOKEN` key. Asserted by the helper inspecting the env it was handed before spawn. |
| `runZombiectl never mutates process.env` (in-suite invariant, not a separate test) | Before/after snapshot of `process.env.ZOMBIE_TOKEN` around every spawn is identical to the value the suite started with. WS-E #C1 regression. |
| `cli-acceptance-prod first action: zombiectl --version equals package.json version` | The just-installed CLI's `--version` output equals the `version` field in the published `package.json`. Catches npm replication lag. |
| `flags-and-env.spec.js §6a → --version short-circuits before --help` | `zombiectl --version --help` and `zombiectl --help --version` both exit 0 with stdout matching the version string (NOT the help body). Regression for the documented precedence in `cli.js:54-77`. |
| `flags-and-env.spec.js §6a → --version --json emits JSON` | `zombiectl --version --json` exits 0; stdout parses as JSON; the parsed object has key `version` matching `package.json`. |
| `flags-and-env.spec.js §6a → --help --json emits JSON` | `zombiectl --help --json` exits 0; stdout parses as JSON (existing `printHelp(stdout, …, { jsonMode: global.json })`). |
| `flags-and-env.spec.js §6b → --api overrides ZOMBIE_API_URL` | `ZOMBIE_API_URL=http://127.0.0.1:1 zombiectl --api <real-api-dev> workspace list --json` exits 0 (hits the real API, not the unroutable env value). |
| `flags-and-env.spec.js §6b → ZOMBIE_API_URL honored when --api absent` | `ZOMBIE_API_URL=<real-api-dev> zombiectl workspace list --json` (no `--api`) exits 0. |
| `flags-and-env.spec.js §6c → credentials.json overrides ZOMBIE_TOKEN` | With a valid `credentials.json` AND `ZOMBIE_TOKEN=garbage` in env, `zombiectl workspace list --json` exits 0 (creds wins). Regression for the README's "Auth token (overridden by login)" claim. |
| `flags-and-env.spec.js §6c → ZOMBIE_API_KEY auths as admin` | With a vault-provisioned admin key in `ZOMBIE_API_KEY` and no `ZOMBIE_TOKEN`/creds, `zombiectl workspace list --json` exits 0. Skipped if the vault item is absent. |
| `flags-and-env.spec.js §6c → NO_COLOR disables ANSI codes` | `NO_COLOR=1 zombiectl --help` stdout contains zero `\x1b[` escape sequences. |
| `flags-and-env.spec.js §6d → --no-open does NOT spawn a browser` | During `zombiectl login --no-open --no-input`, the child process tree contains zero `chromium`/`Chrome`/`Safari` PIDs descended from the CLI's PID. Stdout still contains a parseable `login_url`. |
| `flags-and-env.spec.js §6e → SIGINT on `zombiectl login --no-open` exits cleanly` | Spawn the CLI, await first poll line on stdout, send `child.kill("SIGINT")`. Asserts: child exits with non-zero code; `credentials.json` does NOT exist in the tmpdir-scoped `ZOMBIE_STATE_DIR`. |
| `flags-and-env.spec.js §6e → SIGINT on `zombiectl login` (no --no-open) exits cleanly` | Same as above but with `--no-open` omitted. Browser-process cleanup not asserted (OS-dependent). |

Negative tests — `help-and-errors.spec.js` covers the parse + auth-guard negatives; `§4c` covers the auth-required invalid-arg-value negatives. Every Failure Mode row maps to either an existing positive test or one of the negative-matrix sweeps above.

Regression tests — every existing `zombiectl/test/*.test.js` MUST continue to pass. The new acceptance suite is additive; the existing `bun run test` glob excludes `test/acceptance/`.

---

## Acceptance Criteria

- [ ] M65_001 vault prerequisite met (inherited gate) — verify: `op read 'op://VAULT/e2e-fixtures/regular/email'` returns a non-mailinator domain.
- [ ] `bun run test:acceptance` against local `ZOMBIE_ACCEPTANCE_TARGET=https://api-dev.usezombie.com` runs all three specs green — verify: paste the green run line.
- [ ] `cli-acceptance-dev` passes on `deploy-dev.yml` for the implementation PR's branch — verify: link the GH Actions run URL.
- [ ] `cli-acceptance-prod` passes on `post-release.yml` for the first release after this PR merges — verify: link the GH Actions run URL.
- [ ] `help-and-errors.spec.js` runs on PROD (no auth needed) and DEV — verify: both targets show all of its tests green.
- [ ] `lifecycle-after-login.spec.js` is skipped against PROD — verify: dry-run with `ZOMBIE_ACCEPTANCE_TARGET=https://api.usezombie.com` shows the spec as `skipped`.
- [ ] Help triplet (item 1) — `zombiectl`, `zombiectl help`, `zombiectl -h`, `zombiectl --help` exit 0 with identical help output — verify: matrix in `help-and-errors.spec.js` passes.
- [ ] Invalid subcommand on every group (item 2) — verify: `grep -c "expectInvalidSubcommand" zombiectl/test/acceptance/help-and-errors.spec.js` ≥ number of groups in `COMMAND_GROUPS`.
- [ ] Missing required arg (item 3) — verify: `grep -c "expectMissingArg" zombiectl/test/acceptance/help-and-errors.spec.js` ≥ number of rows in `REQUIRES_POSITIONAL_ARG`.
- [ ] Read-only sweep covered in BOTH auth contexts (items 4 + 8) — verify: each of `lifecycle-with-token.spec.js` and `lifecycle-after-login.spec.js` iterates `READ_ONLY_COMMANDS` (`grep -c "READ_ONLY_COMMANDS" zombiectl/test/acceptance/{lifecycle-with-token,lifecycle-after-login}.spec.js` is 1:1).
- [ ] Invalid-arg-value matrix (item 10) — verify: `grep -c "expectInvalidArgValue" zombiectl/test/acceptance/lifecycle-with-token.spec.js` ≥ number of rows in `REQUIRES_IDENTIFIER`; each `expectedErrorCode` is registered in `src/errors/error_registry.zig`.
- [ ] Persisted-credentials path proven without `ZOMBIE_TOKEN` (items 6, 9) — verify: in `lifecycle-after-login.spec.js`, every spawn after §5a passes an env that explicitly excludes `ZOMBIE_TOKEN`. Asserted by the helper.
- [ ] Global-flag combinations (§6a) — verify: `flags-and-env.spec.js` has explicit tests for `--version --help` (version wins), `--version --json` (JSON shape), `--help --json` (JSON shape).
- [ ] `--api` and `ZOMBIE_API_URL` precedence (§6b) — verify: both precedence assertions are present in `flags-and-env.spec.js`.
- [ ] `credentials.json` overrides `ZOMBIE_TOKEN` env (§6c) — verify: `grep -n "garbage\|overrides.*ZOMBIE_TOKEN\|creds.*wins" zombiectl/test/acceptance/flags-and-env.spec.js`.
- [ ] `NO_COLOR` regression (§6c) — verify: `grep -n "NO_COLOR" zombiectl/test/acceptance/flags-and-env.spec.js`.
- [ ] `--no-open` browser-suppression assertion (§6d) — verify: `grep -n "chromium\|process.tree\|no.*browser" zombiectl/test/acceptance/flags-and-env.spec.js` ≥ 1 match.
- [ ] SIGINT regression (§6e) — verify: `grep -c "SIGINT\|child.kill" zombiectl/test/acceptance/flags-and-env.spec.js` ≥ 2 (both `--no-open` and bare `login`).
- [ ] Empty-list standard message (§4b' + §5b') — verify: `grep -c "EMPTY_LIST_CONVENTIONS\|no workspaces\|items.*\\[\\]" zombiectl/test/acceptance/{lifecycle-with-token,lifecycle-after-login}.spec.js` ≥ 2 in each file; matrix-driven over `READ_ONLY_COMMANDS`.
- [ ] Invalid-format ID does NOT stress the API (§4c2) — verify: in `lifecycle-with-token.spec.js`, the §4c2 block sets `ZOMBIE_API_URL=http://127.0.0.1:1` AND asserts captured stderr does NOT contain `ECONNREFUSED`. Two regression-proofs in one assertion.
- [ ] Error message stem matches `validate.js` (§4c2) — verify: the test reads the stem from `zombiectl/src/program/validate.js` rather than hardcoding it. RULE UFS — single source of truth.
- [ ] (Conditional) uuidv7-aware library swap landed in `validate.js` per §4c2 — verify: `grep -n "uuid\|uuidv7" zombiectl/src/program/validate.js`; AND `zombiectl/package.json` lists the chosen library under `dependencies`; AND the old regex is fully removed (RULE NLG).
- [ ] WS-E #C1 regression: captured stdout/stderr never contain the minted JWT substring — verify: `grep -c "expect.*not.toContain.*sessionJwt" zombiectl/test/acceptance/*.spec.js` ≥ 2.
- [ ] WS-E #C2 mitigation: op:// load step in both workflows is sequenced AFTER `npm i` / `bun install` — verify: `grep -n -A2 "1password/load-secrets-action" .github/workflows/{deploy-dev,post-release}.yml` shows it appearing later than the install step in YAML line order.
- [ ] WS-E #C3 regression: `credentials.json` mode 0600 assertion present — verify: `grep -n "0o600" zombiectl/test/acceptance/lifecycle-after-login.spec.js`.
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
grep -n "0o600" zombiectl/test/acceptance/lifecycle-after-login.spec.js
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
- **`lifecycle-after-login.spec.js` on PROD.** Skipped until M65_001's vault + Clerk-PROD-test-mode conditions are met.
- **A separate JS twin for `bootstrap.ts` / `svix.ts`.** The CLI suite does NOT drive webhooks. `provisionUser`/`mintTokens`/`attachJwt` is the minimum sufficient surface; bootstrap is implicit because the dashboard suite's `globalSetup` already ran the `user.created` Svix post for the shared `regular` fixture before this suite ever fires.
- **CLI behavioral changes** — except for the `help` route addition called out in Files Changed. Any OTHER bug the suite surfaces lands in a separate PR.
- **`~/Projects/docs/changelog.mdx` `<Update>`.** This PR is not user-visible; the implementation PR adds the changelog entry.

---

## Discovery (out-of-scope but adjacent observations the implementing agent SHOULD surface)

1. **Daily cron for `cli-acceptance-prod`.** Backend changes that ship without a CLI release still need to re-exercise the published CLI. A daily cron is the cheapest cover. If the implementing agent finds an existing daily cron job in `.github/workflows/`, they reuse its expression; otherwise they add a new one. Either way, surface in the PR.
2. **`zombiectl login` browser-handoff page selectors.** The dashboard's `/cli-auth/{session_id}` (or whatever path the app actually uses — implementing agent reads `ui/packages/app/app/` to confirm) is the click-target. If the page selector drifts, `lifecycle-after-login.spec.js` fails loud. Worth documenting the page's selector contract in `docs/AUTH.md` alongside the CLI carve-out — but only if a stable test-id already exists. If the dashboard uses ad-hoc labels, surface as a follow-on.
3. **`scripts/audit-runtime-imports.mjs` extension.** That script already audits `src/`; extending it to also audit `test/acceptance/` for `ui/packages` imports closes the Invariant #5 loop deterministically (instead of relying on a one-line grep in CI). Worth a follow-on if it's a small change.
4. **Cross-runtime constants drift.** `zombiectl/test/acceptance/fixtures/constants.js` and `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` carry the same identifier set. RULE UFS calls for one literal, every reader — but across runtimes the literal must be duplicated. The implementing agent should consider whether a build-time generator (read one source, emit both files) is warranted, or whether a CI grep ("both files have `CLERK_API_BASE = ...`") is enough. Default: CI grep.
5. **`postinstall.mjs` scrutiny.** `cli-acceptance-prod` is the first job to run `@usezombie/zombiectl`'s postinstall under CI with secrets in scope. Worth a separate security pass on `scripts/postinstall.mjs` — what does it read, what does it write, what does it phone home about. Surface findings in the PR; landing a fix is a separate spec if needed.
6. **CLI/backend ID-format invariant mismatch.** The CLI's `validate.js` accepts any UUID format OR a 4-128 char alphanumeric ID; the backend's `src/types/id_format.zig` generates strict uuidv7 (`generateZombieId`, `generateWorkspaceId`, etc. → `allocUuidV7`) AND validates incoming IDs via `isUuidV7`. The CLI is more permissive than the backend invariant. Captain's preference (recorded in §4c2): swap the CLI validator to a uuidv7-aware library if supply-chain posture allows. If the library swap lands in this milestone, this Discovery item is closed. If it doesn't, the mismatch is documented here for a follow-on hygiene PR — risk is that the CLI happily accepts a uuidv4 from a user (e.g. copy-pasted from another system) and only the backend reports the 404, costing one stressed-API call per mistake. A tightened CLI validator catches it client-side.
7. **Empty-list message convention gaps.** `workspace.js:95` shows the canonical "no \<items\>" pattern (`ui.info("no workspaces")`). Spec §4b' / §5b' will sweep every `list` command and surface any that does NOT emit the standard stem (e.g. silently exits with no output, or emits a different stem). Each gap is a CLI bug to fix in a follow-on PR — the spec asserts the EXPECTED behavior, and a missing implementation surfaces as a test failure with a clear name. Implementing agent records each gap as a Discovery row in the implementation PR's session notes.

---

## Branch + PR conventions for this spec PR

- Branch: `chore/m65-002-spec-zombiectl-e2e-lifecycle` (off `main`).
- Single commit: `chore(spec): add M65_002 — zombiectl e2e full lifecycle scenarios`.
- PR title: `chore(spec): M65_002 — zombiectl e2e full lifecycle scenarios`.
- PR body links: this spec file, `docs/v2/pending/M65_001_…`, `docs/AUTH.md` "PROD fixture identity carve-out" anchor.
- No `/review` skill chain on this PR — the chain runs on the implementation PR per the table above.
- Captain inspects, prioritises, and opens the implementation milestone separately.
