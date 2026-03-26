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
zombied-executor
    |
    +--> NullClaw runtime
    +--> bubblewrap / Landlock / cgroup scope / network policy
```

### Current direction

1. Worker owns orchestration, retries, billing, artifact persistence, and PR creation.
2. `zombied-executor` owns dangerous execution, sandbox policy, timeout teardown, and resource enforcement.
3. Linux sandboxing must be enforced inside the executor boundary, not as an afterthought in worker-side shell wrapping.

### Security consequences

- worker compromise and executor compromise are easier to reason about separately
- timeout, OOM, and kill actions happen inside the same runtime that owns the sandbox
- future Firecracker migration keeps the worker contract stable
- free-plan billing enforcement happens at admission and finalization boundaries, not by unsafe mid-session kill heuristics
- operator/admin control-plane endpoints can now be fenced by normalized role claims instead of relying on hidden CLI help

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
| `zombied-executor` health failure | Executor boundary unavailable |

Correlation fields remain:
- `trace_id`
- `run_id`
- `workspace_id`
- `stage_id`
- `role_id`
- `skill_id`

## RBAC Boundary

UseZombie now treats role as part of the authenticated identity contract:

- `user` can access normal workspace operations
- `operator` can manage harnesses, skill secrets, agent scoring views, and workspace scoring configuration
- `admin` can perform billing lifecycle mutations and API-key-backed administrative operations

Enforcement is server-side. Hidden CLI commands are not treated as a security boundary.
The contract is pinned by live HTTP integration tests that prove `harness`, `skill-secret`, and admin billing-event routes reject under-scoped JWTs with `INSUFFICIENT_ROLE`.

## Free Plan Billing Boundary

The free-plan credit model is intentionally simple and fail-closed:

1. Before admitting run, sync, or operator harness execution, the server reconciles workspace billing state and checks remaining free credit.
2. If credit is exhausted, the request is rejected before execution starts with `UZ-BILLING-005`.
3. Runtime usage is deducted only when a run reaches a completed billable outcome.
4. Failed, interrupted, or otherwise incomplete runs do not consume free credit.

This avoids two bad designs:

- no hidden overdraft path
- no brittle mid-run termination policy pretending to provide precise real-time credit policing

## Out of Scope For v1

- mid-session resume after executor or worker restart
- pretending upgrades are transparent to active agent work
- Firecracker guest isolation in the same milestone
