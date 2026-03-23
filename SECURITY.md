# Security Model

This is the short policy entrypoint for UseZombie.

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
6. Execution boundary: worker orchestrates, `sandbox-executor` executes

## Sandbox Rules

1. Worker calls a typed executor API instead of owning the entire agent runtime forever.
2. `sandbox-executor` embeds NullClaw and owns Linux sandbox enforcement.
3. If executor posture is unsafe, startup or run admission must fail closed.
4. If executor dies mid-stage, the run is retried or blocked from persisted stage state.
5. Active runs are not guaranteed to survive worker or executor upgrades.

## Detection

1. `UZ-SANDBOX-001` backend or prerequisite unavailable
2. `UZ-SANDBOX-002` forced teardown / kill-switch fired
3. `UZ-SANDBOX-003` command blocked by policy
4. Correlate by `trace_id` first, then `run_id`

## Detailed Guides

1. [Security Posture](docs/SECURITY.md)
2. [Postgres Security](docs/security/POSTGRES.md)
3. [Redis Security](docs/security/REDIS.md)
4. [Tailscale Security](docs/security/TAILSCALE.md)
5. [GitHub App Security](docs/security/GITHUB_APP.md)
6. [Clerk Security](docs/security/CLERK.md)

