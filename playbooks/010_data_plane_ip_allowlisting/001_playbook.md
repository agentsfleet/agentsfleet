# Data Plane IP Allowlisting

Legacy milestone/workstream ID: `M29_001`

Environment:

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
```

## Human vs Agent

1. Human approves vault reads for this session.
2. Agent runs gate section 1 (egress inventory and CIDR format).
3. Human fills missing vault fields.
4. Agent runs gate section 2 (provider target separation).
5. Human approves provider writes before any mutation script.

## Vault Read Approval

```bash
export ALLOW_VAULT_READS=1
```

Run full gate:

```bash
ALLOW_VAULT_READS=1 ./playbooks/010_data_plane_ip_allowlisting/001_gate.sh
```

Run sections:

```bash
ALLOW_VAULT_READS=1 SECTIONS=1 ./playbooks/010_data_plane_ip_allowlisting/001_gate.sh
ALLOW_VAULT_READS=1 SECTIONS=2 ./playbooks/010_data_plane_ip_allowlisting/001_gate.sh
```

## Required Vault Fields

DEV (`$VAULT_DEV`):

- `fly-egress-ips/cidrs`
- `fly-egress-ips/updated-at`
- `ovh-worker-egress-ips/cidrs`
- `ovh-worker-egress-ips/updated-at`
- `planetscale-dev/allowlist-org`
- `planetscale-dev/allowlist-project`
- `upstash-dev/db-id`

PROD (`$VAULT_PROD`):

- `fly-egress-ips/cidrs`
- `fly-egress-ips/updated-at`
- `ovh-worker-egress-ips/cidrs`
- `ovh-worker-egress-ips/updated-at`
- `planetscale-prod/allowlist-org`
- `planetscale-prod/allowlist-project`
- `upstash-prod/db-id`

## Provider Write Approval

```bash
export ALLOW_PROVIDER_WRITES=1
```

Do not apply provider allowlist mutations without this explicit approval.
