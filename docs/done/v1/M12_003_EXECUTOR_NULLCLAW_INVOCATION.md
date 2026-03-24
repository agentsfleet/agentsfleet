# M12_003: Executor NullClaw Invocation — Dynamic Agent Execution

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 003
**Date:** Mar 23, 2026
**Status:** DONE
**Priority:** P0 — without this the executor sidecar is an empty pipe
**Batch:** B1 — completes the executor runtime boundary
**Depends on:** M12_002 (executor API + sidecar binary)

---

## 1.0 Problem

**Status:** DONE

The executor sidecar (M12_002) accepts lifecycle RPCs and manages sessions, leases, metrics, and sandbox enforcement — but `StartStage` returns a placeholder. No NullClaw invocation happens. The worker sends work to the executor, the executor says "done!" without doing anything.

Without real execution, the entire sidecar architecture is pointless:
- Process isolation has no execution to isolate
- Sandbox enforcement (Landlock, cgroups) has nothing to sandbox
- Lease/heartbeat management has no long-running work to monitor

**Dimensions:**
- 1.1 DONE Extend the `StartStage` RPC to carry the full agent execution payload so the executor can run any dynamic agent
- 1.2 DONE The executor must be agent-agnostic — no hardcoded echo/scout/warden. It receives a NullClaw config and runs it
- 1.3 DONE Worker pipeline must switch from in-process `agents.runByRole()` to `ExecutorClient.startStage()` when executor is configured

---

## 2.0 Protocol Extension: StartStage Payload

**Status:** DONE

The current `StartStage` RPC only sends `execution_id`, `stage_id`, `role_id`, `skill_id`. That's not enough to run an agent.

**Dimensions:**
- 2.1 DONE Add `agent_config` object to `StartStage` params — contains NullClaw config (model, provider, system prompt, temperature, max_tokens)
- 2.2 DONE Add `tools` array to `StartStage` params — list of tool definitions the agent can use (shell, file read/write, memory, custom)
- 2.3 DONE Add `message` field to `StartStage` params — the user/stage input message that the agent processes
- 2.4 DONE Add `context` object to `StartStage` params — stage-specific inputs (spec content, plan content, memory context, implementation summary, defects — whatever the topology stage needs)
- 2.5 DONE Workspace path is already in the session from `CreateExecution` — no duplication needed

### StartStage params schema (v1)

```
StartStage params:
{
  "execution_id": "hex-16-bytes",
  "stage_id": "plan",
  "role_id": "analyst",
  "skill_id": "analyst",

  "agent_config": {
    "model": "claude-sonnet-4-20250514",
    "provider": "anthropic",
    "system_prompt": "You are a software architect...",
    "temperature": 0.7,
    "max_tokens": 16384
  },

  "tools": [
    { "name": "file_read", "enabled": true },
    { "name": "file_write", "enabled": true },
    { "name": "shell", "enabled": true, "sandbox": true },
    { "name": "memory_read", "enabled": true },
    { "name": "memory_write", "enabled": true }
  ],

  "message": "Analyze this spec and produce an implementation plan...",

  "context": {
    "spec_content": "# Feature: ...",
    "plan_content": null,
    "memory_context": "Previous observations: ...",
    "defects_content": null,
    "implementation_summary": null
  }
}
```

The executor does NOT interpret `context` fields — it passes them through to the NullClaw agent via the message or tool context. The worker is responsible for assembling the right context per stage.

---

## 3.0 Executor Runner Module

**Status:** DONE

A `runner.zig` module that bridges the handler to NullClaw.

**Dimensions:**
- 3.1 DONE `runner.zig` creates a NullClaw `Agent` from the deserialized `agent_config`
- 3.2 DONE Runner builds the tool set from the `tools` array, applying sandbox enforcement (shell tool uses `SandboxShellTool` with Landlock + cgroups)
- 3.3 DONE Runner calls `agent.runSingle(message)` synchronously — the worker blocks on the RPC until execution completes
- 3.4 DONE Runner captures output content, token count, wall seconds, exit status, and failure class
- 3.5 DONE Runner applies resource governance: cgroup limits from session's `ResourceLimits`, Landlock from session's workspace path
- 3.6 DONE On NullClaw error, runner maps to `FailureClass` (timeout → `timeout_kill`, OOM → `oom_kill`, etc.)

### Call chain

```
Worker                          Executor Sidecar
  │                                │
  ├─ startStage(payload) ─────────►│
  │                                ├─ handler.handleStartStage()
  │                                │   ├─ validate params
  │                                │   ├─ check session lease
  │                                │   ├─ runner.execute(session, agent_config, tools, message, context)
  │                                │   │   ├─ NullClaw Config.fromParams(agent_config)
  │                                │   │   ├─ build tool set with sandbox enforcement
  │                                │   │   ├─ Agent.init(config, tools)
  │                                │   │   ├─ agent.runSingle(message)  ← BLOCKING
  │                                │   │   ├─ capture result (content, tokens, wall_time)
  │                                │   │   └─ return ExecutionResult
  │                                │   ├─ session.recordStageResult(result)
  │                                │   ├─ session.touchLease()
  │                                │   └─ serialize JSON-RPC response
  │◄─ StageResult ─────────────────┤
  │                                │
```

---

## 4.0 Worker Pipeline Switch

**Status:** DONE

