# M12_003: Executor NullClaw Invocation — Dynamic Agent Execution

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 003
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P0 — without this the executor sidecar is an empty pipe
**Batch:** B1 — completes the executor runtime boundary
**Depends on:** M12_002 (executor API + sidecar binary)

---

## 1.0 Problem

**Status:** PENDING

The executor sidecar (M12_002) accepts lifecycle RPCs and manages sessions, leases, metrics, and sandbox enforcement — but `StartStage` returns a placeholder. No NullClaw invocation happens. The worker sends work to the executor, the executor says "done!" without doing anything.

Without real execution, the entire sidecar architecture is pointless:
- Process isolation has no execution to isolate
- Sandbox enforcement (Landlock, cgroups) has nothing to sandbox
- Lease/heartbeat management has no long-running work to monitor

**Dimensions:**
- 1.1 PENDING Extend the `StartStage` RPC to carry the full agent execution payload so the executor can run any dynamic agent
- 1.2 PENDING The executor must be agent-agnostic — no hardcoded echo/scout/warden. It receives a NullClaw config and runs it
- 1.3 PENDING Worker pipeline must switch from in-process `agents.runByRole()` to `ExecutorClient.startStage()` when executor is configured

---

## 2.0 Protocol Extension: StartStage Payload

**Status:** PENDING

The current `StartStage` RPC only sends `execution_id`, `stage_id`, `role_id`, `skill_id`. That's not enough to run an agent.

**Dimensions:**
- 2.1 PENDING Add `agent_config` object to `StartStage` params — contains NullClaw config (model, provider, system prompt, temperature, max_tokens)
- 2.2 PENDING Add `tools` array to `StartStage` params — list of tool definitions the agent can use (shell, file read/write, memory, custom)
- 2.3 PENDING Add `message` field to `StartStage` params — the user/stage input message that the agent processes
- 2.4 PENDING Add `context` object to `StartStage` params — stage-specific inputs (spec content, plan content, memory context, implementation summary, defects — whatever the topology stage needs)
- 2.5 PENDING Workspace path is already in the session from `CreateExecution` — no duplication needed

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

**Status:** PENDING

A `runner.zig` module that bridges the handler to NullClaw.

**Dimensions:**
- 3.1 PENDING `runner.zig` creates a NullClaw `Agent` from the deserialized `agent_config`
- 3.2 PENDING Runner builds the tool set from the `tools` array, applying sandbox enforcement (shell tool uses `SandboxShellTool` with Landlock + cgroups)
- 3.3 PENDING Runner calls `agent.runSingle(message)` synchronously — the worker blocks on the RPC until execution completes
- 3.4 PENDING Runner captures output content, token count, wall seconds, exit status, and failure class
- 3.5 PENDING Runner applies resource governance: cgroup limits from session's `ResourceLimits`, Landlock from session's workspace path
- 3.6 PENDING On NullClaw error, runner maps to `FailureClass` (timeout → `timeout_kill`, OOM → `oom_kill`, etc.)

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

**Status:** PENDING

When `EXECUTOR_SOCKET_PATH` is set, the worker dispatches stages through the executor instead of running NullClaw in-process.

**Dimensions:**
- 4.1 PENDING `worker_stage_executor.zig` checks for executor client in `ExecuteConfig`
- 4.2 PENDING When executor is available: build `StartStage` payload from `RoleBinding` + `RoleInput`, send via `ExecutorClient.startStage()`
- 4.3 PENDING When executor is NOT available (dev mode, macOS): fall back to direct `agents.runByRole()` as today
- 4.4 PENDING Worker heartbeat thread sends `Heartbeat` RPC during long-running stage execution to keep the lease alive
- 4.5 PENDING On executor failure (`TransportLoss`, `ExecutorCrash`): log error with error code, classify as retryable, let existing retry logic handle

### Backward compatibility

```
EXECUTOR_SOCKET_PATH set     → dispatch via executor (production, baremetal)
EXECUTOR_SOCKET_PATH unset   → direct in-process execution (dev, macOS)
```

This is already the model in `worker.zig` — the executor client is optional. This spec wires the actual dispatch path.

---

## 5.0 Verification

**Status:** PENDING

- 5.1 PENDING Integration test: worker sends `StartStage` with real agent config, executor runs NullClaw, worker receives content
- 5.2 PENDING Integration test: NullClaw timeout triggers `timeout_kill` failure class through executor boundary
- 5.3 PENDING Integration test: OOM inside NullClaw triggers `oom_kill` failure class
- 5.4 PENDING Unit test: runner maps all NullClaw error types to FailureClass
- 5.5 PENDING Unit test: StartStage with missing `agent_config` returns `invalid_params`
- 5.6 PENDING Backward compatibility: worker with no executor runs stages in-process (existing behavior unchanged)

---

## 6.0 Acceptance Criteria

- [ ] 6.1 `StartStage` RPC carries full agent execution payload (config, tools, message, context)
- [ ] 6.2 Executor invokes NullClaw and returns real content, token counts, wall time
- [ ] 6.3 The executor is agent-agnostic — any dynamic agent config works, no hardcoded roles
- [ ] 6.4 Sandbox enforcement (Landlock, cgroups) applies to the NullClaw execution
- [ ] 6.5 Worker falls back to in-process execution when executor is not configured
- [ ] 6.6 All failure classes propagate correctly through the executor boundary

---

## 7.0 Out of Scope

- Firecracker VM execution (future spec, not M12)
- StreamEvents (deferred to v1.1)
- Multi-stage pipelining within a single execution (worker drives stage-by-stage)
- Agent configuration management / registry (worker resolves config, executor just runs it)
