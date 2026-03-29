# Security Posture & Cryptographic Risk Assessment

**Date:** Mar 28, 2026
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

## Credential Injection Model

Agent execution requires three credential types, each with distinct injection paths:

### Anthropic API Key
- Stored in 1Password vault (`op://ZMB_CD_DEV/anthropic-dev/credential`)
- Deployed to worker via `.env` at deploy time
- Passed to executor via `StartStage` RPC payload `agent_config.api_key` field
- Never exposed as executor process environment variable
- Worker halts startup if absent (`UZ-CRED-001`)

### GitHub App Installation Token
- Worker signs JWT using GitHub App private key (already in vault)
- Requests short-lived installation token per-run, scoped to target repo
- Token TTL: 1 hour (GitHub default), refreshed if run approaches 55-minute mark
- Held in memory only, never persisted to database
- Token request failure classifies as `policy_deny` (`UZ-CRED-002`)

### Package Registry Network Allowlist
- Phase 1: executor network policy extended from `deny_all` to allowlist for known registries (npmjs.org, pypi.org, crates.io, pkg.go.dev)
- Phase 2: internal package mirror replaces allowlist for supply chain security
- Controlled by `EXECUTOR_NETWORK_POLICY` env var; default: `deny_all`

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
| `UZ-CRED-001` | Anthropic API key missing at worker startup |
| `UZ-CRED-002` | GitHub App installation token request failed |

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

## Spec Injection Threat Model

A malicious or compromised spec could instruct the agent to write backdoor code. The sandbox prevents data exfiltration during execution (network deny, Landlock filesystem restriction), but the generated code merges via PR.

Mitigations:
- Spec validation rejects specs referencing files outside the repo
- Gate loop (lint/test/build) catches compilation and test failures but NOT semantic security issues
- Phase 1: all PRs require human review (no auto-merge)
- Phase 2: score-gated merge with configurable threshold; low-score PRs require human review
- Future: static analysis gate (semgrep, gitleaks) added to the gate loop
- Future: secret scanning on PR diff before merge

This is an acknowledged gap. No complete solution exists today for semantic code review by agents. The primary mitigation in Phase 1 is human review of every PR.

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
