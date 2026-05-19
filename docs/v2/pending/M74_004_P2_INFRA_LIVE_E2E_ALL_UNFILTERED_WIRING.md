<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_004: `make live-e2e-all` — drop placeholder filters, run full integration suite unfiltered

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 004
**Date:** May 19, 2026
**Status:** PENDING
**Priority:** P2 — false-positive gate; currently runs zero tests and exits 0 silently. Real signal restoration.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m74-004-live-e2e-all-unfiltered (to be created on CHORE(open))
**Depends on:** None.
**Provenance:** Surfaced during M74_001 Piece 1 closeout. Captain decision (May 19, 2026): "you must run with no filers in the all one" — `live-e2e-all` runs the full integration suite unfiltered.

**Canonical architecture:** `make/test-integration.mk:105` (`_test-integration-full` — the canonical infra-up + migrate + env-threaded `zig build test` recipe to mirror).

---

## Implementing agent — read these first

1. `make/acceptance.mk:5-47` — current `live-e2e-all` target body, `BACKEND_E2E_FILTER_*` constants, and the `_zig_test_filter` / `_e2e_backend` primitives.
2. `make/test-integration.mk:65-138` — `_test-integration-full` is the canonical pattern: depends on `_reset-test-db` (which depends on `_ensure-test-infra`), threads `LIVE_DB=1` / `TEST_DATABASE_URL` / `TEST_REDIS_TLS_URL` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE` through to `zig build test`.
3. `docker-compose.yml` — the `postgres` + `redis` services `_ensure-test-infra` brings up.
4. Captain quote (May 19, 2026): *"you must run with no filers in the all one."* — `live-e2e-all` runs the full integration suite, no `-Dtest-filter`.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it) applies if the diff lands near related dead Makefile recipes; RULE NLG forbids legacy framing.
- `docs/gates/doc-read.md` — applies to any `*.mk` change touching test/integration infra.

---

## Overview

**Goal (testable):** `make live-e2e-all` runs the full Zig integration test suite (no `-Dtest-filter`) against a real Postgres + Redis brought up via `docker compose`, with `LIVE_DB=1` + Redis TLS env threaded through. Exit code reflects whether any integration test failed. The `BACKEND_E2E_FILTER_*` placeholder constants are removed; `_zig_test_filter` is removed or repurposed.

**Problem:** `live-e2e-all` is a false-positive gate today. Three independent defects:

1. **`BACKEND_E2E_FILTER_1..4` reference tests that never existed in this repo.** All four filter strings match zero `test "…"` declarations in `src/`. The forward-looking placeholders were planted when `make/acceptance.mk` was first created (commit `21830dd1`, May 16, 2026) and no real test was ever written to match them. `zig build test -Dtest-filter=<no-match>` builds the test binary and runs 0 tests — exit 0, no signal.
2. **`_zig_test_filter` does not depend on infra.** Missing `_ensure-test-infra` + `_reset-test-db` deps. Even if filters matched real tests, the suite would fail to connect to Postgres/Redis because the containers aren't guaranteed up.
3. **`_zig_test_filter` does not thread infra env.** Missing `TEST_DATABASE_URL` / `LIVE_DB=1` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE`. The threaded `TEST_REDIS_TLS_URL` alone is insufficient — DB-backed tests skip silently if `LIVE_DB` isn't set.

**Solution summary:** Delete `BACKEND_E2E_FILTER_1..4` + `BACKEND_E2E_SMOKE_FILTER`. Rewrite `_e2e_backend` to depend on `_reset-test-db` (transitively pulls in `_ensure-test-infra` and DB migration) and invoke `zig build test` directly with the full `_test-integration-full` env block — no `-Dtest-filter`. `live-e2e-all` aliases to the rewritten `_e2e_backend`. `_zig_test_filter` and `_e2e_backend_smoke` get deleted (no curated subset, no fast-smoke variant — the full suite is the contract). `dry-smoke` either drops the backend leg or aliases to `_e2e_backend` (decision in PLAN).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/acceptance.mk` | EDIT | Delete `BACKEND_E2E_FILTER_*` constants, `_zig_test_filter` primitive, `_e2e_backend_smoke`, `_e2e_smoke` aggregate. Rewrite `_e2e_backend` as a `_test-integration-full`-shape recipe. |
| `Makefile` (top-level `help` block) | EDIT | Update the `live-e2e-all` help text to reflect "full integration suite unfiltered" framing. |

---

## Sections (implementation slices)

### §1 — Delete placeholder filters

Remove `BACKEND_E2E_FILTER_1..4`, `BACKEND_E2E_SMOKE_FILTER`, `_zig_test_filter`. Grep the rest of the repo for callers; none expected.

### §2 — Rewrite `_e2e_backend`

Mirror `_test-integration-full`'s body: depend on `_reset-test-db`, build the same env block (resolve `TEST_DATABASE_URL` + `TEST_REDIS_TLS_URL` with the same defaults + sslmode/TLS-cert fallbacks), invoke `zig build test` (no `-Dtest-filter`). `live-e2e-all` becomes a 1-line alias.

### §3 — Drop or alias smoke

Captain didn't specify smoke. PLAN-decision: drop `_e2e_smoke` / `dry-smoke`'s backend leg entirely (smoke is for fast UI checks; the integration suite isn't a smoke-grade thing).

---

## Acceptance Criteria

- `make live-e2e-all` exits 0 against a healthy local Docker compose stack (Postgres + Redis healthy).
- `make live-e2e-all` runs every shipped integration test (`grep -rn 'test "integration:' src/ | wc -l` matches the test count reported by `zig build test`).
- `grep -nE 'BACKEND_E2E_FILTER|_zig_test_filter' make/` returns zero matches.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| `make live-e2e-all` green | Full integration suite passes against real PG + Redis | CI (`docker compose` available) + local dev |
| filter-constant cleanup | `BACKEND_E2E_FILTER_*` + `_zig_test_filter` deleted from `make/` | Grep gate |

---

## Discovery

(none — single-PR Makefile cleanup, blast radius limited to `make/acceptance.mk` + the top-level help block)

---

## Out of Scope

- Curating a "live-e2e" subset distinct from `test-integration`. Captain's call: run the full suite, no filters.
- Splitting the integration suite into faster/slower tiers — orthogonal to this rewrite.
- Bringing the dashboard Playwright suite or website Playwright suite into `live-e2e-all`'s blast radius. `dry` already covers those; `live-e2e-all` is backend-only.
