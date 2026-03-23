# Security Posture & Cryptographic Risk Assessment

**Date:** Mar 22, 2026
**Status:** Living document

## Security Boundary For Agent Execution

UseZombie now treats agent execution as a separate runtime boundary from worker orchestration.

```text
worker/control plane
    |
    | executor API
    v
sandbox-executor
    |
    +--> NullClaw runtime
    +--> bubblewrap / Landlock / cgroup scope / network policy
```

### Current direction

1. Worker owns orchestration, retries, billing, artifact persistence, and PR creation.
2. `sandbox-executor` owns dangerous execution, sandbox policy, timeout teardown, and resource enforcement.
3. Linux sandboxing must be enforced inside the executor boundary, not as an afterthought in worker-side shell wrapping.

### Security consequences

- worker compromise and executor compromise are easier to reason about separately
- timeout, OOM, and kill actions happen inside the same runtime that owns the sandbox
- future Firecracker migration keeps the worker contract stable

## Failure Semantics

The security model is fail-closed, but not mid-session durable.

1. If executor startup posture is unsafe, worker must refuse to run.
2. If executor dies mid-stage, the run must fail or retry from persisted stage state.
3. If worker dies, executor must not continue forever without a valid lease.
4. Upgrades may interrupt active stages unless the operator drains them first.

This is safer than pretending a hidden agent session survived an infrastructure event.

## Sandbox Detection Signals

| Signal | Meaning |
|--------|---------|
| `UZ-SANDBOX-001` | Sandbox backend or Linux prerequisite unavailable |
| `UZ-SANDBOX-002` | Kill-switch or forced sandbox teardown fired |
| `UZ-SANDBOX-003` | Command blocked before execution |
| `sandbox-executor` health failure | Executor boundary unavailable |

Correlation fields remain:
- `trace_id`
- `run_id`
- `workspace_id`
- `stage_id`
- `role_id`
- `skill_id`

## Out of Scope For v1

- mid-session resume after executor or worker restart
- pretending upgrades are transparent to active agent work
- Firecracker guest isolation in the same milestone

