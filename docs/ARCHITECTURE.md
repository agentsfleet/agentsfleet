# UseZombie Architecture (v1 Canonical)

Date: Mar 22, 2026
Status: Canonical architecture baseline for current implementation and near-term direction

## Goal

UseZombie accepts a spec request and produces a validated pull request through a deterministic control plane with explicit retries, auditable artifacts, and a replaceable execution substrate.

## Version Roadmap

### v1 — Ship

1. **Queue:** Redis streams for worker coordination.
2. **Execution contract:** worker orchestrates runs through a local `sandbox-executor` API.
3. **Execution backend:** `sandbox-executor` embeds NullClaw and applies host-level sandboxing on Linux.
4. **Git:** hardened git CLI subprocess.
5. **Auth:** Clerk for user/API auth, GitHub App for automation.
6. **Delivery:** `zombiectl` CLI.

### v2 — Harden

1. **Execution backend:** Firecracker behind the same executor API.
2. **Git:** libgit2 native calls.
3. **Scaling:** multi-worker concurrency with stronger admission control.
4. **Encryption:** fuller key management and rotation automation.

### v3 — Scale

1. **Mission Control UI:** `app.usezombie.com`.
2. **Team model:** richer workspace and policy controls.
3. **Billing:** deeper usage metering and policy controls.

## Canonical Assumptions

1. `zombied` is split into API and worker roles.
2. Postgres is the source of truth for run state, artifacts, and policy history.
3. Redis is mandatory for queueing and worker coordination.
4. Worker orchestration and dangerous code execution are separate runtime boundaries.
5. v1 execution durability is stage-boundary durability, not mid-token session migration.
6. The worker-facing executor contract must survive a future backend swap from host sandbox to Firecracker.

## System Components

1. `zombiectl`: CLI used by humans or agents to submit work and inspect runs.
2. `zombied API`: validates requests, persists run metadata, enqueues work.
3. `zombied worker`: claims work, resolves active harness/profile, drives stage state transitions, persists artifacts, handles retries, billing, and PR creation.
4. `sandbox-executor`: local execution service controlled by the worker over a typed API; owns sandbox lifecycle and agent runtime execution.
5. `Redis`: stream-based queue + consumer-group coordination.
6. `Postgres`: run state, artifacts, transitions, usage, policy events, vault data.
7. `Clerk`: identity for CLI and API.
8. `GitHub App`: automation credential source for git push and PR creation.

## Canonical Execution Lifecycle

1. `spec request`: `zombiectl` submits a run request to the API.
2. `worker scheduling`: API writes the run row and enqueues `run_id` in Redis.
3. `profile resolution`: worker resolves the active workspace harness/profile.
4. `execution lease`: worker opens an executor session for the active stage.
5. `sandbox execution`: `sandbox-executor` runs the stage via embedded NullClaw inside the selected sandbox backend.
6. `result evaluation`: worker persists verdict, artifacts, metrics, and failure classification in Postgres.
7. `iteration loop`: on retryable failure, worker re-enqueues the same `run_id`.
8. `PR creation`: on pass, worker pushes branch and opens PR via GitHub App installation token.

## Runtime Boundary

```text
zombiectl / API
      |
      v
  zombied worker
      |
      | executor API
      v
 sandbox-executor
      |
      +--> NullClaw embedded runtime
      +--> bubblewrap / Landlock / cgroup scope / network policy
      +--> usage + failure telemetry
```

### Why this boundary exists

- keep agent execution crashes and kill paths out of the worker process
- make Linux sandbox enforcement authoritative in one place
- preserve one worker contract as host sandboxing evolves into Firecracker

## Failure And Restart Model

UseZombie is **durable at stage boundaries**:
- run state is persisted in Postgres
- queue state is persisted in Redis
- in-flight agent process state is not durable

Operationally:
- if `sandbox-executor` crashes mid-stage, the run is retried or blocked from persisted state
- if the worker crashes mid-stage, the active lease is eventually lost and the stage is restarted
- upgrading worker or executor interrupts in-flight work unless the operator first drains active runs

This is deliberate. We prefer honest restart semantics over pretending we support mid-session migration.

## Dynamic Harness/Profile Model

Harnesses remain workspace-scoped and profile-driven:
- operators store source
- compile candidate versions
- activate valid versions
- worker resolves the active version before execution

Echo, Scout, and Warden remain the built-in defaults, but sandboxing hangs off the stage execution context rather than static names.

## v2 Firecracker Model

Firecracker is a backend swap behind the same executor contract:

```text
worker
  -> CreateExecution / StartStage / StreamEvents / CancelExecution / DestroyExecution
  -> backend=firecracker
  -> guest runtime executes stage
  -> worker receives the same typed results and failure signals
```

The point of the executor API is to avoid rewriting the control plane when the backend changes.

