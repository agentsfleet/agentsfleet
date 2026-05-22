<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_003: e2e / acceptance / dry make targets mirror CI + acceptance-suite hardening

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 003
**Date:** May 21, 2026
**Status:** DONE
**Priority:** P2 — local developer make targets pointed at the wrong things, two valueless CI jobs, and a flaky/red acceptance suite. No production-code risk; the change restores real signal, gives developers local commands that match the pipeline, and unblocks the dashboard acceptance suite.
**Categories:** INFRA, TESTING
**Batch:** B1
**Branch:** feat/m74-003-live-e2e-auth-portability
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

## Discovery

- **`live-e2e-all` removed, not aliased.** The interim `live-e2e-all: _test-integration-full` made it a behavioural duplicate of `make test-integration` — flagged by memory (`live-e2e-all` must not clone `test-integration`) and confirmed by the M74_004 sibling agent. `test-integration` owns the full backend suite; `acceptance-e2e` + `cli-acceptance` are the acceptance lanes. No umbrella name (Indy's call).
- **`test:e2e:acceptance:all` runs the prod rung from a laptop**, which mutates real prod fixtures (signup, zombie create/kill, billing). It is opt-in and mirrors CI `acceptance-e2e-prod`, but a developer running `:all` casually will hit prod. Open question whether `:all` should chain `:prod` or stop at `:dev`. Kept as scavenged per Indy's "drift fixes + ladder runner" decision; flag for review.
- **Orphaned Zig `test-auth` build step.** `build.zig` still defines a `test-auth` step that nothing invokes and that is red on `main` (M62 `error_registry` + M74_002 `auth → src/queue/` escapes, the latter on a non-portable `queue → src/zombie/event_envelope` chain). Whether to keep/rename/remove the `src/auth/` portability concept is a separate decision — likely its own spec, since the auth→queue coupling means `src/auth/` is no longer cleanly extractable.
- **TRIGGER.md fixture is duplicated three ways** — `seed.ts:triggerMd`, `install-zombie-cli.spec.ts:triggerMd`, and `install-ui.ts:fixtureTriggerMd` each hand-roll the same minimal-valid bundle YAML (they diverged: only `install-ui.ts` still carried the rejected `type: api`, which is why one slipped the scavenge). All three now emit the valid `triggers:`/`cron` shape, but the duplication is a UFS smell — a shared `validTriggerMd()` fixture helper is a sensible follow-up (out of scope here to keep the diff to the correctness fix Indy authorized).
- **Branch name** carries a stale `-portability` suffix from the abandoned premise; harmless, not renamed.

---

## Out of Scope

- Fixing/removing the orphaned Zig `test-auth` portability gate and its M62/M74_002 boundary escapes.
- Making `src/auth/` extractable into a standalone `zombie-auth` binary.
- The abandoned `error_registry` named-module sweep (parked in a branch stash).
- Any change to the M65_001 / M65_002 acceptance assertions or the integration test bodies (only the fixtures the suite depends on are corrected).
- Whether `test:e2e:acceptance:all` should chain the prod rung (flagged in Discovery for a follow-up decision).
