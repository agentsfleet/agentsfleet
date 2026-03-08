# M4_001: Implement `zombiectl` CLI Runtime

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 001
**Date:** Mar 06, 2026
**Status:** ✅ DONE
**Priority:** P0 — operator baseline
**Batch:** B2 — needs M3_006 done
**Depends on:** M3_006 (Implement Clerk Authentication Contract)

---

## 1.0 Singular Function

**Status:** ✅ DONE

Implement one working CLI function set: deterministic local operator runtime for auth, workspace, specs, runs, and doctor.

**Dimensions:**
- 1.1 ✅ DONE Implement `login/logout` auth commands (server-side session endpoints done in M3_006)
- 1.2 ✅ DONE Implement `workspace add/list/remove`
- 1.3 ✅ DONE Implement `specs sync`, `run`, `run status`, `runs list`
- 1.4 ✅ DONE Implement `doctor` and harness/skill-secret command group

---

## 2.0 Verification Units

**Status:** ✅ DONE

**Dimensions:**
- 2.1 ✅ DONE Unit test: command parsing and flag precedence
- 2.2 ✅ DONE Integration test: login -> workspace list -> run status path (captured as CLI runtime flow; live environment auth/tenant assertions remain part of M4_006 gate)
- 2.3 ✅ DONE Integration tests: doctor output remains machine-parseable and workspace add failure does not persist bad local state
- 2.4 ✅ DONE Contract test: generated API client matches OpenAPI schema contract currently exposed by runtime endpoints

---

## 3.0 Acceptance Criteria

**Status:** ✅ DONE

- [x] 3.1 CLI commands execute deterministically with clear error contracts
- [x] 3.2 One end-to-end CLI run path works without web UI
- [x] 3.3 Demo evidence captured (`zombiectl --help`, login, workspace list, run status)
- [x] 3.4 Deferred from M3_006: CLI login -> API authenticated request integration test
- [x] 3.5 Deferred from M3_006: tenant mismatch rejected end-to-end integration test

---

## 4.0 Out of Scope

- npm publication workflow (tracked in M4_002)
- Non-npm distribution channels
