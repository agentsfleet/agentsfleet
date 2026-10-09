# Security Model

This is the short policy entrypoint for agentsfleet.

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
6. Execution boundary: `agentsfleetd` assigns work via leases; `agentsfleet-runner` runs each lease's tool calls in a bubblewrap sandbox of its own (`rustd/crates/afr_sandbox`), with the agent loop in its trusted supervisor (`rustd/crates/afr_agent`)

## Sandbox Rules

1. `agentsfleet-runner` leases one event and runs its tool calls in a sandbox of its own (`rustd/crates/afr_sandbox`), destroyed when the lease ends or held frozen for the same fleet's next lease.
2. The agent loop and the model keys stay in the runner's supervisor (`rustd/crates/afr_agent`), outside every sandbox; the runner owns Linux sandbox enforcement (bubblewrap, Landlock, seccomp, cgroup v2) for each lease.
3. If the sandbox posture is unsafe, run admission must fail closed.
4. If a sandboxed child dies mid-stage, the lease expires and the run is reclaimed + re-run by another runner, or blocked from persisted stage state.
5. Active runs are not guaranteed to survive `agentsfleet-runner` upgrades.

## Detection

1. `UZ-SANDBOX-001` backend or prerequisite unavailable
2. `UZ-SANDBOX-002` forced teardown / kill-switch fired
3. `UZ-SANDBOX-003` command blocked by policy
4. Correlate by `trace_id` first, then `run_id`

