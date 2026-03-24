# UseZombie Architecture (v1 Canonical)

Date: Mar 24, 2026
Status: Canonical architecture baseline for current implementation and near-term direction

## Goal

UseZombie accepts a spec request and produces a validated pull request through a deterministic control plane with explicit retries, auditable artifacts, and a replaceable execution substrate.

## Version Roadmap

### v1 — Ship

1. **Queue:** Redis streams for worker coordination.
2. **Execution contract:** worker orchestrates runs through a local `zombied-executor` API.
3. **Execution backend:** `zombied-executor` embeds NullClaw and applies host-level sandboxing on Linux.
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
4. `zombied-executor`: local execution service controlled by the worker over a typed API; owns sandbox lifecycle and agent runtime execution.
5. `Redis`: stream-based queue + consumer-group coordination.
6. `Postgres`: run state, artifacts, transitions, usage, policy events, vault data.
7. `Clerk`: identity for CLI and API.
8. `GitHub App`: automation credential source for git push and PR creation.

## Canonical Execution Lifecycle

1. `spec request`: `zombiectl` submits a run request to the API.
2. `worker scheduling`: API writes the run row and enqueues `run_id` in Redis.
3. `profile resolution`: worker resolves the active workspace harness/profile.
4. `execution lease`: worker opens an executor session for the active stage.
5. `sandbox execution`: `zombied-executor` runs the stage via embedded NullClaw inside the selected sandbox backend.
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
      | JSON-RPC over Unix socket (/run/zombie/executor.sock)
      | CreateExecution → StartStage(agent_config, tools, message, context) → GetUsage → DestroyExecution
      v
 zombied-executor
      |
      +--> runner.zig (agent-agnostic NullClaw bridge)
      |       +--> Config from env + RPC overrides (model, provider, temperature, max_tokens)
      |       +--> Tool set from RPC spec or allTools() fallback
      |       +--> Agent.fromConfig() → agent.runSingle(composed_message) → ExecutionResult
      |
      +--> Landlock (filesystem policy)
      +--> cgroups v2 (memory + CPU limits)
      +--> network policy (deny_all)
      +--> executor_metrics → Prometheus /metrics → Grafana Cloud
      +--> structured logs → Loki via OTLP
```

### Fallback path (dev / macOS)

When `EXECUTOR_SOCKET_PATH` is unset, the worker runs NullClaw in-process via `agents.runByRole()`. This preserves local development without requiring the executor sidecar.

### Why this boundary exists

- keep agent execution crashes and kill paths out of the worker process
- make Linux sandbox enforcement authoritative in one place
- preserve one worker contract as host sandboxing evolves into Firecracker
- the executor is agent-agnostic — it receives a NullClaw config and runs any dynamic agent

## Sandbox Architecture

The executor sidecar enforces four isolation layers on every agent execution. All enforcement is Linux-only; macOS falls back to in-process execution without sandboxing.

### 1. Landlock — filesystem policy

Landlock is a Linux Security Module that restricts filesystem access at the process level using three syscalls (`landlock_create_ruleset`, `landlock_add_rule`, `landlock_restrict_self`).

**Policy applied per execution:**

| Path | Access |
|------|--------|
| Workspace directory (`/tmp/workspace/...`) | Read + Write + Create + Remove + Symlink |
| System paths (`/usr`, `/bin`, `/lib`, `/etc`, `/dev`, `/proc`, `/tmp`) | Read-only + Execute |
| Everything else | **Denied** |

The policy is applied by `landlock.zig` before the NullClaw agent runs. An agent cannot read or write files outside its workspace and the system paths required for tool execution.

**Implementation:** `src/executor/landlock.zig` — raw syscall wrappers (444, 445, 446). No libc dependency. Falls back to no-op on non-Linux.

### 2. cgroups v2 — resource limits

Each execution gets a transient cgroup scope under `/sys/fs/cgroup/zombie.executor/exec-{execution_id}/`.

**Limits enforced:**

| Resource | Control file | Default |
|----------|-------------|---------|
| Memory hard limit | `memory.max` | 512 MB (`EXECUTOR_MEMORY_LIMIT_MB`) |
| CPU quota | `cpu.max` | 100% of one core (`EXECUTOR_CPU_LIMIT_PERCENT`) |

**OOM detection:** After execution, the runner reads `memory.events` to check if `oom_kill > 0`. If so, the result is classified as `FailureClass.oom_kill` and the `zombie_executor_oom_kills_total` metric increments.

**Peak tracking:** `memory.peak` is read after execution and recorded via `setExecutorMemoryPeakBytes()` for the `zombie_executor_memory_peak_bytes` gauge.

**Cleanup:** The cgroup scope directory is deleted on `DestroyExecution`.

**Implementation:** `src/executor/cgroup.zig` — creates/destroys transient scopes, reads event counters. Falls back to no-op on non-Linux.

### 3. Network policy — egress denial

Network access is denied by default using bubblewrap's `--unshare-net` flag, which places the process in a new network namespace with no interfaces. This prevents agents from making outbound HTTP calls, exfiltrating data, or reaching internal services.

**Implementation:** `src/executor/network.zig` — `deny_all` policy appends bwrap network args. Future `allowlist` policy is stubbed for v2.

### 4. Process isolation — sidecar boundary

The executor runs as a separate systemd service (`zombied-executor.service`) with hardened unit configuration:

```ini
NoNewPrivileges=yes          # prevent privilege escalation
ProtectSystem=strict         # mount /usr, /boot, /efi read-only
ProtectHome=yes              # hide /home, /root, /run/user
ReadWritePaths=/run/zombie /sys/fs/cgroup /tmp
```

The worker communicates with the executor over a Unix socket (`/run/zombie/executor.sock`) using length-prefixed JSON-RPC. If the executor crashes, the worker receives a transport error, classifies the stage as `FailureClass.executor_crash`, and the existing retry logic handles recovery.

### Sandbox enforcement flow

```text
Worker: startStage(payload) ──────────────► Executor
                                              │
                                              ├─ 1. Create cgroup scope (memory.max, cpu.max)
                                              ├─ 2. Apply Landlock policy (workspace RW, system RO)
                                              ├─ 3. Apply network policy (deny_all)
                                              ├─ 4. runner.execute():
                                              │      ├─ Config.load() + RPC overrides
                                              │      ├─ build tools from spec
                                              │      ├─ Agent.fromConfig() → agent.runSingle()
                                              │      └─ capture content, tokens, wall_time
                                              ├─ 5. Check memory.events for oom_kill
                                              ├─ 6. Record memory.peak to metrics
                                              ├─ 7. session.recordStageResult()
                                              └─ 8. Return ExecutionResult via JSON-RPC
