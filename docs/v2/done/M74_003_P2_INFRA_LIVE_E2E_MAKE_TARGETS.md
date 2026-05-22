<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_003: e2e/acceptance/dry make targets mirror CI + acceptance hardening + zombiectl login non-interactive path & DX hardening

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 003
**Date:** May 21, 2026
**Status:** DONE
**Priority:** P2 (B1) — local make targets pointed at the wrong things, two valueless CI jobs, a flaky/red acceptance suite. P1 (B2) — `zombiectl login` has no non-interactive direct-token path (Continuous Integration (CI)/scripts cannot pass a token), carries a stale `ZMB_TOKEN` env alias that wins over the canonical `ZOMBIE_TOKEN`, and the verification-code prompt has developer-experience gaps (poll-lag before the prompt, no client-side validation, unclean Ctrl-C / End-of-File (EOF) handling).
**Categories:** INFRA, TESTING, AUTH, CLI
**Batch:** B1 (make-targets + acceptance hardening — DONE) · B2 (zombiectl login non-interactive path + env consolidation + prompt DX — DONE)

> **Scope expansion (B2).** B1 (make-targets + acceptance fixtures) already shipped — all `DONE` below. Per Indy's explicit direction the `zombiectl login` changes are folded into this same spec file + PR #339 rather than split to their own milestone (the split-security-features default would carve them out — registered, overridden). **The login flow itself is UNCHANGED (Pick A — keep the typed verification code + Elliptic-Curve Diffie-Hellman (ECDH) handshake, matching the Supabase reference CLI at `~/Projects/oss/cli/apps/cli/src/next/commands/login`).** A loopback + Proof Key for Code Exchange (PKCE) redesign was considered and **dropped**: the reference does not use loopback, so adopting it would diverge from the reference (needs ack) AND defeat reference parity. B2 is bounded to four moves: (1) a non-interactive direct-token path (Supabase `resolveToken` shape — `--token` > `ZOMBIE_TOKEN` env > piped stdin > browser); (2) delete the `ZMB_TOKEN` env alias, `ZOMBIE_TOKEN`-only; (3) prompt UX + edge-case hardening with tests; (4) document two used-but-undocumented env vars. AUTH.md gets a light env-var/non-interactive touch — no threat-model rewrite, since the flow is unchanged. Docs PR [`usezombie/docs#67`](https://github.com/usezombie/docs/pull/67) documents the flow we are KEEPING, so it stays valid and can merge.
**Branch:** feat/m74-003-login-loopback (B2 working branch, off `feat/m74-003-live-e2e-auth-portability`; merges into PR #339). The `-login-loopback` suffix is stale from the abandoned loopback premise — harmless, not renamed.
**Depends on:** None.
**Provenance:** Consolidates the original M74_003 (authored wrongly as "restore the `src/auth/` portability gate via an `error_registry` named module") and M74_004 (`make live-e2e-all` placeholder-filter cleanup) into one workstream. **Two corrections happened during implementation:** (1) `live-e2e-*` is the **live-API acceptance** namespace, not a Zig compile gate — the abandoned `error_registry` sweep is parked in a branch stash, do not resurrect; (2) the interim wiring `live-e2e-all: _test-integration-full` made `live-e2e-all` a behavioural twin of `make test-integration` — a duplicate target with no added signal. Caught against memory (`live-e2e-all` must not clone `test-integration`) and independently confirmed by the M74_004 sibling agent before it tore down. **Resolution: `live-e2e-all` is removed entirely** — `acceptance-e2e` + `cli-acceptance` are the correct local twins of the CI acceptance jobs, and `test-integration` already owns the full backend suite. The acceptance-suite drift fixes that lived uncommitted in the M74_004 worktree are folded in here.

**Canonical architecture:** `make/test-integration.mk` (`_test-integration-full` — the canonical infra-up + migrate + env-threaded `zig build test`, aliased by `test-integration`) + the auth-acceptance jobs in `.github/workflows/{deploy-dev,smoke-post-deploy}.yml` + the Playwright acceptance suite (`ui/packages/app/playwright.acceptance.config.ts`).

---

## Implementing agent — read these first

1. `make/test-integration.mk` (`_test-integration-full`) — the canonical full-suite recipe, aliased by `test-integration`. It is NOT re-exposed under an acceptance name; the backend tier owns it.
2. `.github/workflows/deploy-dev.yml` — `acceptance-e2e-dev` (`bun run test:e2e:acceptance`) + `cli-acceptance-dev` (`bun run test:acceptance`) are the CI jobs the `acceptance-e2e` / `cli-acceptance` make targets mirror 1:1.
3. `ui/packages/app/package.json` (`test:e2e:acceptance*`) + `zombiectl/package.json` (`test:acceptance`) — the acceptance suites (signup, login, lifecycle, install). The Playwright acceptance fixtures live under `ui/packages/app/tests/e2e/acceptance/`.
4. `docs/AUTH.md` — the "re-mint on 401" posture the acceptance api-client now implements (the cached Bearer outlives its TTL on a long serial run).
5. `make/test.mk` — the include orchestrator; `dry.mk` is added beside `acceptance.mk`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Mirror e2e/acceptance/dry make targets to the pipeline + harden the dashboard acceptance suite
- **Intent (one sentence):** make-target names and bodies match what CI actually runs, the false-positive 0-test gate and the duplicate `live-e2e-all` target are gone, the dry lanes are honest UI-only page renders, and the dashboard acceptance suite is green again.
- **Handshake:** five coordinated moves — (1) retire the mislabeled `live-e2e-auth` Zig gate → `acceptance-e2e` + `cli-acceptance`; (2) **remove** `live-e2e-all` (it was a duplicate of `test-integration`); (3) dry lanes → website+app only, moved to `make/dry.mk`; (4) drop the `dry-backend` / `dry-backend-smoke` CI jobs; (5) fold in the acceptance-suite drift fixes (TRIGGER.md schema, re-mint on 401, binary path) + the local acceptance-ladder runner. No production Zig source change.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it: dead `_e2e_backend`/filter chain removed, not shimmed); RULE NLG (no legacy framing pre-2.0).
- `docs/AUTH.md` — the acceptance fixtures touch Clerk session-token minting; the re-mint-on-401 change follows AUTH.md's documented recovery posture (test fixtures only — no production auth surface).
- `docs/ZIG_RULES.md` — N/A; no `*.zig` touched (recipes *run* `zig build`, they don't edit Zig).
- `feedback_validate_spec_intent_vs_architecture` — the duplicate-target smell (`live-e2e-all` collapsing into `test-integration`) is a design error: stop and surface, don't ship it. This spec is the corrected design.

---

## Applicable Gates

> Blast radius: `make/acceptance.mk`, `make/dry.mk` (new), `make/test.mk`, `Makefile`, two `.github/workflows/dry*.yml`, and five test-only files under `ui/packages/app/`. No production source files.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / SCHEMA / ERROR REGISTRY / LOGGING | no | Makefile/YAML + test-fixture TS only; none of these surfaces touched. |
| Length / UFS / UI / DESIGN TOKEN | watch | TS edits are test fixtures under `tests/e2e/acceptance/`; repeated literals named (`UNAUTHORIZED = 401`); no app components, no Tailwind. |
| Milestone-ID Gate | watch | `make/` + `.github/` + test fixtures are outside `docs/` — no `M74_003`/`§` IDs in recipe bodies, comments, or test names. |
| check-gh-actions-valid (pre-commit) | yes | `.github/workflows/**` edited — `actionlint` must pass; verified clean on both `dry.yml` + `dry-smoke.yml`. **CI/CD edits authorized by Indy this session** (the two `dry-backend*` jobs "have no value"). |

---

## Overview

**Goal (testable):**
- `make acceptance-e2e` → `cd ui/packages/app && bun run test:e2e:acceptance`; `make cli-acceptance` → `cd zombiectl && bun run test:acceptance` (mirror CI `acceptance-e2e-*` / `cli-acceptance-*`).
- `make live-e2e-all` no longer exists — `make -n live-e2e-all` errors with "No rule to make target".
- `make dry` / `make dry-smoke` run website + app Playwright only (no backend leg); both live in `make/dry.mk`.
- `make/acceptance.mk` contains only `acceptance-e2e`, `cli-acceptance`.
- The `dry-backend` + `dry-backend-smoke` CI jobs are removed.
- The dashboard acceptance suite runs without the TRIGGER.md-schema, token-expiry, and binary-path failures (smoke `7/8 → 8/8`, zero token-expiry failures).
- `ui/packages/app/package.json` exposes a local acceptance ladder: `test:e2e:acceptance:{local,dev,prod,all}`.

**Problem:** Defects compounded across the make layer and the acceptance suite:
1. `make live-e2e-auth` ran `zig build test-auth` — a Zig compile-isolation gate (M18) wearing a `live-e2e-*` name. It never touched the API. No CI used it.
2. `make live-e2e-all` ran four `BACKEND_E2E_FILTER_*` placeholders matching zero `test "…"` declarations → `zig build test -Dtest-filter=<no-match>` runs 0 tests, exits 0: a silent false-positive gate. The interim fix (`live-e2e-all: _test-integration-full`) traded that for a *duplicate* of `test-integration` — same recipe, two names, no added signal.
3. `dry` / `dry-smoke` advertised "backend live-e2e + website + app" but their backend leg was the same broken filter chain; the `dry-backend` (full suite, redundant with `test-integration`) and `dry-backend-smoke` (the 0-test filter) CI jobs added cost and false signal.
4. The dashboard acceptance suite was red/flaky: TRIGGER.md fixtures used a `trigger:`/`type: api` shape the parser rejects; the cached Bearer outlived its ~15-min TTL on long serial runs (→ `UZ-AUTH-003 Token expired`); and the CLI binary path pointed at `zombiectl/bin/` instead of `zombiectl/dist/bin/`.

**Solution summary:**
- Remove `live-e2e-auth`; add `acceptance-e2e` + `cli-acceptance` mirroring the CI jobs.
- **Remove `live-e2e-all`** — `test-integration` is the full backend suite; the acceptance lanes are `acceptance-e2e` + `cli-acceptance`. No third name aliasing either.
- Delete the entire `_e2e` / `_e2e_backend` / `_e2e_smoke` / `_zig_test_filter` / `BACKEND_E2E_*` chain.
- Move `dry` / `dry-smoke` / `dry-app*` / `_dry_website*` into `make/dry.mk` as website+app-only lanes; include it from `make/test.mk`.
- Remove the `dry-backend` (`dry.yml`) and `dry-backend-smoke` (`dry-smoke.yml`) jobs; drop them from the aggregate `needs:` arrays.
- Fix the acceptance fixtures: `triggers:` list with a single `cron` trigger; `refreshSessionToken(sessionId)` + re-mint-on-401 retry in the api-client; `zombiectl/dist/bin/zombiectl.js`. Add the `test:e2e:acceptance:{dev,prod,all}` ladder scripts.

---

## Prior-Art / Reference Implementations

- **In-repo** → `_test-integration-full` (`make/test-integration.mk`) is the canonical full-suite recipe, owned by `test-integration`. Acceptance targets do not re-alias it.
- **In-repo** → the CI `acceptance-e2e-*` / `cli-acceptance-*` jobs are the canonical acceptance commands; the make targets are their local twins.
- **In-repo** → `docs/AUTH.md`'s re-mint-on-401 posture is the model for the api-client recovery path.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/acceptance.mk` | EDIT | Trim to `acceptance-e2e` + `cli-acceptance`. Remove `live-e2e-auth`, `live-e2e-all`, the filter/`_e2e*`/smoke chain, and the dry targets. |
| `make/dry.mk` | NEW | `dry` / `dry-smoke` (website + app only) + `dry-app` / `dry-app-smoke` / `_dry_website` / `_dry_website_smoke`, moved out of `acceptance.mk`. |
| `make/test.mk` | EDIT | `include make/dry.mk` beside `acceptance.mk`. |
| `Makefile` | EDIT | Help block: replace the `live-e2e-auth` line with `acceptance-e2e` + `cli-acceptance`; remove the `live-e2e-all` line; re-describe `dry`/`dry-smoke` (website+app only). |
| `.github/workflows/dry.yml` | EDIT (authorized) | Remove the `dry-backend` job; drop it from the `dry` aggregate `needs:`. |
| `.github/workflows/dry-smoke.yml` | EDIT (authorized) | Remove the `dry-backend-smoke` job; drop it from the `dry-smoke` aggregate `needs:`. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | EDIT | TRIGGER.md fixture → `triggers:` list, single `cron` trigger (parser rejects `type: api`). |
| `ui/packages/app/tests/e2e/acceptance/install-zombie-cli.spec.ts` | EDIT | Same TRIGGER.md schema fix; CLI binary path `zombiectl/bin/` → `zombiectl/dist/bin/`. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/install-ui.ts` | EDIT | Same TRIGGER.md schema fix (third copy; missed by the scavenge). Live — imported by `signup-lifecycle`, `login-install-lifecycle`, `workspace-zombie-lifecycle`. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` | EDIT | Extract `refreshSessionToken(sessionId)` — re-mint a fresh customized-session JWT on an existing Clerk session. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/api-client.ts` | EDIT | Re-mint on HTTP 401 + retry once; align `JwtEntry` to what `global-setup.ts` writes (add `sessionId`, drop unused `cookieJwt`). |
| `ui/packages/app/package.json` | EDIT | Add `test:e2e:acceptance:{dev,prod,all}` ladder scripts (pinned to the CI dev/prod targets). |

---

## Decomposition & alternatives

- **Chosen shape:** one consolidated workstream covering the e2e/acceptance/dry make-target fixes, the dry.mk split, the CI job removals, and the acceptance-suite drift fixes + ladder runner scavenged from the M74_004 worktree.
- **Alternatives considered:** (a) keep M74_003 (auth) and M74_004 (live-e2e-all) as separate specs/PRs — rejected; same file, same category, Indy chose one merged ticket. (b) keep `live-e2e-all` as an alias of `_test-integration-full` — **rejected; duplicate-target smell** (memory + sibling-agent confirmation). (c) make `live-e2e-all` an umbrella over `acceptance-e2e` + `cli-acceptance` — rejected by Indy; the two named lanes are clearer than a third umbrella. (d) keep dry targets in acceptance.mk — rejected; dry (UI) and acceptance (auth) are distinct concerns.
- **Patch-vs-refactor verdict:** **patch + small reorg** — make-target rewires, a file split, and targeted test-fixture corrections. No production source or build-graph change.

---

## Sections (implementation slices)

### §1 — acceptance.mk: acceptance-e2e + cli-acceptance — DONE

Remove `live-e2e-auth`, `live-e2e-all`, and the `_e2e*`/filter/smoke chain. `acceptance.mk` holds only `acceptance-e2e` (→ `bun run test:e2e:acceptance`) + `cli-acceptance` (→ `bun run test:acceptance`), mirroring the CI jobs.

### §2 — dry.mk: website + app dry lanes — DONE

Move `dry` / `dry-smoke` / `dry-app` / `dry-app-smoke` / `_dry_website` / `_dry_website_smoke` into `make/dry.mk`, website+app only. Include from `make/test.mk`.

### §3 — Remove the dry-backend CI jobs — DONE

Delete `dry-backend` (`dry.yml`) + `dry-backend-smoke` (`dry-smoke.yml`); fix the aggregate `needs:` arrays. `actionlint` must stay clean.

### §4 — Makefile help — DONE

Drop `live-e2e-auth` and `live-e2e-all` from the help block; add `acceptance-e2e` + `cli-acceptance`; re-describe `dry`/`dry-smoke` (website+app only).

### §5 — Acceptance-suite drift fixes + ladder runner — DONE

TRIGGER.md fixtures → `triggers:` list with a single `cron` trigger (parser rejects `type: api`) across **all three** copies (`seed.ts`, `install-zombie-cli.spec.ts`, `install-ui.ts` — the last missed by the original scavenge, fixed under RULE NLR); `refreshSessionToken(sessionId)` + re-mint-on-401 retry in the api-client (cached Bearer outlives ~15-min TTL); CLI binary path → `zombiectl/dist/bin/`. Add `test:e2e:acceptance:{dev,prod,all}` scripts pinned to the CI dev/prod targets.

---

## Interfaces

```
make acceptance-e2e → cd ui/packages/app && bun run test:e2e:acceptance   # mirrors CI acceptance-e2e-{dev,prod}
make cli-acceptance → cd zombiectl       && bun run test:acceptance       # mirrors CI cli-acceptance-{dev,prod}
make dry            → _dry_website dry-app               # website + app Playwright (no Clerk auth)
make dry-smoke      → _dry_website_smoke dry-app-smoke   # fast website + app (no Clerk auth)

# package.json (ui/packages/app) acceptance ladder:
test:e2e:acceptance        → bunx playwright --config=playwright.acceptance.config.ts          # BASE_URL-driven
test:e2e:acceptance:local  → NEXT_PUBLIC_API_URL=localhost:3000 …                              # local rung
test:e2e:acceptance:dev    → BASE_URL=usezombie-app.vercel.app NEXT_PUBLIC_API_URL=api-dev …   # dev rung
test:e2e:acceptance:prod   → BASE_URL=app.usezombie.com       NEXT_PUBLIC_API_URL=api …        # prod rung
test:e2e:acceptance:all    → local && dev && prod                                              # full ladder

# fixtures (test-only):
refreshSessionToken(sessionId): Promise<string>   # mint a fresh customized-session JWT on an existing session
clientFor(handle): re-mints on 401 + retries once
```

No HTTP/REST/OpenAPI/CLI/schema surface. The acceptance suites read their standard env (`BASE_URL`/`NEXT_PUBLIC_API_URL`/`CLERK_*`; `ZOMBIE_ACCEPTANCE_TARGET`) exactly as the CI jobs supply them.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `acceptance-e2e` fails to start | `CLERK_*` / base URL unset, Playwright not installed | Operator/CI supplies env + browser (op:// Clerk creds), as `acceptance-e2e-dev` does. |
| `cli-acceptance` live legs skip | `ZOMBIE_ACCEPTANCE_TARGET` unset / non-https | Expected — suite self-skips (52 pass / 2 skip locally). |
| Acceptance `create_zombie` 400 on TRIGGER.md | fixture used `trigger:`/`type: api` (parser rejects) | Fixtures emit a `triggers:` list with a single `cron` trigger — the smallest valid shape. |
| Acceptance `UZ-AUTH-003 Token expired` mid-run | cached Bearer outlived its ~15-min TTL on a long serial run | `clientFor` re-mints on 401 via `refreshSessionToken(sessionId)` and retries once (AUTH.md posture). |
| `install-zombie-cli` spec can't find the binary | path pointed at `zombiectl/bin/` | Corrected to `zombiectl/dist/bin/zombiectl.js` (matches `package.json` bin). |
| A `dry*` CI job breaks on job removal | stale `needs:` referencing a removed job | Both aggregate `needs:` arrays updated; `actionlint` verified clean. |
| `test:e2e:acceptance:all` mutates prod fixtures from a laptop | the `:prod` rung runs against `app.usezombie.com` | Opt-in only (developer must run `:prod`/`:all` explicitly); mirrors CI `acceptance-e2e-prod`. See Discovery. |

---

## Invariants

1. **`make/acceptance.mk` contains only `acceptance-e2e`, `cli-acceptance`** — `grep -nE '_e2e_backend|_e2e_smoke|_zig_test_filter|BACKEND_E2E|live-e2e-auth|live-e2e-all|^dry' make/acceptance.mk` returns empty.
2. **`live-e2e-all` no longer exists** — `make -n live-e2e-all` errors with "No rule to make target"; `grep -rn 'live-e2e-all' make/ Makefile` returns empty.
3. **`dry` / `dry-smoke` are website+app only** — `make -n dry` shows no backend/`zig` step.
4. **No workflow references `dry-backend`, `make live-e2e-all`, or `make _e2e_smoke`** — and `actionlint` passes.
5. **Acceptance TRIGGER.md fixtures emit no parser-rejected `type: api`** — `grep -rnE '"[[:space:]]*type:[[:space:]]*api"' ui/packages/app/tests/e2e/acceptance/` returns empty (matches the emitted YAML string literal, not explanatory prose comments mentioning `type: api`).

---

## Acceptance Criteria

- `make -n acceptance-e2e` → `cd ui/packages/app && bun run test:e2e:acceptance`.
- `make -n cli-acceptance` → `cd zombiectl && bun run test:acceptance`.
- `make -n live-e2e-all` errors ("No rule to make target"); `grep -n 'test-auth\|-Dtest-filter\|live-e2e-all' make/acceptance.mk` returns nothing.
- `make -n dry` / `make -n dry-smoke` show only website + app commands, no `zig`.
- `grep -rn 'dry-backend\|make live-e2e-all\|make _e2e_smoke' .github/workflows/` returns nothing; `actionlint` clean.
- `cd zombiectl && bun run test:acceptance` runs green locally (live legs self-skip): pass / skip / 0 fail.
- `make lint-app` clean (Oxlint + `tsc --noEmit`) with the fixture changes.
- `grep -rnE '"[[:space:]]*type:[[:space:]]*api"' ui/packages/app/tests/e2e/acceptance/` returns nothing (emitted YAML, comments excluded).

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| `make -n` for each target | dispatches to the matching pipeline command / recipe; `live-e2e-all` is unknown | local / CI |
| `bun run test:acceptance` (zombiectl) | CLI auth lifecycle suite executes; live legs self-skip | local |
| `actionlint .github/workflows/dry*.yml` | edited workflows valid after job removal | pre-commit / local |
| Playwright acceptance suite (dev rung) | TRIGGER.md fixtures accepted; no token-expiry; CLI install found at `dist/bin` | CI `acceptance-e2e-dev` / local with Clerk DEV creds |
| `make lint-app` | fixture TS changes pass Oxlint + `tsc --noEmit` | local / CI |

The acceptance/integration assertions themselves are owned by M65_001, M65_002, and the existing integration suite; this spec wires the entrypoints and corrects the fixtures that the suite depends on.

---

## B2 — zombiectl login: non-interactive path + env consolidation + prompt DX

> Everything below is B2. B1 (Sections §1–§5, all `DONE`) is unchanged. **The login *flow* is unchanged** — typed verification code + ECDH handshake stay (Pick A, matching the Supabase reference `~/Projects/oss/cli/apps/cli/src/next/commands/login`). B2 only adds a non-interactive path, consolidates the token env var, hardens the prompt, and documents two strays. AUTH.md gets a light env-var/non-interactive touch — **no Flow 1 threat-model rewrite**, since the protocol is unchanged.

### B2 Problem

Three independent gaps on the shipped `zombiectl login`:
1. **No non-interactive path.** `loginCore` always runs the browser device flow. There is no `--token` flag and no piped-stdin token; the only non-interactive credential is exporting `ZOMBIE_TOKEN` out-of-band, which `login` merely *warns* about (`login-device-flow.ts:envTokenAwareness`). The Supabase reference handles this with `resolveToken` (`--token` > env > piped stdin > browser) + a direct `saveDirectToken`.
2. **`ZMB_TOKEN` is a stale alias that *wins*.** The resolver treats `ZMB_TOKEN` as canonical and prefers it over `ZOMBIE_TOKEN` (`auth-token.ts:15,76`, `config.ts:56,61`), while every other env var is `ZOMBIE_`-prefixed. Two names for one thing, and the outlier wins.
3. **Prompt DX / edge-case gaps.** The CLI polls `GET /sessions` to `verification_pending` *then* prompts — up to one poll interval of lag after Approve, behind a bare spinner. The prompt (`input.readLine`) has no client-side validation (empty/garbage round-trips to the server as `UZ-AUTH-018`), and `catch → ""` masks cancel/EOF. Ctrl-C at the prompt is not routed to the clean exit-130 path the poll phase has; Ctrl-D/EOF can loop.

### B2 Design

1. **Non-interactive resolve order** (mirrors Supabase `resolveToken`): `--token <pat>` flag → `ZOMBIE_TOKEN` env → piped stdin (non-TTY) → browser device flow. A directly-supplied token is validated (`pingMe`) and persisted to `credentials.json` (0o600) with no browser. The interactive device flow (typed code + ECDH) is the unchanged fallback.
2. **`ZOMBIE_TOKEN`-only.** Delete every `ZMB_TOKEN` env reference (and the lowercase `zmb` local in `auth-token.ts:76`); `ZOMBIE_TOKEN` is the single name. **Surgical** — the `zmb_` / `zmb_t_` API-key prefixes, `UZ-ZMB-*` error codes, and `ZMB_CD_*` vault names are NOT touched (case-insensitive blast-radius sweep confirms the overload).
3. **Prompt UX.** Prompt for the code immediately after opening the browser (drop the poll-gate lag, matching Supabase); validate the 6-digit shape client-side before the network call; route Ctrl-C to the existing interrupted→exit-130 path (no partial `credentials.json`); make EOF a clean exit, not a loop; empty Enter re-prompts locally.
4. **Env-var doc hygiene.** Add the two used-but-undocumented vars (`ZOMBIE_DASHBOARD_URL`, `ZOMBIE_PROGRESS_STYLE`) to the `--help` env block + golden. No removals — both have live callers.

### B2 Sections (implementation slices)

#### §B2.1 — Non-interactive direct-token path — DONE
Add a `resolveToken`-equivalent ahead of the device flow in `login.ts`: `--token` flag (new, on `cli-tree.ts`) → `ZOMBIE_TOKEN` env → piped stdin (non-TTY) → browser. Direct token → `validateToken` / `pingMe` → persist → done, no browser. Keep `--token-name`, `--no-open`, `--timeout-sec`, `--force`, `--no-input`.

#### §B2.2 — `ZMB_TOKEN` → `ZOMBIE_TOKEN` consolidation — DONE
Remove `ZMB_TOKEN` as the canonical/winning env name across `auth-token.ts` (incl. the `zmb` local at :76), `config.ts`, `cli.ts`, `argv-redact.ts`, `login-device-flow.ts` (`ZMB_TOKEN_ENV_KEYS` / `envTokenAwareness`), `cli-tree.ts` help text. Regenerate the `help-no-color.txt` golden. Update `auth-token-resolve.unit.test.ts`, `login-effect.unit.test.ts`, `login-device-flow.unit.test.ts`. **Prefix-safe per the case-insensitive sweep** — `zmb_` / `zmb_t_` / `UZ-ZMB-*` / `ZMB_CD_*` untouched.

#### §B2.3 — Prompt UX: immediate prompt + client-side validation — DONE
In `login.ts` / `login-device-flow.ts`: prompt for the code immediately after opening the browser. **The poll was dropped entirely** (Open-Decision-#2 = A, Supabase-faithful): `pollUntilVerificationPending` / `pollSessionStatus` / `PollOutcome` / the `expired`/`timeout` outcome model / `failedOutcomeError` / the "waiting for browser approval" spinner are removed. Possessing the code implies the dashboard approved; expiry surfaces at `/verify` (410). The 6-digit shape is validated in `promptVerificationCode` before `submitVerificationCode` — empty/garbage re-prompts locally with no round-trip; the dead `UZ-AUTH-018` server-malformed branch is removed. **`--timeout-sec` / `--poll-ms` removed** (poll-era flags, orphaned by the drop — Indy 2026-05-22 "if poll is gone, remove"); `--timeout-sec`'s "keep" in §B2.1 is superseded.

#### §B2.4 — Edge-case hardening (Ctrl-C / EOF / Enter) + tests — DONE
SIGINT at the code prompt aborts via the prompt's `AbortSignal` (`withSigintAbort` now wraps the prompt, not the poll) → `InterruptedError` (exit 130), nothing persisted. `input.readLine(prompt, signal?)` returns `string | null` — `null` (EOF / closed stdin / abort) is a clean cancel, `""` (bare Enter) re-prompts; the `catch → ""` masking is gone. `promptYesNo` treats `null` as "don't proceed". Tests: client-side re-prompt with no round-trip, EOF→InterruptedError, `--no-input`→abort, cancel→no creds written. **Live caveat:** the device flow is now terminal-only, so the spawned-binary live device-flow acceptance (`lifecycle-after-login.spec.ts`, flags-and-env) can't drive it without a PTY harness — reframed/noted as a follow-up; mechanics are unit-covered.

#### §B2.5 — Env-var doc hygiene — DONE
Add `ZOMBIE_DASHBOARD_URL` (`config.ts:55`) + `ZOMBIE_PROGRESS_STYLE` (`ui-progress.ts:43`) to the `cli-tree.ts` env block; regenerate the golden help. No removals. Both added to the `helpTail()` env block (URL var beside `ZOMBIE_API_URL`, style var beside `NO_COLOR`); `test/golden/help-no-color.txt` regenerated; golden suite (byte-exact + ≤80-col) green.

#### §B2.6 — AUTH.md + user-docs light touch — DONE
`docs/AUTH.md`: update token env-var references (`ZMB_TOKEN` → `ZOMBIE_TOKEN`) and add a sentence on the non-interactive `--token` / env / stdin path. **No Flow 1 threat-model rewrite — the protocol is unchanged.** `usezombie/docs#67` stays valid (it documents the kept flow); add the `--token` flag + the corrected env-var name where it lists them. The `ZMB_TOKEN`→`ZOMBIE_TOKEN` half was a no-op (AUTH.md never referenced `ZMB_TOKEN`); added a "Non-interactive token seeding" note in Flow 1 covering the `--token`/`ZOMBIE_TOKEN`/stdin resolve order, validate-first persist, and the device flow staying terminal-only. `usezombie/docs#67` covers the user-docs half (pushed, greptile-clean).

### B2 Interfaces

```
# zombiectl login — flags
--token <pat>          NEW — non-interactive: validate + persist a token, no browser
--token-name <label>   keep
--no-open / --no-input / --timeout-sec / --force   keep

# Token resolution order (new)
--token flag > ZOMBIE_TOKEN env > piped stdin (non-TTY) > interactive device flow

# Env var (consolidated)
ZOMBIE_TOKEN           the only auth-token env var (ZMB_TOKEN deleted)

# Backend / dashboard / device-flow protocol — UNCHANGED (Pick A)
```

### B2 Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `--token <invalid>` | bad/expired PAT supplied directly | `pingMe` rejects → exit ≠ 0, **nothing persisted**. |
| Piped stdin but empty | `printf '' \| zombiectl login` | Treated as no token → falls through to browser flow (or fails under `--no-input`). |
| Ctrl-C at the code prompt | user aborts mid-prompt | `InterruptedError` → exit 130, no partial `credentials.json` (same as poll phase). |
| Ctrl-D / EOF at the prompt | closed stdin / piped EOF | Clean non-zero exit — **no re-prompt loop** (current `catch → ""` masking removed). |
| Empty / non-6-digit Enter | typo | Client-side validation → local re-prompt, **no server round-trip**. |
| `ZMB_TOKEN` still set in a user's shell post-upgrade | muscle memory / old docs | No longer honored; `--help` + AUTH.md document `ZOMBIE_TOKEN`. (Acceptable break — pre-2.0, RULE NLG: no compat alias.) |

### B2 Invariants

1. **`ZMB_TOKEN` is gone** — `git grep -i 'ZMB_TOKEN' -- zombiectl/src zombiectl/test` returns 0; the `zmb_` / `zmb_t_` / `UZ-ZMB-*` / `ZMB_CD_*` families remain untouched.
2. **`ZOMBIE_TOKEN` resolves auth everywhere** — no env path depends on `ZMB_TOKEN`.
3. **Direct token = no browser** — `--token` / env / stdin validates + persists without opening a browser or prompting.
4. **Ctrl-C at the prompt exits 130 with no `credentials.json` written** — same contract the poll phase already holds (M65_002 flags-and-env spec).
5. **Empty/garbage code never hits the server** — client-side 6-digit validation gates `POST /verify`.
6. **The device-flow protocol is byte-identical to M74_002** — typed code + ECDH + endpoints + state machine unchanged (Pick A).

### B2 Acceptance Criteria

- `zombiectl login --token <valid>` persists creds + exits 0 with **no browser opened**; an invalid token exits ≠ 0 and writes nothing.
- `printf '%s' "$TOK" | zombiectl login` (non-TTY) authenticates via piped stdin.
- `git grep -i ZMB_TOKEN -- zombiectl/src zombiectl/test` → 0 hits; `make lint-zombiectl` + the golden-help test green; the `zmb_t_` / `UZ-ZMB-*` families still present.
- Interactive login prompts for the code immediately after Approve (no poll-gate lag); a non-6-digit entry is rejected locally with no network call.
- Ctrl-C at the prompt → exit 130, no `credentials.json`; Ctrl-D/EOF → clean non-zero exit, no loop — both covered by new `zombiectl` tests.
- `ZOMBIE_DASHBOARD_URL` + `ZOMBIE_PROGRESS_STYLE` appear in `--help`; golden updated.

### B2 Blast radius (case-insensitive-verified)

| Area | Files | Action |
|------|-------|--------|
| CLI — non-interactive | `login.ts`, `login-device-flow.ts`, `program/cli-tree.ts`, `program/handlers-bind.ts` | add `--token` / env / stdin resolve order + direct-token persist |
| CLI — env consolidation | `auth-token.ts`, `config.ts`, `cli.ts`, `argv-redact.ts`, `login-device-flow.ts`, `cli-tree.ts` | delete `ZMB_TOKEN` (surgical), `ZOMBIE_TOKEN`-only |
| CLI — prompt UX | `login.ts`, `login-device-flow.ts`, `services/input.ts` | immediate prompt, client validation, clean cancel/EOF |
| Docs | `cli-tree.ts` help + golden `help-no-color.txt`; `docs/AUTH.md` (light); `usezombie/docs` (`--token` + env name) | document new flag + 2 strays; consolidate env name |
| Tests | `auth-token-resolve.unit.test.ts`, `login-effect.unit.test.ts`, `login-device-flow.unit.test.ts`, new edge-case `zombiectl/test/*` | env consolidation + non-interactive + edge cases |
| **Untouched** | `zmb_` / `zmb_t_` key prefixes, `UZ-ZMB-*` codes, `ZMB_CD_*` vaults; backend `sessions.zig` / `session_store_redis.zig` / `session_state.zig`; dashboard `cli-auth` page; the device-flow protocol | Pick A — flow unchanged |

### B2 Open decisions

1. **`--no-input` semantics — RESOLVED (Indy, 2026-05-22): token-only-or-fail.** `--no-input` **or** non-TTY ⟹ a token must come from `--token` / `ZOMBIE_TOKEN` / piped stdin; with none, login fails (it cannot complete the typed-code device flow non-interactively). Implementation: `resolveDirectToken` fast-fails on **non-TTY + no token** (the CI case); `--no-input` on a real TTY still fails, via the existing verify-prompt `InterruptedError` (exit 130). Consequence: **the browser device flow is now TTY-only.** The two spawned-binary acceptance tests that drove the device flow over a piped (non-TTY) stdin (`--no-open` browser-suppression, SIGINT-during-poll) were reframed to assert the new fast-fail (no browser, no partial `credentials.json`); the device-flow + abort mechanics they covered remain unit-tested. A PTY harness for spawned interactive coverage is a possible follow-up.
2. **Drop the poll entirely vs keep a lightweight expiry check — RESOLVED (Indy, 2026-05-22): drop entirely (A).** Supabase has no poll; possessing the code implies approval, and a stale session fails at `/verify` (410). No background expiry poll. The poll machinery + the `expired`/`timeout` outcome model + `--timeout-sec`/`--poll-ms` are removed.

---

## Discovery

- **`live-e2e-all` removed, not aliased.** The interim `live-e2e-all: _test-integration-full` made it a behavioural duplicate of `make test-integration` — flagged by memory (`live-e2e-all` must not clone `test-integration`) and confirmed by the M74_004 sibling agent. `test-integration` owns the full backend suite; `acceptance-e2e` + `cli-acceptance` are the acceptance lanes. No umbrella name (Indy's call).
- **`test:e2e:acceptance:all` runs the prod rung from a laptop**, which mutates real prod fixtures (signup, zombie create/kill, billing). It is opt-in and mirrors CI `acceptance-e2e-prod`, but a developer running `:all` casually will hit prod. Open question whether `:all` should chain `:prod` or stop at `:dev`. Kept as scavenged per Indy's "drift fixes + ladder runner" decision; flag for review.
- **Orphaned Zig `test-auth` build step.** `build.zig` still defines a `test-auth` step that nothing invokes and that is red on `main` (M62 `error_registry` + M74_002 `auth → src/queue/` escapes, the latter on a non-portable `queue → src/zombie/event_envelope` chain). Whether to keep/rename/remove the `src/auth/` portability concept is a separate decision — likely its own spec, since the auth→queue coupling means `src/auth/` is no longer cleanly extractable.
- **TRIGGER.md fixture is duplicated three ways** — `seed.ts:triggerMd`, `install-zombie-cli.spec.ts:triggerMd`, and `install-ui.ts:fixtureTriggerMd` each hand-roll the same minimal-valid bundle YAML (they diverged: only `install-ui.ts` still carried the rejected `type: api`, which is why one slipped the scavenge). All three now emit the valid `triggers:`/`cron` shape, but the duplication is a UFS smell — a shared `validTriggerMd()` fixture helper is a sensible follow-up (out of scope here to keep the diff to the correctness fix Indy authorized).
- **Branch name** carries a stale `-portability` suffix from the abandoned premise; harmless, not renamed.
- **§B2.1 env source = raw `ctx.env["ZOMBIE_TOKEN"]`, not `CliConfig.accessToken`.** The plan said the env token was "merged into `CliConfig.accessToken`," but at the login command `handlers-bind` overrides `accessToken` with the file-or-env-merged `ctx.token` — so reading it would treat an existing `credentials.json` as a "direct token" and re-persist it instead of opening the browser. `handlers-bind` reads the raw env value and passes it as `LoginFlags.envToken`; `resolveDirectToken` trims it.
- **§B2.1 piped stdin = a small `Stdin` service** (`src/services/stdin.ts`: `isTTY` + `readToEnd` over an injectable stream), threaded `cli.ts (io.stdin) → ctx.stdin → handlers-bind → mainLayerFor`. Mirrors the `Output`-streams seam; reads a generic Readable rather than global `Bun.stdin` so integration tests can pin TTY-ness without consuming the runner's stdin. The 5 in-process URL-resolution / failure-mode login tests inject an `isTTY:true` stdin to keep exercising the device flow.

---

## Out of Scope

- Fixing/removing the orphaned Zig `test-auth` portability gate and its M62/M74_002 boundary escapes.
- Making `src/auth/` extractable into a standalone `zombie-auth` binary.
- The abandoned `error_registry` named-module sweep (parked in a branch stash).
- Any change to the M65_001 / M65_002 acceptance assertions or the integration test bodies (only the fixtures the suite depends on are corrected).
- Whether `test:e2e:acceptance:all` should chain the prod rung (flagged in Discovery for a follow-up decision).
