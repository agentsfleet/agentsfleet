# Verify the Development Installation

**Owner:** Human
**Executors:** Agent aggregates evidence and applies approved operations;
Pipeline produced the deployment evidence
**Prerequisites:** step 05 completed; `DEV_RUNNER_READY=true`;
`DEV_VERIFY_RUN_ID` is green

Do not start another deployment. This step verifies and records the workflow
started in step 05.

## Required result

The recorded workflow is green only when all of these pass:

- API readiness and dashboard smoke
- development runner deployment
- browser end-to-end acceptance
- CLI acceptance using the worktree binary

Then complete the development operational gates:

```bash
ENV=dev ACTION=apply \
  ALLOW_VAULT_READS=1 \
  ALLOW_PROVIDER_WRITES=1 \
  ./playbooks/operations/ip_allowlisting/00_gate.sh

ALLOW_VAULT_READS=1 \
  ALLOW_OBSERVABILITY_WRITES=1 \
  ./playbooks/operations/observability/00_gate.sh apply dev grafana

ALLOW_VAULT_READS=1 \
  ./playbooks/operations/observability/00_gate.sh verify dev grafana
```

The allowlisting playbook pauses for the Human's Upstash dashboard confirmation
because Upstash does not publish an API for editing or listing exact ranges.

Complete every live-acceptance section left open by the six provider
registration playbooks. Record provider-side identifiers, tenant/fleet
identifiers, and timestamps, but no credential values.

Record the workflow URL, every named job result, each operational gate result,
and provider-side non-secret identifiers. Do not begin production while any job
or operational gate is red. Continue to
`playbooks/founding/07_deploy_prod/001_playbook.md`.
