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
**Status:** IN_PROGRESS
**Priority:** P2 — false-positive gate; currently runs zero tests and exits 0 silently. Real signal restoration.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m74-004-live-e2e-all-unfiltered
**Depends on:** None.
**Provenance:** Surfaced during M74_001 Piece 1 closeout. Captain decision (May 19, 2026): "you must run with no filers in the all one" — `live-e2e-all` runs the full integration suite unfiltered. Scope amended at CHORE(open) (May 21, 2026): §3's drop-smoke decision collided with a live `dry-smoke.yml` CI caller of `make _e2e_smoke`; Captain steer "all of these worked before i need them working again" → keep + fix the smoke lane rather than drop it (see Discovery).

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

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: `make/acceptance.mk` + `make/test-integration.mk` + the top-level `Makefile` help block. No source files.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` edited — the recipe *runs* `zig build test`, it doesn't touch Zig source. |
| PUB / Struct-Shape | no | no Zig surface. |
| File & Function Length (≤350/≤50/≤70) | no | `.mk`/`Makefile` are outside the length-gate surface; the change net-removes lines. |
| UFS (repeated/semantic literals) | no | the UFS surface is `*.zig`/`*.ts`/`*.tsx`/`*.js`/`*.jsx`; Makefiles are out of scope. |
| UI Substitution / DESIGN TOKEN | no | no `*.tsx`/`*.jsx`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | none of these surfaces touched. |

Note: editing `Makefile`/`make/*.mk` triggers the pre-commit `check-gh-actions-valid` lane (actionlint + make-target-ref sweep). Both `.github/workflows/dry.yml` (`make live-e2e-all`) and `.github/workflows/dry-smoke.yml` (`make _e2e_smoke`) reference make targets — `_zig_test_filter` is the only target deleted, and nothing references it from a workflow, so every workflow `make <target>` ref stays valid. The workflows are **not** edited.

---

## Overview

**Goal (testable):** `make live-e2e-all` runs the full Zig integration test suite (no `-Dtest-filter`) against a real Postgres + Redis brought up via `docker compose`, with `LIVE_DB=1` + Redis TLS env threaded through. Exit code reflects whether any integration test failed. The `BACKEND_E2E_FILTER_*` placeholder constants are removed; `_zig_test_filter` is removed or repurposed.

**Problem:** `live-e2e-all` is a false-positive gate today. Three independent defects:

1. **`BACKEND_E2E_FILTER_1..4` reference tests that never existed in this repo.** All four filter strings match zero `test "…"` declarations in `src/`. The forward-looking placeholders were planted when `make/acceptance.mk` was first created (commit `21830dd1`, May 16, 2026) and no real test was ever written to match them. `zig build test -Dtest-filter=<no-match>` builds the test binary and runs 0 tests — exit 0, no signal.
2. **`_zig_test_filter` does not depend on infra.** Missing `_ensure-test-infra` + `_reset-test-db` deps. Even if filters matched real tests, the suite would fail to connect to Postgres/Redis because the containers aren't guaranteed up.
3. **`_zig_test_filter` does not thread infra env.** Missing `TEST_DATABASE_URL` / `LIVE_DB=1` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE`. The threaded `TEST_REDIS_TLS_URL` alone is insufficient — DB-backed tests skip silently if `LIVE_DB` isn't set.

**Solution summary:** Delete `BACKEND_E2E_FILTER_1..4` + `_zig_test_filter`. Parametrize the canonical `_test-integration-full` with an *optional* `TEST_FILTER` (unset = full suite = today's behavior; set = `-Dtest-filter=<value>`) so both the full lane and the smoke lane share one correct infra+env recipe. Rewrite `_e2e_backend` to depend on `_test-integration-full` with no filter — full suite, no `-Dtest-filter`. `live-e2e-all` runs `_e2e_backend` (full). Fix `BACKEND_E2E_SMOKE_FILTER` to a string that matches **real** tests (`integration: ready decision`, 4 readiness tests touching DB + Redis health) and route `_e2e_backend_smoke` through `$(MAKE) _test-integration-full` with that filter — so the smoke lane finally runs against real infra with the env threaded (fixing defects #2 + #3 for smoke too). `_e2e_smoke` / `dry-smoke`'s backend leg is **kept and fixed**, not dropped (Captain steer May 21, 2026) — the live `dry-smoke.yml` CI job keeps working.

---

## Prior-Art / Reference Implementations

> Mirror the canonical integration recipe — don't invent a new infra-up shape.

- **In-repo** → `make/test-integration.mk:65-138` — `_test-integration-full` is the canonical pattern: depends on `_reset-test-db` (→ `_ensure-test-infra`), threads `LIVE_DB=1` / `TEST_DATABASE_URL` / `TEST_REDIS_TLS_URL` / `REDIS_URL_API` / `REDIS_TLS_CA_CERT_FILE` into `zig build test`.
- **Alignment:** no divergence — `_e2e_backend` is rewritten to a `_test-integration-full`-shaped recipe minus the `-Dtest-filter`. Not greenfield; the shape already exists at `make/test-integration.mk:65-138`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/acceptance.mk` | EDIT | Delete `BACKEND_E2E_FILTER_1..4` constants + the `_zig_test_filter` primitive. Rewrite `_e2e_backend` to depend on `_test-integration-full` (full suite, no filter). Fix `BACKEND_E2E_SMOKE_FILTER` to match real tests and route `_e2e_backend_smoke` through `$(MAKE) _test-integration-full` with that filter. `_e2e_smoke` / `dry-smoke` keep their backend leg (now real). |
| `make/test-integration.mk` | EDIT | Parametrize `_test-integration-full` with an optional `TEST_FILTER` (unset = full suite, backward-compatible; set = `-Dtest-filter=<value>`) so the full lane and the smoke lane share one infra+env recipe instead of duplicating ~30 lines. |
| `Makefile` (top-level `help` block) | EDIT | Update the `live-e2e-all` help text to reflect "full integration suite unfiltered" framing. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** parametrize the canonical `_test-integration-full` with an optional `TEST_FILTER`, point the full lane (`_e2e_backend` → `live-e2e-all`) at it with no filter, and point the smoke lane (`_e2e_backend_smoke`) at it with a real fast filter. Delete the broken `BACKEND_E2E_FILTER_1..4` + `_zig_test_filter`. A targeted Makefile cleanup that shares one recipe instead of three.
- **Alternatives considered:** (a) curate a real `live-e2e` subset distinct from `test-integration` for the **full** lane — rejected per Indy's "no filters in the all one". (b) drop the backend smoke leg entirely (original §3) — rejected: `dry-smoke.yml` calls `make _e2e_smoke`, so dropping it would break a live CI gate (Indy: "all of these worked before i need them working again"). (c) alias smoke → full suite — rejected: both `dry.yml` and `dry-smoke.yml` fire on every PR, so it would run the full 169-test suite twice per push.
- **Patch-vs-refactor verdict:** this is a **patch** — removing dead filter constants and routing two lanes through one parametrized canonical recipe. No new abstraction.

---

## Sections (implementation slices)

### §1 — Delete the broken filter machinery + parametrize the canonical recipe — DONE

Remove `BACKEND_E2E_FILTER_1..4` and the `_zig_test_filter` primitive. Parametrize `_test-integration-full` (in `make/test-integration.mk`) with an optional `TEST_FILTER`: unset → `zig build test` (full suite, today's behavior, backward-compatible); set → `zig build -Dtest-filter="$$TEST_FILTER" test`. Use POSIX `set --`/`"$@"` so a filter value with spaces stays one argument. Grepped the repo: `_zig_test_filter` has no caller outside `acceptance.mk`; no workflow references it.

### §2 — Rewrite `_e2e_backend` (full lane) — DONE

`_e2e_backend: _test-integration-full` with no `TEST_FILTER` → full suite, no `-Dtest-filter`, against real PG + Redis via the canonical dependency chain (`_reset-test-db` → `_ensure-test-infra`) and env block. `live-e2e-all` runs `_e2e_backend` (via the shared `_e2e` aggregate that `dry` also uses).

### §3 — Keep + fix the smoke lane (Captain steer, May 21, 2026) — DONE

Original PLAN-decision (drop smoke entirely) collided with the live `dry-smoke.yml` CI job that runs `make _e2e_smoke`. Captain steer: "all of these worked before i need them working again." So `_e2e_backend_smoke` is **kept** and fixed: `BACKEND_E2E_SMOKE_FILTER` becomes `integration: ready decision` (a substring matching the 4 readiness tests that exercise DB-unhealthy + Redis-degraded + healthy paths), and `_e2e_backend_smoke` runs `$(MAKE) _test-integration-full` with that filter — real infra, env threaded, fast. `dry-smoke`'s backend leg now runs real tests instead of zero. No workflow edit needed.

---

## Interfaces

Make-target interface — both lanes share the parametrized canonical recipe:

```
_test-integration-full              # parametrized: optional TEST_FILTER env
    deps: _reset-test-db (→ _ensure-test-infra)
    env:  LIVE_DB=1, TEST_DATABASE_URL, TEST_REDIS_TLS_URL, REDIS_URL_API, REDIS_TLS_CA_CERT_FILE
    cmd:  zig build [-Dtest-filter="$TEST_FILTER"] test   # filter applied iff TEST_FILTER set

make live-e2e-all → _e2e → _e2e_backend: _test-integration-full   # TEST_FILTER unset → FULL suite
make _e2e_smoke   → _e2e_backend_smoke: $(MAKE) _test-integration-full
                                          TEST_FILTER="integration: ready decision"  # fast real subset
make test-integration → _test-integration-full                    # unchanged: TEST_FILTER unset → FULL

removed: BACKEND_E2E_FILTER_1..4, _zig_test_filter
kept + fixed: BACKEND_E2E_SMOKE_FILTER (now matches real tests), _e2e_backend_smoke, _e2e_smoke
```

No source, HTTP, CLI, or schema interface changes. The contract is the Make targets' behavior: `live-e2e-all`'s exit code reflects the full suite's pass/fail; `_e2e_smoke`'s reflects a real fast subset's.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `live-e2e-all` exits 0 with 0 tests run | The defect being fixed — filters matched nothing | Post-rewrite, assert the run count matches `grep -rn 'test "integration:' src/ \| wc -l`; a 0-count run is a failure, not a pass. |
| Suite can't reach Postgres/Redis | `_e2e_backend` not depending on `_reset-test-db`/`_ensure-test-infra` | Mirror `_test-integration-full`'s dependency chain so containers are guaranteed up before `zig build test`. |
| DB-backed tests skip silently | `LIVE_DB` not threaded into the env | Thread the full env block (`LIVE_DB=1` + `TEST_DATABASE_URL` + Redis TLS vars), not just `TEST_REDIS_TLS_URL`. |
| `dry-smoke.yml`'s `make _e2e_smoke` breaks | Original §3 would delete `_e2e_smoke` while a live CI job calls it | Resolved: keep `_e2e_smoke`/`_e2e_backend_smoke`, fix them to run a real subset. `check-gh-actions-valid` make-target-ref sweep stays green; no workflow edit. |
| Smoke filter matches zero tests (regress to original bug) | `BACKEND_E2E_SMOKE_FILTER` typo'd or test renamed | `integration: ready decision` matched 4 real tests at authoring (`grep -c`); VERIFY asserts the smoke run reports >0 tests. |

---

## Invariants

1. **`live-e2e-all` runs the full integration suite, no `-Dtest-filter`** — `_e2e_backend` depends on `_test-integration-full` with `TEST_FILTER` unset; enforced by the run-count assertion in Acceptance Criteria.
2. **The broken filter machinery is gone** — `grep -nE 'BACKEND_E2E_FILTER_[0-9]|_zig_test_filter' make/` returns empty. (`BACKEND_E2E_SMOKE_FILTER` is retained and now matches real tests — it is not a `BACKEND_E2E_FILTER_N` placeholder.)
3. **Both lanes run against real infra** — `_test-integration-full` depends on `_reset-test-db` (→ `_ensure-test-infra`); both full and smoke lanes fail loud if Postgres/Redis are unreachable rather than skipping silently.
4. **The smoke lane runs real tests** — `BACKEND_E2E_SMOKE_FILTER` matches ≥1 `test "integration:` declaration; a zero-test smoke run is a failure, not a pass.

---

## Acceptance Criteria

- `make live-e2e-all` exits 0 against a healthy local Docker compose stack (Postgres + Redis healthy).
- `make live-e2e-all` runs every shipped integration test (`grep -rn 'test "integration:' src/ | wc -l` matches the test count reported by `zig build test`).
- `make _e2e_smoke` exits 0 and runs >0 tests (the readiness subset) against the same real stack.
- `grep -nE 'BACKEND_E2E_FILTER_[0-9]|_zig_test_filter' make/` returns zero matches.
- `make check-gh-actions-valid` passes — every `make <target>` ref in `.github/workflows/` still resolves.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| `make live-e2e-all` green | Full integration suite passes against real PG + Redis | CI (`docker compose` available) + local dev |
| `make _e2e_smoke` green | Readiness subset passes against real PG + Redis; >0 tests run | CI (`dry-smoke.yml`) + local dev |
| filter-constant cleanup | `BACKEND_E2E_FILTER_[0-9]` + `_zig_test_filter` deleted from `make/` | Grep gate |
| workflow refs valid | every `make <target>` in `.github/workflows/` resolves | `make check-gh-actions-valid` |

---

## Discovery

- **§3 scope amendment (May 21, 2026).** Pre-edit grep found a live caller the original spec assumed didn't exist: `.github/workflows/dry-smoke.yml:22` runs `make _e2e_smoke`. Dropping `_e2e_smoke` (original §3) would have broken that CI job and failed the `check-gh-actions-valid` make-target-ref sweep. Surfaced to Captain with the alias-vs-drop tradeoff.
  > Indy (2026-05-21): "Well all of these worked before i need them working again" — context: the smoke + dry-smoke + live-e2e-all lanes and both CI workflows must stay green. Resolution: keep + fix the backend smoke lane (real subset against real infra), do not drop it and do not edit the workflows.
- **Blast radius** now `make/acceptance.mk` + `make/test-integration.mk` (parametrize `_test-integration-full`) + the top-level `Makefile` help block. No workflow files, no source.

---

## Out of Scope

- Curating a "live-e2e" subset distinct from `test-integration`. Captain's call: run the full suite, no filters.
- Splitting the integration suite into faster/slower tiers — orthogonal to this rewrite.
- Bringing the dashboard Playwright suite or website Playwright suite into `live-e2e-all`'s blast radius. `dry` already covers those; `live-e2e-all` is backend-only.