```

### Failure classification

Every execution failure is mapped to a `FailureClass` that propagates through the executor boundary to the worker:

| FailureClass | Trigger | Error code | Metric |
|-------------|---------|------------|--------|
| `startup_posture` | Config load or agent init failure | UZ-EXEC-012 | `stages_failed_total` |
| `timeout_kill` | Agent exceeds deadline | UZ-EXEC-003 | `timeout_kills_total` |
| `oom_kill` | cgroup memory.events shows oom_kill | UZ-EXEC-004 | `oom_kills_total` |
| `resource_kill` | CPU or disk limit exceeded | UZ-EXEC-005 | `resource_kills_total` |
| `landlock_deny` | Filesystem access outside policy | UZ-EXEC-011 | `landlock_denials_total` |
| `policy_deny` | Network or tool policy violation | UZ-EXEC-008 | `failures_total` |
| `executor_crash` | Agent runtime error | UZ-EXEC-013 | `stages_failed_total` |
| `transport_loss` | Unix socket disconnection | UZ-EXEC-006 | `failures_total` |
| `lease_expired` | Heartbeat timeout | UZ-EXEC-007 | `lease_expired_total` |

## Executor Observability

### Metrics (Prometheus → Grafana Cloud)

Rendered at `/metrics` by the zombied API process. The executor sidecar emits atomic counters; the main binary reads them via `executor_metrics.zig` and renders Prometheus text format.

**Session lifecycle:**
- `zombie_executor_sessions_created_total` — counter
- `zombie_executor_sessions_active` — gauge
- `zombie_executor_failures_total` — counter
- `zombie_executor_cancellations_total` — counter

**Stage execution:**
- `zombie_executor_stages_started_total` — counter
- `zombie_executor_stages_completed_total` — counter
- `zombie_executor_stages_failed_total` — counter
- `zombie_executor_agent_tokens_total` — counter
- `zombie_executor_agent_duration_seconds` — histogram (buckets: 1, 3, 5, 10, 30, 60, 120, 300s)

**Sandbox enforcement:**
- `zombie_executor_oom_kills_total` — counter
- `zombie_executor_timeout_kills_total` — counter
- `zombie_executor_landlock_denials_total` — counter
- `zombie_executor_resource_kills_total` — counter
- `zombie_executor_lease_expired_total` — counter
- `zombie_executor_cpu_throttled_ms_total` — counter
- `zombie_executor_memory_peak_bytes` — gauge

### Structured logs (Loki via OTLP)

Scoped logger: `std.log.scoped(.executor_runner)`.

- `executor.runner.start execution_id={hex} stage_id={s} role_id={s} model={s}`
- `executor.runner.done exit_ok=true tokens={d} wall_seconds={d}`
- `executor.runner.failed error_code=UZ-EXEC-0XX err={s} wall_seconds={d}`

All error paths include `error_code=` for Loki filtering and Grafana alerting.

### Error codes

| Code | Meaning |
|------|---------|
| UZ-EXEC-001 | Session create failed |
| UZ-EXEC-002 | Stage start failed |
| UZ-EXEC-003 | Timeout kill |
| UZ-EXEC-004 | OOM kill |
| UZ-EXEC-005 | Resource kill |
| UZ-EXEC-006 | Transport loss |
| UZ-EXEC-007 | Lease expired |
| UZ-EXEC-008 | Policy deny |
| UZ-EXEC-009 | Startup posture check failed |
| UZ-EXEC-010 | Executor crash |
| UZ-EXEC-011 | Landlock deny |
| UZ-EXEC-012 | Runner agent init failed |
| UZ-EXEC-013 | Runner agent execution failed |
| UZ-EXEC-014 | Runner invalid config |

### Product analytics (PostHog)

PostHog events are emitted by the **worker**, not the executor. After the executor returns a result, the worker calls `posthog_events.trackAgentCompleted()` with actor, tokens, duration, and exit status. The executor boundary is infra-only — PostHog tracks product-level behavior.

## Deployment

### Fly.io (API + reconciler)

The `zombied` API runs on Fly.io as a Docker container. It serves HTTP on port 3000, exposes `/metrics` on port 9091, and is fronted by a Cloudflare Tunnel (no public `*.fly.dev` domain).

### Bare-metal (worker + executor)

The worker and executor run on OVH bare-metal servers as systemd services. Deployment is via SSH over Tailscale.

```text
/opt/zombie/
  ├── bin/
  │   ├── zombied              ← worker binary
  │   └── zombied-executor     ← executor sidecar binary
  ├── deploy/
  │   ├── zombied-worker.service
  │   └── zombied-executor.service
  ├── deploy.sh                ← installs binaries + systemd units
  └── .env                     ← from 1Password vault
