# M4_007: Runtime Environment Contract

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 007
**Date:** Mar 06, 2026
**Status:** ✅ DONE
**Priority:** P1 — operator safety baseline
**Batch:** B2 — needs M3_006 done (M3_000, M3_004, M4_004 already DONE)
**Depends on:** M3_006 (Implement Clerk Authentication Contract)

---

## 1.0 Define Environment Contract

**Status:** ✅ DONE

Define one authoritative env contract for `zombied` and `zombiectl`.

Resolved policy for "env vs CLI flags" to keep `zombiectl` slim:
- Non-secret operator ergonomics may allow CLI overrides.
- Secret/security-critical values remain env-only.
- Deterministic precedence for supported overrides: `CLI flag > process env > .env.local (dev fallback, non-overriding) > default`.
- Current implemented non-secret CLI override: `zombied serve --port <u16>`.

**Dimensions:**
- 1.1 ✅ DONE Publish canonical contract doc at `docs/CONFIGURATION.md`.
- 1.2 ✅ DONE Define required core/auth/secrets keys (`API_KEY`, `ENCRYPTION_MASTER_KEY`, GitHub/Clerk keys).
- 1.3 ✅ DONE Define role-separated data plane URLs (`DATABASE_URL_API`, `DATABASE_URL_WORKER`, `REDIS_URL_API`, `REDIS_URL_WORKER`) and `rediss://` requirement.
- 1.4 ✅ DONE Define runtime knobs/readiness thresholds (`WORKER_CONCURRENCY`, `RUN_TIMEOUT_MS`, `READY_MAX_QUEUE_DEPTH`, `READY_MAX_QUEUE_AGE_MS`).

---

## 2.0 Enforce Validation And Doctor Semantics

**Status:** ✅ DONE

**Dimensions:**
- 2.1 ✅ DONE Fail startup deterministically on invalid/missing critical env values
- 2.2 ✅ DONE Return errors that identify exact key and expected format
- 2.3 ✅ DONE Extend `zombied doctor` to validate DB, Redis, and auth dependency reachability
- 2.4 ✅ DONE Keep doctor output deterministic and machine-parseable (`zombied doctor --format=json`)

---

## 3.0 Define Telemetry Contract (Schema + Trace Context)

**Status:** ✅ DONE (scope moved)

This scope is intentionally moved out of M4_007 so runtime env closure is not blocked by broader observability delivery.
Canonical owner for deferred implementation remains:
- `docs/spec/v1/M4_005_EVENTS_OBSERVABILITY_AND_CONFIG.md`

Supporting context already landed in docs/code:
- `docs/CONFIGURATION.md` (trace field contract terms)
- `src/observability/metrics.zig` and side-effect outbox metrics
- `src/state/machine.zig` side-effect outbox writes (replay boundary foundation)

**Dimensions:**
- 3.1 ✅ DONE (moved) Versioned event envelope ownership moved to `M4_005` / observability workstreams.
- 3.2 ✅ DONE (moved) Trace field normalization tracked in `M4_005`; baseline contract note exists in `docs/CONFIGURATION.md`.
- 3.3 ✅ DONE (moved) OTEL export path is deferred to `M4_005`; not claimed implemented in M4_007.
- 3.4 ✅ DONE (moved) Durable replay/outbox hardening ownership moved to `M4_005` (foundational outbox already exists).

---

## 4.0 Define Encryption Metadata + Rotation Workflow

**Status:** ✅ DONE (scope moved)

Clarify the two open questions:
- "Key-versioned encryption envelope metadata" means each encrypted value stores *which key version encrypted it*.
- "No-downtime key rotation" means rolling to a new key version without blocking API/worker traffic.

Scope disposition:
- Implemented baseline pieces already landed in `M3_000` (`kek_version` schema and envelope fields).
- Remaining operational rotation workflow and doctor-level rotation checks are deferred to `M4_005`.

**Dimensions:**
- 4.1 ✅ DONE (moved/partially delivered) Envelope metadata baseline delivered in `M3_000`; remaining contract hardening tracked in `M4_005`.
- 4.2 ✅ DONE (moved) Dual-read/single-write rotation logic is explicitly deferred to `M4_005`.
- 4.3 ✅ DONE (moved) Operator rotation workflow docs are deferred to `M4_005`.
- 4.4 ✅ DONE (moved) Rotation-state doctor/health checks are deferred to `M4_005`.

---

## 5.0 Canonicalize Documentation

**Status:** ✅ DONE

**Dimensions:**
- 5.1 ✅ DONE Remove duplicated env references in deployment docs and point to `docs/CONFIGURATION.md`.
- 5.2 ✅ DONE Keep `.env.example` and runtime docs consistent with role-separated `rediss://` contract.
- 5.3 ✅ DONE (moved) Rotation-window operator notes are owned by `M4_005`/security hardening docs, not M4_007.
- 5.4 ✅ DONE Add drift rule/checklist in `docs/CONFIGURATION.md` ("Drift Rule" section).

---

## 6.0 Acceptance Criteria

**Status:** ✅ DONE

- [x] 6.1 Runtime env contract is singular and explicit in `docs/CONFIGURATION.md`.
- [x] 6.2 Startup/runtime enforcement exists in code (`src/config/env_vars.zig`, `src/cmd/serve.zig`, `src/cmd/worker.zig`) with deterministic key-specific failures.
- [x] 6.3 `zombied doctor` validates role/env, DB, Redis, secrets, and auth reachability with machine output (`--format=json` / `--json`) in `src/cmd/doctor.zig`.
- [x] 6.4 Existing tests cover env contract validation and doctor output argument semantics (`src/config/env_vars.zig` tests, `src/cmd/doctor.zig` tests); repo note confirms `zig build test` passing.
- [x] 6.5 Deferred telemetry/rotation hardening is explicitly tracked under `M4_005` and no longer blocks runtime env contract closure in `M4_007`.

---

## 7.0 Out of Scope

- Cross-region config orchestration
- UI/dashboard observability features
- Full production compliance controls beyond v1
