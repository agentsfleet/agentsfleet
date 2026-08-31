# Verify the Production Installation

**Owner:** Human
**Executors:** Agent starts and monitors the run; Pipeline deploys and verifies
**Prerequisites:** steps 07 and 08 completed; `PROD_RUNNER_READY=true`

Rerun the original tag workflow. Publishing and release creation are
idempotent; the rerun now executes the canary and approved fleet rollout.

```bash
gh run rerun "$PROD_RELEASE_RUN_ID" \
  --repo agentsfleet/agentsfleet
gh run watch "$PROD_RELEASE_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

The Human approves the `production-fleet` GitHub environment only after the canary
runner is green.

## Required result

- Production API and tunnel readiness pass.
- Canary deployment passes before the remaining runner fleet starts.
- Every configured runner is active after rollout.
- Post-release command-line interface (CLI) acceptance runs against
  `https://api.agentsfleet.net` using the exact package version in `VERSION`.
- The accepted package version is promoted to npm `latest`.
- The production dashboard is reachable at `https://app.agentsfleet.net`.

Complete the production operational gates:

```bash
ENV=prod ACTION=apply \
  ALLOW_VAULT_READS=1 \
  ALLOW_PROVIDER_WRITES=1 \
  ./playbooks/operations/ip_allowlisting/00_gate.sh

ALLOW_VAULT_READS=1 \
  ALLOW_OBSERVABILITY_WRITES=1 \
  ./playbooks/operations/observability/00_gate.sh apply prod grafana

ALLOW_VAULT_READS=1 \
  ./playbooks/operations/observability/00_gate.sh verify prod grafana
```

Confirm package promotion:

```bash
version="$(cat VERSION)"
test "$(npm view @agentsfleet/cli@latest version)" = "$version"
```

Complete every live-acceptance section left open by the six production provider
registration playbooks. Record provider-side identifiers, tenant/fleet
identifiers, and timestamps, but no credential values.

Production is ready to move only when this step, the provider gates, and both
public domains are green. A skipped job is not evidence.