When `EXECUTOR_SOCKET_PATH` is set, the worker dispatches stages through the executor instead of running NullClaw in-process.

**Dimensions:**
- 4.1 DONE `worker_stage_executor.zig` checks for executor client in `ExecuteConfig`
- 4.2 DONE When executor is available: build `StartStage` payload from `RoleBinding` + `RoleInput`, send via `ExecutorClient.startStage()`
- 4.3 DONE When executor is NOT available (dev mode, macOS): fall back to direct `agents.runByRole()` as today
- 4.4 DONE Worker heartbeat thread sends `Heartbeat` RPC during long-running stage execution to keep the lease alive
- 4.5 DONE On executor failure (`TransportLoss`, `ExecutorCrash`): log error with error code, classify as retryable, let existing retry logic handle

### Backward compatibility

```
EXECUTOR_SOCKET_PATH set     → dispatch via executor (production, baremetal)
EXECUTOR_SOCKET_PATH unset   → direct in-process execution (dev, macOS)
```

This is already the model in `worker.zig` — the executor client is optional. This spec wires the actual dispatch path.

---

## 5.0 Observability: Tracing, Metrics & Logging

**Status:** DONE

The runner module must participate in the existing observability stack so that NullClaw invocations are visible in Grafana Cloud and PostHog.

**Dimensions:**
- 5.1 DONE **Structured logging** — scoped `std.log.scoped(.executor_runner)`. Every invocation logs: `executor.runner.start execution_id={hex} stage_id={s} role_id={s} model={s}` and `executor.runner.done execution_id={hex} exit_ok={} tokens={d} wall_seconds={d} failure={?s}`
- 5.2 DONE **Error codes** — Add `ERR_EXEC_RUNNER_AGENT_INIT` (`UZ-EXEC-012`), `ERR_EXEC_RUNNER_AGENT_RUN` (`UZ-EXEC-013`), `ERR_EXEC_RUNNER_INVALID_CONFIG` (`UZ-EXEC-014`) to `src/errors/codes.zig`
- 5.3 DONE **Executor metrics** — Add to `executor_metrics.zig`: `zombie_executor_stages_started_total`, `zombie_executor_stages_completed_total`, `zombie_executor_stages_failed_total`, `zombie_executor_agent_tokens_total`, `zombie_executor_agent_duration_seconds` (histogram: 1,3,5,10,30,60,120,300s buckets)
- 5.4 DONE **Failure-class metric routing** — On failure, increment the appropriate existing metric (`incExecutorOomKills`, `incExecutorTimeoutKills`, `incExecutorLandlockDenials`, `incExecutorResourceKills`) plus the new `stages_failed_total`
- 5.5 DONE **Log on error with error_code** — All error paths log with the `error_code=UZ-EXEC-0XX` pattern consistent with the rest of the codebase

### Metric names (Prometheus)

```
zombie_executor_stages_started_total      counter
zombie_executor_stages_completed_total    counter
zombie_executor_stages_failed_total       counter
zombie_executor_agent_tokens_total        counter
zombie_executor_agent_duration_seconds    histogram (1,3,5,10,30,60,120,300)
```

These are emitted via the existing `executor_metrics.zig` atomic counter pattern, re-exported through the main metrics facade for Prometheus scrape and OTLP push to Grafana Cloud.

PostHog events are emitted by the **worker** (not the executor) — the executor returns structured results and the worker calls `posthog_events.trackAgentCompleted()` as it does today. No new PostHog integration needed in the executor sidecar.

---

## 6.0 Verification

**Status:** DONE

- 6.1 DONE Integration test: worker sends `StartStage` with real agent config, executor runs NullClaw, worker receives content
- 6.2 DONE Integration test: NullClaw timeout triggers `timeout_kill` failure class through executor boundary
- 6.3 DONE Integration test: OOM inside NullClaw triggers `oom_kill` failure class
- 6.4 DONE Unit test: runner maps all NullClaw error types to FailureClass
- 6.5 DONE Unit test: StartStage with missing `agent_config` returns `invalid_params`
- 6.6 DONE Backward compatibility: worker with no executor runs stages in-process (existing behavior unchanged)
- 6.7 DONE Unit test: new executor metrics increment correctly (stages_started, stages_completed, stages_failed, agent_tokens, duration histogram)
- 6.8 DONE Unit test: error codes (UZ-EXEC-012/013/014) appear in log output on error paths

---

## 7.0 Acceptance Criteria

- [x] 7.1 `StartStage` RPC carries full agent execution payload (config, tools, message, context)
- [x] 7.2 Executor invokes NullClaw and returns real content, token counts, wall time
- [x] 7.3 The executor is agent-agnostic — any dynamic agent config works, no hardcoded roles
- [x] 7.4 Sandbox enforcement (Landlock, cgroups) applies to the NullClaw execution
- [x] 7.5 Worker falls back to in-process execution when executor is not configured
- [x] 7.6 All failure classes propagate correctly through the executor boundary
- [x] 7.7 Structured logs emitted on start/complete/fail with error_code pattern
- [x] 7.8 Executor metrics (stages_started/completed/failed, tokens, duration) increment correctly

---

## 8.0 Out of Scope

- Firecracker VM execution (future spec, not M12)
- StreamEvents (deferred to v1.1)
- Multi-stage pipelining within a single execution (worker drives stage-by-stage)
- Agent configuration management / registry (worker resolves config, executor just runs it)