```

**`deploy.sh` sequence:**
1. Install binaries to `/usr/local/bin/`
2. Copy systemd units to `/etc/systemd/system/`
3. `systemctl daemon-reload`
4. Copy `.env` to `/etc/default/zombied-worker`
5. Enable and start `zombied-executor` (starts first)
6. Enable and start `zombied-worker` (Requires= executor)

**Systemd ordering:** `zombied-executor.service` has `Before=zombied-worker.service`. The worker's unit has `Requires=zombied-executor.service` — if the executor fails, the worker stops.

### CI pipeline

| Workflow | Trigger | What it does |
|----------|---------|-------------|
| `test.yml` | push/PR | `make test-zombied` (main + executor unit tests) + zombiectl + website + app |
| `cross-compile.yml` | push/PR | Compile both binaries for x86_64-linux, aarch64-linux, aarch64-macos |
| `lint.yml` | push/PR | zig fmt, ZLint, pg-drain, ESLint, TypeScript |
| `memleak.yml` | push/PR | Valgrind + allocator tests (Linux) |
| `test-integration.yml` | push/PR | DB + Redis integration tests |
| `deploy-dev.yml` | push to main | Docker → Fly.io (API), SSH → OVH (worker + executor) |
| `release.yml` | tag `v*` | Multi-arch binaries, Docker, npm, GitHub Release, prod deploy |

## Failure And Restart Model

UseZombie is **durable at stage boundaries**:
- run state is persisted in Postgres
- queue state is persisted in Redis
- in-flight agent process state is not durable

Operationally:
- if `zombied-executor` crashes mid-stage, the run is retried or blocked from persisted state
- if the worker crashes mid-stage, the active lease is eventually lost and the stage is restarted
- upgrading worker or executor interrupts in-flight work unless the operator first drains active runs

This is deliberate. We prefer honest restart semantics over pretending we support mid-session migration.

## Dynamic Harness/Profile Model

Harnesses are workspace-scoped and profile-driven:
- operators store source
- compile candidate versions
- activate valid versions
- worker resolves the active version before execution

Stages are defined in the active profile (JSON topology) — not hardcoded. Built-in skill kinds (`echo`, `scout`, `warden`) provide defaults, but custom skills can be registered via `SkillRegistry` and referenced by any profile stage. The executor is agent-agnostic: it receives a NullClaw config + tool spec + message from the worker and runs it without interpreting roles.

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
