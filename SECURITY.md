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
6. Run boundary: `agentsfleetd` assigns work via runs. `agentsfleet-runner` runs each run's tool calls in a sandbox of its own, built by its engine behind `rustd/crates/runner/afr_engine`. The agent loop runs in its trusted supervisor (`rustd/crates/runner/afr_agent`)

## Sandbox Rules

1. `agentsfleet-runner` runs one event and runs its tool calls in a sandbox of its own. The runner destroys it when the run ends, or parks it for the same fleet's next run.
2. The agent loop and the model keys stay in the runner's supervisor (`rustd/crates/runner/afr_agent`), outside every sandbox. No secret enters a sandbox: the fetch token stays with the host's repository mirror, and files cross only through the executor.
3. The engine is the operator's `--engine` choice. bubblewrap (`afr_engine_bubblewrap`) confines a process tree on the host kernel with namespaces, Landlock, seccomp and cgroup v2. Firecracker (`afr_engine_firecracker`) gives each run a microVM with its own kernel.
4. If the sandbox posture is unsafe, run admission must fail closed. A Firecracker runner whose `/dev/kvm` will not open refuses to boot, and no other engine starts in its place.
5. If a sandboxed child dies mid-stage, the run expires. The run is then reclaimed and re-run by another runner, or blocked from saved stage state.
6. Active runs are not guaranteed to survive `agentsfleet-runner` upgrades.

## The microVM wall

Under Firecracker, a tenant's code runs inside a guest kernel, so a kernel bug inside the guest stays inside that microVM. Between two microVMs on one host, each jail has its own user and group, and each workspace image is locked to one monitor. Each tap's host table admits only its run's plan, each vsock reaches only its own host socket, and kernel same-page merging stays off.

Only the host reaches a guest's executor. The executor drops a vsock peer that is not the host, and the guest kernel has no vsock loopback.

Simultaneous multithreading (SMT) stays on, on every host, by Indy's ruling ("On everywhere", Nov 26, 2026). Two microVMs may therefore share one physical core, and a guest may read another's activity through cache timing. That cross-VM cache-timing risk is accepted.

Under bubblewrap, every sandbox shares the host kernel, and a kernel privilege-escalation bug escapes it. Production runs bubblewrap until M258_003 moves it to Firecracker.

## Detection

1. `UZ-SANDBOX-001` backend or prerequisite unavailable
2. `UZ-SANDBOX-002` forced teardown / kill-switch fired
3. `UZ-SANDBOX-003` command blocked by policy
4. Correlate by `trace_id` first, then `run_id`

