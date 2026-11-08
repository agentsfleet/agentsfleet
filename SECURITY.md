# Security Model

This is the short policy entrypoint for agentsfleet.

## Security Objectives

1. Keep control-plane coordination separate from dangerous agent runs.
2. Fail closed on missing runtime prerequisites or degraded sandbox posture.
3. Keep automation credentials short-lived and outside agent output files.
4. Make failures attributable: coordination, sandbox, policy, or validation.

## Core Boundaries

1. Network boundary
2. Data boundary
3. Queue boundary
4. Identity boundary
5. GitHub automation boundary
6. Run boundary: `agentsfleetd` assigns work via leases. `agentsfleet-runner` runs each lease's tool calls in a bubblewrap sandbox of its own (`rustd/crates/runner/afr_sandbox`). The agent loop runs in its trusted supervisor (`rustd/crates/runner/afr_agent`)

## Sandbox Rules

1. `agentsfleet-runner` leases one event and runs its tool calls in a sandbox of its own (`rustd/crates/runner/afr_sandbox`). The runner destroys it when the lease ends, or holds it frozen for the same fleet's next lease.
2. The agent loop and the model keys stay in the runner's supervisor (`rustd/crates/runner/afr_agent`), outside every sandbox. The runner owns Linux sandbox enforcement (bubblewrap, Landlock, seccomp, cgroup v2) for each lease.
3. If the sandbox posture is unsafe, run admission must fail closed.
4. If a sandboxed child dies mid-stage, the lease expires. The run is then reclaimed and re-run by another runner, or blocked from saved stage state.
5. Active runs are not guaranteed to survive `agentsfleet-runner` upgrades.

## Detection

1. `UZ-SANDBOX-001` backend or prerequisite unavailable
2. `UZ-SANDBOX-002` forced teardown / kill-switch fired
3. `UZ-SANDBOX-003` command blocked by policy
4. Correlate by `trace_id` first, then `run_id`

