# M6_006: Validate v1 Acceptance E2E Gate

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 006
**Date:** Mar 05, 2026
**Updated:** Mar 20, 2026: 10:00 AM
**Status:** DONE
**Priority:** P0 — release gate
**Batch:** B7 — deferred after M6_003
**Depends on:** M4_001 (Implement `zombiectl` CLI Runtime), M3_006 (Implement Clerk Authentication Contract)
**Note:** M3_004 (Redis streams) and M3_005 (security hardening) already DONE
**Successor:** M7_001_DEV_ACCEPTANCE.md (DEV CLI acceptance gate), M7_003_PROD_ACCEPTANCE.md (PROD release gate)

---

## 1.0 Acceptance Target

**Status:** DONE — gate contract defined; CLI acceptance execution moved to M7_001_DEV_ACCEPTANCE.md

The canonical v1 acceptance target is a CLI-driven end-to-end run: authenticate, connect a repo, sync specs, trigger a run, and verify outcomes via PR evidence.

**Dimensions:**
- 1.1 ✅ DONE Gate contract defined: `zombiectl login` → `workspace add` → `specs sync` → `run` → `runs list`
- 1.2 ✅ DONE All server-side endpoints required by the CLI acceptance flow are implemented and tested
- 1.3 ✅ DONE CLI hardened and acceptance-ready (M6_006 §7.0)
- 1.4 ✅ DONE Integration DB gate wired in CI (M6_006 §6.0)
- 1.5 DEFERRED Live execution evidence → M7_001_DEV_ACCEPTANCE.md §4.0

---

## 2.0 Gate Contract

**Status:** DONE — defined and implemented; execution evidence in M7_001_DEV_ACCEPTANCE.md

### 2.1 Functional Gate
- 2.1.1 ✅ DONE Every queued spec reaches terminal state with deterministic reason codes
- 2.1.2 ✅ DONE Every successful run opens a valid PR URL tied to the run record
- 2.1.3 ✅ DONE Failed runs emit actionable failure reason and operator-visible logs

### 2.2 Performance Gate
- 2.2.1 ✅ DONE Spec-to-PR latency tracked per run via run record timestamps
- 2.2.2 DEFERRED Under-5-minute gate measured in M7_001_DEV_ACCEPTANCE.md §3.0

### 2.3 Security and Reliability Gate
- 2.3.1 ✅ DONE Role-separated DB and Redis ACL paths verified in unit + integration tests
- 2.3.2 ✅ DONE Retry/backoff/idempotency behaviors covered in integration test suite

---

## 3.0 Verification Commands

**Status:** DONE — commands defined; live execution in M7_001_DEV_ACCEPTANCE.md §4.0

```bash
npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Gate contract defined and all server endpoints implemented
- [x] 4.2 CLI hardened: auth guard, payload guard, operator commands hidden, doctor output, full help
- [x] 4.3 Integration DB gate wired in CI and passing (246 Zig unit tests, 217 integration tests)
- [x] 4.4 Live end-to-end execution tracked in M7_001_DEV_ACCEPTANCE.md

---

## 5.0 Out of Scope

- UI-driven acceptance workflows
- Non-CLI onboarding variants
- Production SRE runbook expansion beyond v1 gate evidence

---

## 6.0 Integration DB Gate Environment (`HANDLER_DB_TEST_URL`)

**Status:** DONE (Mar 17, 2026) — shipped in PR feat/m6-006-db-gate

`HANDLER_DB_TEST_URL` is the canonical env var. CI starts Postgres via `docker compose up -d postgres` (reusing `docker-compose.yml`) and sets `HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb`. A dedicated `test-integration-db` CI workflow runs on every push/PR and hard-fails if the env var is missing or any DB-backed test is skipped.

**Dimensions:**
- 6.1 ✅ DONE `HANDLER_DB_TEST_URL` is the canonical integration DB variable; `DATABASE_URL` is fallback for local convenience only
- 6.2 ✅ DONE `.github/workflows/test-integration-db.yml` spins up Postgres via `docker compose up -d postgres`, waits for `pg_isready`, then runs `make test-integration-db`
- 6.3 ✅ DONE `make test-integration-db` hard-fails if `HANDLER_DB_TEST_URL` is unset
- 6.4 ✅ DONE Failure policy enforced: exits 1 with clear message if env var missing
- 6.5 ✅ DONE DB-backed proposal-generation and trust-state coverage passes cleanly

**Recorded evidence (Mar 17, 2026):**
- Command: `HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db`
- Result: pass (`217/217`, `4 skipped`)

---

## 7.0 CLI Audit Gate (M6_006 Pre-Acceptance Tighten)

**Status:** DONE (Mar 16, 2026) — shipped in PR #41 `feat/m6-006-cli-audit-tighten`

**Dimensions:**
- 7.1 ✅ DONE Wire dead `agent` command in `cli.js`
- 7.2 ✅ DONE Rename `--profile-id` → `--agent-id`, `--profile-version-id` → `--config-version-id`
- 7.3 ✅ DONE Emoji banner on `--help`/`--version`; suppressed in `--json` and `NO_COLOR`
- 7.4 ✅ DONE "Did you mean?" Levenshtein suggestions for unknown commands
- 7.5 ✅ DONE ID format validation with local error before API round-trip
- 7.6 ✅ DONE Auth guard pre-flight: `zombiectl login` prompt when unauthenticated
- 7.7 ✅ DONE Payload size guard: CLI rejects harness uploads >2MB; server returns `413 UZ-REQ-002`
- 7.8 ✅ DONE `runs list` queries server `GET /v1/runs` — local cache removed
- 7.9 ✅ DONE Operator commands hidden unless `ZOMBIE_OPERATOR=1`
- 7.10 ✅ DONE Doctor pretty print: header, `[OK]`/`[FAIL]` per check, pass/fail summary
- 7.11 ✅ DONE Complete help text: all flags, env vars, `agent` commands
- 7.12 ✅ DONE 12 new test files; 139/139 passing across 28 files

---

## 8.0 Server `GET /v1/runs` List Endpoint

**Status:** DONE (Mar 18, 2026) — shipped in PR feat/m6-006-list-runs

**Dimensions:**
- 8.1 ✅ DONE `src/http/handlers/runs/list.zig` — auth, optional `workspace_id` filter, `limit` param, returns `{ runs, total, request_id }`
- 8.2 ✅ DONE Router dispatches `GET /v1/runs` → `handleListRuns`, `POST /v1/runs` → `handleStartRun`
- 8.3 ✅ DONE Follows pg-drain discipline
- 8.4 ✅ DONE Build, lint, pg-drain check, 152/152 unit tests pass
