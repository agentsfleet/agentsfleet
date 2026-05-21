<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_003: Fix the e2e / acceptance / dry make targets to mirror the CI pipeline

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 003
**Date:** May 21, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — local developer-convenience make targets pointing at the wrong things, plus two valueless CI jobs. No production risk; the change restores real signal and gives developers local commands that match the pipeline.
**Categories:** INFRA, TESTING
**Batch:** B1
**Branch:** feat/m74-003-live-e2e-auth-portability
**Depends on:** None.
**Provenance:** Consolidates the original M74_003 (authored wrongly as "restore the `src/auth/` portability gate via an `error_registry` named module") and M74_004 (`make live-e2e-all` placeholder-filter cleanup) into one workstream. Corrected with Indy during implementation: `live-e2e-*` is the **live-API acceptance** namespace, not a Zig compile gate. The abandoned `error_registry` sweep is parked in a branch stash; do not resurrect.

**Canonical architecture:** `make/test-integration.mk` (`_test-integration-full` — the canonical infra-up + migrate + env-threaded `zig build test`) + the auth-acceptance jobs in `.github/workflows/{deploy-dev,post-release,smoke-post-deploy}.yml`.

---

## Implementing agent — read these first

1. `make/test-integration.mk:104-137` (`_test-integration-full`) — the canonical full-suite recipe `live-e2e-all` now reuses (depends on) rather than duplicates. `test-integration` already aliases it.
2. `.github/workflows/deploy-dev.yml` — `acceptance-e2e-dev` (`bun run test:e2e:acceptance`) + `cli-acceptance-dev` (`bun run test:acceptance`) are the CI jobs the `acceptance-e2e` / `cli-acceptance` make targets mirror 1:1.
3. `ui/packages/app/package.json` (`test:e2e:acceptance`) + `zombiectl/package.json` (`test:acceptance`) — the acceptance suites (owned by M65_001 / M65_002; not modified here).
4. `make/test.mk` — the include orchestrator; `dry.mk` is added beside `acceptance.mk`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Fix e2e/acceptance/dry make targets to mirror the pipeline (retire mislabeled live-e2e-auth, unfilter live-e2e-all, split dry into dry.mk, drop valueless dry-backend CI jobs)
- **Intent (one sentence):** make-target names and bodies match what the CI pipeline actually runs, the false-positive 0-test gate becomes a real full-suite run, and the dry lanes become honest UI-only page renders.
- **Handshake:** four coordinated moves — (1) `live-e2e-auth` → `acceptance-e2e` + `cli-acceptance`; (2) `live-e2e-all` → full unfiltered suite via `_test-integration-full`; (3) dry lanes → website+app only, moved to `make/dry.mk`; (4) drop the `dry-backend` / `dry-backend-smoke` CI jobs. No Zig source change.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it: dead `_e2e_backend`/filter chain removed, not shimmed); RULE NLG (no legacy framing pre-2.0).
- `docs/ZIG_RULES.md` — N/A; no `*.zig` touched (recipes *run* `zig build`, they don't edit Zig).

---

## Applicable Gates

> Blast radius: `make/acceptance.mk`, `make/dry.mk` (new), `make/test.mk`, `Makefile`, two `.github/workflows/dry*.yml`. No source files.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / Length / UFS / UI / LOGGING / SCHEMA / ERROR REGISTRY | no | Makefile/YAML only; none of these surfaces touched. |
| Milestone-ID Gate | watch | `make/` + `.github/` are outside `docs/` — no `M74_003`/`§` IDs in recipe bodies, comments, or workflow files. |
| check-gh-actions-valid (pre-commit) | yes | `.github/workflows/**` edited — `actionlint` must pass; verified clean on both `dry.yml` + `dry-smoke.yml`. **CI/CD edits authorized by Indy this session** (the two `dry-backend*` jobs "have no value"). |

---

## Overview

**Goal (testable):**
- `make acceptance-e2e` → `cd ui/packages/app && bun run test:e2e:acceptance`; `make cli-acceptance` → `cd zombiectl && bun run test:acceptance` (mirror CI `acceptance-e2e-*` / `cli-acceptance-*`).
- `make live-e2e-all` runs the FULL Zig integration suite unfiltered (no `-Dtest-filter`) vs real Postgres + Redis, via `_test-integration-full`.
- `make dry` / `make dry-smoke` run website + app Playwright only (no backend leg); both live in `make/dry.mk`.
- `make/acceptance.mk` contains only `live-e2e-all`, `acceptance-e2e`, `cli-acceptance`.
- The `dry-backend` + `dry-backend-smoke` CI jobs are removed.

**Problem:** Three defects compounded:
1. `make live-e2e-auth` ran `zig build test-auth` — a Zig compile-isolation gate (M18_002) wearing a `live-e2e-*` name. It never touched the API. No CI used it.
2. `make live-e2e-all` ran four `BACKEND_E2E_FILTER_*` placeholders that match zero `test "…"` declarations → `zig build test -Dtest-filter=<no-match>` runs 0 tests, exits 0: a silent false-positive gate.
3. `dry` / `dry-smoke` advertised "backend live-e2e + website + app" but their backend leg was the same broken filter chain; the `dry-backend` (full suite, redundant with `test-integration`) and `dry-backend-smoke` (the 0-test filter) CI jobs added cost and false signal.

**Solution summary:**
- Remove `live-e2e-auth`; add `acceptance-e2e` + `cli-acceptance` mirroring the CI jobs.
- Point `live-e2e-all` at `_test-integration-full` (reuse, not duplicate — `test-integration` already does).
- Delete the entire `_e2e` / `_e2e_backend` / `_e2e_smoke` / `_e2e_backend_smoke` / `_zig_test_filter` / `BACKEND_E2E_*` chain.
- Move `dry` / `dry-smoke` / `dry-app*` / `_dry_website*` into `make/dry.mk` as website+app-only lanes; include it from `make/test.mk`.
- Remove the `dry-backend` (`dry.yml`) and `dry-backend-smoke` (`dry-smoke.yml`) jobs and drop them from the aggregate `needs:` arrays.

---

## Prior-Art / Reference Implementations

- **In-repo** → `_test-integration-full` (`make/test-integration.mk`) is the canonical full-suite recipe; `live-e2e-all` reuses it via dependency.
- **In-repo** → the CI `acceptance-e2e-*` / `cli-acceptance-*` jobs are the canonical acceptance commands; the make targets are their local twins.
- **Divergence (for the better) from M74_004's literal text:** M74_004 said "rewrite `_e2e_backend` to mirror `_test-integration-full`'s body." Reusing via `live-e2e-all: _test-integration-full` (one recipe, not a 30-line duplicate) is DRYer and matches `feedback_make_targets`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/acceptance.mk` | EDIT | Trim to `live-e2e-all` (→ `_test-integration-full`) + `acceptance-e2e` + `cli-acceptance`. Delete `live-e2e-auth`, the filter/`_e2e*`/smoke chain, and the dry targets. |
| `make/dry.mk` | NEW | `dry` / `dry-smoke` (website + app only) + `dry-app` / `dry-app-smoke` / `_dry_website` / `_dry_website_smoke`, moved out of `acceptance.mk`. |
| `make/test.mk` | EDIT | `include make/dry.mk` beside `acceptance.mk`. |
| `Makefile` | EDIT | Help block: replace the `live-e2e-auth` line with `acceptance-e2e` + `cli-acceptance`; update `live-e2e-all`, `dry`, `dry-smoke` descriptions. |
| `.github/workflows/dry.yml` | EDIT (authorized) | Remove the `dry-backend` job (`make live-e2e-all`); drop it from the `dry` aggregate `needs:`. |
| `.github/workflows/dry-smoke.yml` | EDIT (authorized) | Remove the `dry-backend-smoke` job (`make _e2e_smoke`); drop it from the `dry-smoke` aggregate `needs:`. |

---

## Decomposition & alternatives

- **Chosen shape:** one consolidated workstream covering all the e2e/acceptance/dry make-target fixes + the dry.mk split + the CI job removals.
- **Alternatives considered:** (a) keep M74_003 (auth) and M74_004 (live-e2e-all) as separate specs/PRs — rejected; same file, same category, Indy chose one merged ticket. (b) keep dry targets in acceptance.mk — rejected; dry (UI) and acceptance/live-e2e (auth/backend) are distinct concerns, so dry.mk separates them.
- **Patch-vs-refactor verdict:** **patch + small reorg** — make-target rewires + a file split. No source or build-graph change.

---

## Sections (implementation slices)

### §1 — acceptance.mk: live-e2e-all + acceptance-e2e + cli-acceptance

Remove `live-e2e-auth` and the `_e2e*`/filter/smoke chain. `live-e2e-all: _test-integration-full`. Add `acceptance-e2e` + `cli-acceptance` mirroring the CI jobs.

### §2 — dry.mk: website + app dry lanes

Move `dry` / `dry-smoke` / `dry-app` / `dry-app-smoke` / `_dry_website` / `_dry_website_smoke` into `make/dry.mk`, website+app only. Include from `make/test.mk`.

### §3 — Remove the dry-backend CI jobs

Delete `dry-backend` (`dry.yml`) + `dry-backend-smoke` (`dry-smoke.yml`); fix the aggregate `needs:` arrays. `actionlint` must stay clean.

### §4 — Makefile help

Update the help block: drop `live-e2e-auth`; add `acceptance-e2e` + `cli-acceptance`; re-describe `live-e2e-all` (full unfiltered suite) and `dry`/`dry-smoke` (website+app only).

---

## Interfaces

```
make live-e2e-all   → _test-integration-full            # full Zig integration suite, unfiltered, real PG+Redis
make acceptance-e2e → cd ui/packages/app && bun run test:e2e:acceptance   # mirrors CI acceptance-e2e-{dev,prod}
make cli-acceptance → cd zombiectl       && bun run test:acceptance       # mirrors CI cli-acceptance-{dev,prod}
make dry            → _dry_website dry-app               # website + app Playwright (no Clerk auth)
make dry-smoke      → _dry_website_smoke dry-app-smoke   # fast website + app (no Clerk auth)
```

No HTTP/REST/OpenAPI/CLI/schema surface. The acceptance suites read their standard env (`BASE_URL`/`NEXT_PUBLIC_API_URL`/`CLERK_*`; `ZOMBIE_ACCEPTANCE_TARGET`) exactly as the CI jobs supply them.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `live-e2e-all` exits 0 with 0 tests | the defect being fixed (placeholder filters) | Reuses `_test-integration-full` (no `-Dtest-filter`); a real failure now fails the run. |
| `live-e2e-all` can't reach PG/Redis | infra not up | `_test-integration-full` depends on `_reset-test-db` → `_ensure-test-infra` (docker compose up), so it brings up infra or fails loud. |
| `acceptance-e2e` fails to start | `CLERK_*` / base URL unset, Playwright not installed | Operator/CI supplies env + browser (op:// Clerk creds), as `acceptance-e2e-dev` does. |
| `cli-acceptance` live legs skip | `ZOMBIE_ACCEPTANCE_TARGET` unset / non-https | Expected — suite self-skips (52 pass / 2 skip locally). |
| A `dry*` CI job breaks on job removal | stale `needs:` referencing a removed job | Both aggregate `needs:` arrays updated; `actionlint` verified clean. |

---

## Invariants

1. **`make/acceptance.mk` contains only `live-e2e-all`, `acceptance-e2e`, `cli-acceptance`** — `grep -nE '_e2e_backend|_e2e_smoke|_zig_test_filter|BACKEND_E2E|live-e2e-auth|^dry' make/acceptance.mk` returns empty.
2. **`make live-e2e-all` runs the full suite unfiltered** — `make -n live-e2e-all` shows `_test-integration-full`'s `docker compose` + `zig build test` (no `-Dtest-filter`).
3. **`dry` / `dry-smoke` are website+app only** — `make -n dry` shows no backend/`zig` step.
4. **No workflow references `dry-backend`, `make live-e2e-all`, or `make _e2e_smoke`** — and `actionlint` passes.

---

## Acceptance Criteria

- `make -n acceptance-e2e` → `cd ui/packages/app && bun run test:e2e:acceptance`.
- `make -n cli-acceptance` → `cd zombiectl && bun run test:acceptance`.
- `make -n live-e2e-all` shows `_test-integration-full` (docker compose + unfiltered `zig build test`); `grep -n 'test-auth\|-Dtest-filter' make/acceptance.mk` returns nothing.
- `make -n dry` / `make -n dry-smoke` show only website + app commands, no `zig`.
- `grep -rn 'dry-backend\|make live-e2e-all\|make _e2e_smoke' .github/workflows/` returns nothing; `actionlint` clean.
- `cd zombiectl && bun run test:acceptance` runs green locally (live legs self-skip): pass / skip / 0 fail.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| `make -n` for each target | dispatches to the matching pipeline command / recipe | local / CI |
| `bun run test:acceptance` (zombiectl) | CLI auth lifecycle suite executes; live legs self-skip | local |
| `actionlint .github/workflows/dry*.yml` | edited workflows valid after job removal | pre-commit / local |

The acceptance/integration assertions themselves are owned by M65_001, M65_002, and the existing integration suite; this spec only wires the entrypoints.

---

## Discovery

- `live-e2e-all` now resolves to the same recipe as `make test-integration` (both `_test-integration-full`). Intentional — `live-e2e-all` is the acceptance-lane name, `test-integration` the test-tier name — but it is a duplication of purpose worth revisiting if a distinct "acceptance backend" subset is ever defined.
- The Zig `test-auth` build step in `build.zig` is now fully orphaned (nothing invokes it) and red on `main` (M62 `error_registry` + M74_002 `auth → src/queue/` escapes, the latter on a non-portable `queue → src/zombie/event_envelope` chain). Whether to keep/rename/remove the `src/auth/` portability concept is a separate decision — likely its own spec, since the auth→queue coupling means `src/auth/` is no longer cleanly extractable.

---

## Out of Scope

- Fixing/removing the orphaned Zig `test-auth` portability gate and its M62/M74_002 boundary escapes.
- Making `src/auth/` extractable into a standalone `zombie-auth` binary.
- The abandoned `error_registry` named-module sweep (parked in a branch stash).
- Any change to the M65_001 / M65_002 acceptance suites or the integration test bodies.
