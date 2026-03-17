# M6_006: Validate v1 Acceptance E2E Gate

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 006
**Date:** Mar 05, 2026
**Status:** PENDING
**Priority:** P0 — release gate
**Batch:** B7 — deferred after M6_003
**Depends on:** M4_001 (Implement `zombiectl` CLI Runtime), M3_006 (Implement Clerk Authentication Contract)
**Note:** M3_004 (Redis streams) and M3_005 (security hardening) already DONE
**Prerequisite Update (Mar 10, 2026):** Zig 0.15.2 compile blockers in `src/auth/github.zig` and `src/pipeline/worker_pr_flow.zig` were resolved; `zig build --summary all` and release-target builds (`x86_64/aarch64` for Linux and macOS) now pass locally.

---

## 1.0 Acceptance Target

**Status:** PENDING

The canonical v1 acceptance target is:

`https://github.com/indykish/terraform-provider-e2e`

**Dimensions:**
- 1.1 PENDING Authenticate with `zombiectl login`
- 1.2 PENDING Connect repo using `zombiectl workspace add`
- 1.3 PENDING Sync specs via `zombiectl specs sync`
- 1.4 PENDING Trigger runs via `zombiectl run`
- 1.5 PENDING Verify outcomes via `zombiectl runs list` and PR evidence

---

## 2.0 Gate Contract

**Status:** PENDING

### 2.1 Functional Gate

**Dimensions:**
- 2.1.1 PENDING Every queued spec reaches terminal state with deterministic reason codes
- 2.1.2 PENDING Every successful run opens a valid PR URL tied to the run record
- 2.1.3 PENDING Failed runs emit actionable failure reason and operator-visible logs

### 2.2 Performance Gate

**Dimensions:**
- 2.2.1 PENDING Track spec-to-PR latency for each run
- 2.2.2 PENDING Meet performance target: under 5 minutes per spec on target baseline infra

### 2.3 Security and Reliability Gate

**Dimensions:**
- 2.3.1 PENDING Verify role-separated DB and Redis ACL paths under acceptance flow
- 2.3.2 PENDING Verify retries/backoff/idempotency behaviors on induced failures

---

## 3.0 Verification Commands

**Status:** PENDING

```bash
npx zombiectl login
npx zombiectl workspace add https://github.com/indykish/terraform-provider-e2e
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

**Dimensions:**
- 3.1 PENDING Capture command outputs and final run/PR summary in release notes
- 3.2 PENDING Store reproducible evidence artifact for final v1 signoff

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 End-to-end acceptance run is repeatable and documented
- [ ] 4.2 Run-state, PR linkage, and failure paths are validated
- [ ] 4.3 Performance and security gates are explicitly evaluated
- [ ] 4.4 v1 release decision can be made from this spec alone

---

## 5.0 Out of Scope

- UI-driven acceptance workflows
- Non-CLI onboarding variants
- Production SRE runbook expansion beyond v1 gate evidence

---

## 6.0 Integration DB Gate Environment (`HANDLER_DB_TEST_URL`)

**Status:** DONE (Mar 17, 2026) — shipped in PR feat/m6-006-db-gate

`HANDLER_DB_TEST_URL` is the canonical env var. CI starts Postgres via `docker compose up -d postgres` (reusing `docker-compose.yml`) and sets `HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb`. A dedicated `test-integration-db` CI workflow (`test-integration-db.yml`) runs on every push/PR and hard-fails if the env var is missing or any DB-backed test is skipped.

**Dimensions:**
- 6.1 DONE `HANDLER_DB_TEST_URL` is the canonical integration DB variable; `DATABASE_URL` is fallback for local convenience only (already in `openHandlerTestConn`, now documented)
- 6.2 DONE `.github/workflows/test-integration-db.yml` spins up Postgres via `docker compose up -d postgres`, waits for `pg_isready`, then runs `make test-integration-db`
- 6.3 DONE `make test-integration-db` target hard-fails if `HANDLER_DB_TEST_URL` is unset — tests cannot silently skip in CI; passing CI run is the acceptance evidence
- 6.4 DONE Failure policy enforced: `make test-integration-db` exits 1 with a clear message if `HANDLER_DB_TEST_URL` is missing, ensuring the gate is always red when DB coverage is absent
- 6.5 DONE DB-backed proposal-generation and trust-state coverage now passes cleanly against the local Postgres harness fixture, including the dynamic auto-agent gate role/skill case and the proposal reconcile path

**Recorded evidence (Mar 17, 2026):**
- Command: `HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db`
- Result: pass (`217/217`, `4 skipped`)
- Notes: fixed remaining pg query-drain handling in proposal generation (`LIMIT 1` readers must drain before `q.deinit()` on early-return paths), removed static echo/warden skill identity assumptions from dynamic topology validation, and normalized DB test teardown/drain handling so the gate no longer aborts during `pool.release()`

---

## 7.0 CLI Audit Gate (M6_006 Pre-Acceptance Tighten)

**Status:** DONE (Mar 16, 2026) — shipped in PR #41 `feat/m6-006-cli-audit-tighten`

CLI hardening completed as a prerequisite to acceptance gate execution. All 139 tests pass.

**Dimensions:**
- 7.1 DONE Wire dead `agent` command in `cli.js` (was registered but never dispatched)
- 7.2 DONE Rename `--profile-id` → `--agent-id`, `--profile-version-id` → `--config-version-id` across harness commands
- 7.3 DONE Emoji banner on `--help`/`--version`; suppressed in `--json` and `NO_COLOR`
- 7.4 DONE "Did you mean?" Levenshtein suggestions for unknown commands (git-style)
- 7.5 DONE ID format validation with local error before API round-trip
- 7.6 DONE Auth guard pre-flight: `zombiectl login` prompt when unauthenticated
- 7.7 DONE Payload size guard: CLI rejects harness uploads >2MB locally before API call; server returns deterministic `413 UZ-REQ-002` with `"Payload too large: max 2MB"` for direct API callers (`checkBodySize` in `common.zig`, `ERR_PAYLOAD_TOO_LARGE` in `codes.zig`)
- 7.8 DONE `runs list` queries server `GET /v1/runs` — local `runs.json` write-behind cache removed
- 7.9 DONE Operator commands (`harness`, `skill-secret`, `agent`) hidden from default `--help` unless `ZOMBIE_OPERATOR=1`
- 7.10 DONE Doctor pretty print: header, `[OK]`/`[FAIL]` per check, pass/fail summary
- 7.11 DONE Complete help text: all flags, env vars, `agent` commands
- 7.12 DONE 12 new test files; 139/139 passing across 28 files
