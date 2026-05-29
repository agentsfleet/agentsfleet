# Security Model

This is the short policy entrypoint for usezombie.

## Security Objectives

1. Keep control-plane orchestration separate from dangerous agent execution.
2. Fail closed on missing runtime prerequisites or degraded sandbox posture.
3. Keep automation credentials short-lived and outside agent artifacts.
4. Make failures attributable: orchestration, sandbox, policy, or validation.

## Core Boundaries

1. Network boundary
2. Data boundary
3. Queue boundary
4. Identity boundary
5. GitHub automation boundary
6. Execution boundary: `zombied` assigns work via leases; `zombie-runner` forks a sandboxed NullClaw child to execute

## Sandbox Rules

1. `zombie-runner` leases one event and forks a sandboxed NullClaw child per run instead of owning the agent runtime forever.
2. `zombie-runner` embeds NullClaw and owns Linux sandbox enforcement for each forked child.
3. If the sandbox posture is unsafe, run admission must fail closed.
4. If a sandboxed child dies mid-stage, the lease expires and the run is reclaimed + re-run by another runner, or blocked from persisted stage state.
5. Active runs are not guaranteed to survive `zombie-runner` upgrades.

## Detection

1. `UZ-SANDBOX-001` backend or prerequisite unavailable
2. `UZ-SANDBOX-002` forced teardown / kill-switch fired
3. `UZ-SANDBOX-003` command blocked by policy
4. Correlate by `trace_id` first, then `run_id`

