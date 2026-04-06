# Data Plane IP Allowlisting

Legacy milestone/workstream ID: `M29_001`

> **Context:** This playbook hardens the data-plane network boundary for the v2 product direction.
> See `docs/v2/usezombie-v2.md` — credentials must never be visible to agent processes.
> IP allowlisting is the network-layer complement to the credential firewall.

---

## Environment

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
```

---

## Human vs Agent

| Step | Who | Action |
|------|-----|--------|
| 1 | **Human** | Approve vault reads: `export ALLOW_VAULT_READS=1` |
| 2 | **Agent** | Run section 1 — egress IP inventory and CIDR format |
| 3 | **Human** | Fill any missing vault fields reported by section 1 |
| 4 | **Agent** | Run section 2 — provider target separation (dev vs prod) |
| 5 | **Human** | Review output, then approve provider writes: `export ALLOW_PROVIDER_WRITES=1` |
| 6 | **Agent** | Run allowlist apply (once `scripts/allowlist-apply.sh` is implemented) |
| 7 | **Agent** | Run verify to confirm no drift |

---

## Gate Scripts

### Run full gate (sections 1 + 2)

```bash
ALLOW_VAULT_READS=1 ./playbooks/010_data_plane_ip_allowlisting/00_gate.sh
```

### Section 1 — Egress IP inventory and CIDR validation

```bash
ALLOW_VAULT_READS=1 ./playbooks/010_data_plane_ip_allowlisting/01_egress_inventory.sh
```

Checks:
- `fly-egress-ips/cidrs` — non-empty IPv4 CIDR JSON array
- `fly-egress-ips/updated-at` — present
- `ovh-worker-egress-ips/cidrs` — non-empty IPv4 CIDR JSON array
- `ovh-worker-egress-ips/updated-at` — present

Runs for both `$VAULT_DEV` and `$VAULT_PROD` by default. Scope with `ENV=dev` or `ENV=prod`.

### Section 2 — Provider target separation

```bash
ALLOW_VAULT_READS=1 ./playbooks/010_data_plane_ip_allowlisting/02_provider_targets.sh
```

Checks:
- PlanetScale `allowlist-org` and `allowlist-project` exist per env
- Upstash `db-id` exists per env
- Dev and prod targets are distinct (refuses cross-env writes)

---

## Required Vault Fields

### DEV (`$VAULT_DEV`)

| Item | Field |
|------|-------|
| `fly-egress-ips` | `cidrs`, `updated-at` |
| `ovh-worker-egress-ips` | `cidrs`, `updated-at` |
| `planetscale-dev` | `allowlist-org`, `allowlist-project` |
| `upstash-dev` | `db-id` |

### PROD (`$VAULT_PROD`)

| Item | Field |
|------|-------|
| `fly-egress-ips` | `cidrs`, `updated-at` |
| `ovh-worker-egress-ips` | `cidrs`, `updated-at` |
| `planetscale-prod` | `allowlist-org`, `allowlist-project` |
| `upstash-prod` | `db-id` |

---

## Provider Write Approval

Mutation scripts (`scripts/allowlist-apply.sh`) will not run without explicit approval:

```bash
export ALLOW_PROVIDER_WRITES=1
```

Never set this before reviewing section 1 and section 2 output.
