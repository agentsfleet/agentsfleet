# M14_001: Orphan Run Recovery and Stale Run Detection

**Prototype:** v1.0.0
**Milestone:** M14
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** DONE
**Priority:** P1 — Runs stuck in RUN_PLANNED after worker crash have no recovery path today
**Batch:** B1
**Depends on:** M13_001 (Worker Fleet Drain and Rolling Deployment)

---

## 1.0 Stale Run Detector

**Status:** DONE

The reconcile daemon (`zombied reconcile --daemon`) runs periodic ticks but currently only reconciles side-effect outbox rows and billing. It does NOT detect runs stuck in non-terminal states (`RUN_PLANNED`, `PLAN_COMPLETE`) after a worker crash. A new reconcile subsystem scans for runs that have been in a non-terminal, non-queued state longer than a configurable staleness threshold and transitions them to `BLOCKED` with reason `WORKER_CRASH_ORPHAN`.

**Current gap:** When the worker process dies mid-run (OOM kill, host reboot, kernel panic), the run stays in `RUN_PLANNED` in Postgres forever. The executor's LeaseManager reaps the in-memory session (§M13 2.3), but nothing updates the database. The Redis stream message is ACK'd before processing begins, so XAUTOCLAIM cannot reclaim it.

**Dimensions:**
- 1.1 ✅ Query: `SELECT run_id FROM runs WHERE state IN ('RUN_PLANNED','PATCH_IN_PROGRESS','PATCH_READY','VERIFICATION_IN_PROGRESS') AND updated_at < cutoff` with `FOR UPDATE SKIP LOCKED`
- 1.2 ✅ Configurable staleness threshold via `ORPHAN_RUN_STALENESS_MS` env var (default: 600000 = 10 min)
- 1.3 ✅ Transition orphaned runs to `BLOCKED` with reason `WORKER_CRASH_ORPHAN` and structured log line
- 1.4 ✅ Prometheus counter `zombie_reconcile_orphan_runs_recovered_total` for alerting

---

## 2.0 Orphan Run Scoring

**Status:** DONE

When a run is orphaned (worker crash), `scoreRunIfTerminal` never fires because it runs in the worker's `defer` block. The reconcile daemon should score orphaned runs with outcome `error_propagation` so quality metrics are not silently dropped.

**Current gap:** `scoring.scoreRunIfTerminal()` is called from `worker_stage_executor.executeRun`'s `defer` block. If the worker process dies, the defer never executes. The run has no score, no quality tier, and no PostHog event — it's invisible to quality dashboards.

**Dimensions:**
- 2.1 ✅ After transitioning an orphaned run to BLOCKED, call `scoring.persistRunAnalysis` with outcome `error_propagation`, zero tokens, and wall_seconds derived from `updated_at - created_at`
- 2.2 ✅ PostHog event `run_orphan_recovered` with run_id, workspace_id, staleness_ms
- 2.3 ✅ Billing finalization: call `billing.finalizeRunForBilling` with `.non_billable` for orphaned runs

---

## 3.0 Automatic Re-queue (Optional)

**Status:** DONE

Orphaned runs that have remaining retry attempts (`attempt < max_attempts`) can be re-queued by transitioning back to `SPEC_QUEUED` and re-publishing to the Redis stream. This is opt-in via `ORPHAN_REQUEUE_ENABLED=true` because automatic retry after a crash may reproduce the crash.

**Current gap:** Runs that land in `BLOCKED` after a worker crash stay there permanently. The only recovery is manual re-trigger via `zombiectl run` or API. For transient failures (host reboot during deploy), automatic re-queue would recover without operator intervention.

**Dimensions:**
- 3.1 ✅ `ORPHAN_REQUEUE_ENABLED` env var (default: false) — opt-in to avoid crash loops
- 3.2 ✅ Re-queue only if `attempt < max_attempts` (default 3); increment attempt counter
- 3.3 ✅ Transition: state → `SPEC_QUEUED` with reason `ORPHAN_REQUEUED` (note: Redis re-publish deferred to re-queue enablement — reconciler does DB-only state reset; next tick picks it up)
- 3.4 ✅ Circuit breaker: if the same run_id was orphaned within 30 minutes, do NOT re-queue — leave in BLOCKED

---

## 4.0 Reconciler Watchdog

**Status:** DONE

The reconcile daemon itself has no watchdog. If it crashes or the advisory lock is held by a dead process, no reconciliation happens. A lightweight health check allows external monitoring (systemd, Fly.io health check) to detect a stalled reconciler.

**Current gap:** `reconcileTick` is called from `runDaemon` in a loop. If the process dies, `g_daemon_state` is nil and the `/healthz` handler on the metrics port reports degraded — but only if something polls it. systemd will restart it (Restart=always), but the advisory lock from the dead process may block the new instance until the Postgres connection times out (default: TCP keepalive, potentially minutes).

**Dimensions:**
- 4.1 ✅ Advisory lock uses `pg_try_advisory_lock` (verified: session-level lock releases on connection close, not process exit)
- 4.2 ✅ Idle timeout: pg.Pool uses TCP connections; advisory lock releases when connection drops. Deployment-time config: set `idle_session_timeout = 30000` on Postgres server for reconciler role to ensure dead connections are reaped within 30s
- 4.3 ✅ Prometheus gauge `zombie_reconcile_running` (1 = healthy, 0 = stopped) for Grafana alerting — set on daemon start, cleared on exit

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 Run stuck in RUN_PLANNED for > staleness threshold is detected and transitioned to BLOCKED
- [x] 5.2 Orphaned run is scored with `error_propagation` outcome and billing finalized as non-billable
- [x] 5.3 (Optional) Orphaned run with remaining attempts is re-queued and completes on retry
- [x] 5.4 Reconciler restart after crash acquires advisory lock within 30s

---

## 6.0 Out of Scope

- Run migration between workers (v2 concern)
- Real-time drain status broadcast to peers (separate observability spec)
- Executor-to-database direct writes (executor has no DB connection by design)
- Automatic root-cause classification of why the worker crashed
