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

## PR Intent & comprehension handshake

> The bridge from spec to merged PR — the agent confirms intent before writing code.

- **PR title (eventual):** make live-e2e-all: drop placeholder filters, run full suite
- **Intent (one sentence):** `make live-e2e-all` runs the full Zig integration suite unfiltered against real Postgres + Redis, so its exit code is real signal instead of a silent 0-tests pass.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list the assumptions you proceed on (`ASSUMPTIONS I'M MAKING: …`). The PLAN-decision to name: §3 drops the backend leg of `dry-smoke`/`_e2e_smoke` — confirm no live caller depends on it before deleting. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — RULE NLR (touch-it-fix-it) applies if the diff lands near related dead Makefile recipes; RULE NLG forbids legacy framing.
- `docs/gates/doc-read.md` — applies to any `*.mk` change touching test/integration infra.

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: `make/acceptance.mk` + the top-level `Makefile` help block. No source files.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` edited — the recipe *runs* `zig build test`, it doesn't touch Zig source. |
| PUB / Struct-Shape | no | no Zig surface. |
| File & Function Length (≤350/≤50/≤70) | no | `.mk`/`Makefile` are outside the length-gate surface; the change net-removes lines. |
| UFS (repeated/semantic literals) | no | the UFS surface is `*.zig`/`*.ts`/`*.tsx`/`*.js`/`*.jsx`; Makefiles are out of scope. |
| UI Substitution / DESIGN TOKEN | no | no `*.tsx`/`*.jsx`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | none of these surfaces touched. |

Note: editing `Makefile`/`make/*.mk` triggers the pre-commit `check-gh-actions-valid` lane (actionlint + make-target-ref sweep) — keep target references valid after deleting `_zig_test_filter`/`_e2e_backend_smoke`.

---

## Overview

**Goal (testable):** `make live-e2e-all` runs the full Zig integration test suite (no `-Dtest-filter`) against a real Postgres + Redis brought up via `docker compose`, with `LIVE_DB=1` + Redis TLS env threaded through. Exit code reflects whether any integration test failed. The `BACKEND_E2E_FILTER_*` placeholder constants are removed; `_zig_test_filter` is removed or repurposed.

**Problem:** `live-e2e-all` is a false-positive gate today. Three independent defects:

1. **`BACKEND_E2E_FILTER_1..4` reference tests that never existed in this repo.** All four filter strings match zero `test "…"` declarations in `src/`. The forward-looking placeholders were planted when `make/acceptance.mk` was first created (commit `21830dd1`, May 16, 2026) and no real test was ever written to match them. `zig build test -Dtest-filter=<no-match>` builds the test binary and runs 0 tests — exit 0, no signal.
2. **`_zig_test_filter` does not depend on infra.** Missing `_ensure-test-infra` + `_reset-test-db` deps. Even if filters matched real tests, the suite would fail to connect to Postgres/Redis because the containers aren't guaranteed up.
3. **`_zig_test_filter` does not thread infra env.** Missing `TEST_DATABASE_URL` / `LIVE_DB=1` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE`. The threaded `TEST_REDIS_TLS_URL` alone is insufficient — DB-backed tests skip silently if `LIVE_DB` isn't set.

**Solution summary:** Delete `BACKEND_E2E_FILTER_1..4` + `BACKEND_E2E_SMOKE_FILTER`. Rewrite `_e2e_backend` to depend on `_reset-test-db` (transitively pulls in `_ensure-test-infra` and DB migration) and invoke `zig build test` directly with the full `_test-integration-full` env block — no `-Dtest-filter`. `live-e2e-all` aliases to the rewritten `_e2e_backend`. `_zig_test_filter` and `_e2e_backend_smoke` get deleted (no curated subset, no fast-smoke variant — the full suite is the contract). `dry-smoke` either drops the backend leg or aliases to `_e2e_backend` (decision in PLAN).

---

## Prior-Art / Reference Implementations

> Mirror the canonical integration recipe — don't invent a new infra-up shape.

- **In-repo** → `make/test-integration.mk:65-138` — `_test-integration-full` is the canonical pattern: depends on `_reset-test-db` (→ `_ensure-test-infra`), threads `LIVE_DB=1` / `TEST_DATABASE_URL` / `TEST_REDIS_TLS_URL` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE` into `zig build test`.
- **Alignment:** no divergence — `_e2e_backend` is rewritten to a `_test-integration-full`-shaped recipe minus the `-Dtest-filter`. Not greenfield; the shape already exists at `make/test-integration.mk:65-138`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/acceptance.mk` | EDIT | Delete `BACKEND_E2E_FILTER_*` constants, `_zig_test_filter` primitive, `_e2e_backend_smoke`, `_e2e_smoke` aggregate. Rewrite `_e2e_backend` as a `_test-integration-full`-shape recipe. |
| `Makefile` (top-level `help` block) | EDIT | Update the `live-e2e-all` help text to reflect "full integration suite unfiltered" framing. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — delete the placeholder filter constants, rewrite `_e2e_backend` to the canonical full-suite shape, drop/alias smoke. A targeted Makefile cleanup.
- **Alternatives considered:** (a) curate a real `live-e2e` subset distinct from `test-integration` — rejected per Indy's call ("run with no filters in the all one"). (b) keep a fast smoke variant of the backend suite — rejected; the integration suite isn't smoke-grade.
- **Patch-vs-refactor verdict:** this is a **patch** — removing dead filter constants and pointing one recipe at the existing canonical full-suite shape. No new abstraction.

---

## Sections (implementation slices)

### §1 — Delete placeholder filters

Remove `BACKEND_E2E_FILTER_1..4`, `BACKEND_E2E_SMOKE_FILTER`, `_zig_test_filter`. Grep the rest of the repo for callers; none expected.

### §2 — Rewrite `_e2e_backend`

Mirror `_test-integration-full`'s body: depend on `_reset-test-db`, build the same env block (resolve `TEST_DATABASE_URL` + `TEST_REDIS_TLS_URL` with the same defaults + sslmode/TLS-cert fallbacks), invoke `zig build test` (no `-Dtest-filter`). `live-e2e-all` becomes a 1-line alias.

### §3 — Drop or alias smoke

Captain didn't specify smoke. PLAN-decision: drop `_e2e_smoke` / `dry-smoke`'s backend leg entirely (smoke is for fast UI checks; the integration suite isn't a smoke-grade thing).

---

## Interfaces

Make-target interface — `live-e2e-all` becomes a thin alias over a `_test-integration-full`-shaped recipe:

```
make live-e2e-all
  → _e2e_backend                    # rewritten: depends on _reset-test-db (→ _ensure-test-infra)
      env: LIVE_DB=1, TEST_DATABASE_URL, TEST_REDIS_TLS_URL, REDIS_URL_API, REDIS_TLS_CA_CERT_FILE
      cmd: zig build test           # NO -Dtest-filter — full integration suite

removed: BACKEND_E2E_FILTER_1..4, BACKEND_E2E_SMOKE_FILTER, _zig_test_filter, _e2e_backend_smoke
```

No source, HTTP, CLI, or schema interface changes. The contract is the Make target's behavior: `live-e2e-all`'s exit code now reflects the full suite's pass/fail.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `live-e2e-all` exits 0 with 0 tests run | The defect being fixed — filters matched nothing | Post-rewrite, assert the run count matches `grep -rn 'test "integration:' src/ \| wc -l`; a 0-count run is a failure, not a pass. |
| Suite can't reach Postgres/Redis | `_e2e_backend` not depending on `_reset-test-db`/`_ensure-test-infra` | Mirror `_test-integration-full`'s dependency chain so containers are guaranteed up before `zig build test`. |
| DB-backed tests skip silently | `LIVE_DB` not threaded into the env | Thread the full env block (`LIVE_DB=1` + `TEST_DATABASE_URL` + Redis TLS vars), not just `TEST_REDIS_TLS_URL`. |
| A `dry-smoke` caller breaks | §3 drops the backend leg of smoke | Grep for callers of `_e2e_backend_smoke`/`_e2e_smoke` before deleting; update or remove each. |

---

## Invariants

1. **`live-e2e-all` runs the full integration suite, no `-Dtest-filter`** — enforced by the run-count assertion in Acceptance Criteria.
2. **The placeholder filter constants are gone** — `grep -nE 'BACKEND_E2E_FILTER|_zig_test_filter' make/` returns empty.
3. **The suite runs against real infra** — `_reset-test-db` (→ `_ensure-test-infra`) is a hard dependency; the run fails loud if Postgres/Redis are unreachable rather than skipping silently.

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
