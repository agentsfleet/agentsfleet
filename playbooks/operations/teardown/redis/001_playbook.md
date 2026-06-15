# Playbook 015: Redis Cache Teardown

## Purpose

Permanently flush all keys from the Upstash Redis cache (DEV and/or PROD). This is a **destructive, irreversible operation** that removes every key, including the per-agent event streams (`agent:{agent_id}:events`) and their `agent_lease` consumer groups, via `FLUSHALL`.

Sibling to the database teardown (`operations/teardown/database`, PlanetScale Postgres). Run both when fully resetting an environment.

## When to Use

- Complete cache reset before re-priming
- Cleaning up after testing in DEV
- Emergency cache purge (PROD - extreme caution)

## Prerequisites

1. Docker available for the `redis:7-alpine` container
2. 1Password CLI access (desktop app integration or `OP_SERVICE_ACCOUNT_TOKEN`)
3. Required environment approvals set

## Required Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `ALLOW_REDIS_TEARDOWN` | `1` | Approve destructive cache operation |
| `ENV` | `dev` / `prod` | Target environment (**must be explicit - no "all" allowed**) |

## Credential Used

`FLUSHALL` requires full ACL privileges, so this playbook reads the **root** Upstash connection string, not the role-scoped one:

| Env | 1Password ref |
|-----|---------------|
| dev | `op://ZMB_CD_DEV/upstash-dev/url` |
| prod | `op://ZMB_CD_PROD/upstash-prod/url` |

The restricted `api-url` / `worker-url` roles reject `FLUSHALL`.

## Usage

### Check credentials only (dry run)

```bash
cd playbooks/operations/teardown/redis
ENV=dev ./01_credential_check.sh
```

### Teardown DEV

```bash
cd playbooks/operations/teardown/redis
ALLOW_REDIS_TEARDOWN=1 ENV=dev ./00_gate.sh
```

### Teardown PROD

```bash
cd playbooks/operations/teardown/redis
ALLOW_REDIS_TEARDOWN=1 ENV=prod ./00_gate.sh
```

### Teardown BOTH

**No "all" option - intentional safety measure. Run separately:**

```bash
ALLOW_REDIS_TEARDOWN=1 ENV=dev ./00_gate.sh
ALLOW_REDIS_TEARDOWN=1 ENV=prod ./00_gate.sh
```

### Verify Teardown

```bash
ENV=dev ./03_verify.sh
ENV=prod ./03_verify.sh
```

## Safety Mechanisms

1. **No "all" option**: Must explicitly target `ENV=dev` or `ENV=prod` separately - prevents accidental mass destruction
2. **Explicit approval required**: `ALLOW_REDIS_TEARDOWN=1` must be set
3. **Typed confirmation**: Must type the environment name ("DEVELOPMENT" or "PRODUCTION") to proceed
4. **Credential verification**: Checks the 1Password item exists before attempting connection
5. **Docker isolation**: Uses a containerized `redis-cli` - no local Redis installation required
6. **No credential leak**: The connection string is forwarded by env-name-only (`-e REDIS_URL`), so the password never appears in `ps aux`

## Post-Teardown

No manual re-priming is required. `FLUSHALL` removes the per-agent event streams and their `agent_lease` consumer groups, but agentsfleetd recreates each one on demand when a agent is created (`POST /v1/workspaces/{ws}/agents` → `ensureEventStream`, idempotent `XGROUP CREATE … MKSTREAM`). An empty cache self-heals on the first agent created after the flush — see `playbooks/founding/03_priming_infra/001_playbook.md` §3.2.

## Troubleshooting

### "MISSING APPROVAL"

```bash
export ALLOW_REDIS_TEARDOWN=1
```

### Connection failures

- Verify Upstash credentials in 1Password vaults (ZMB_CD_DEV / ZMB_CD_PROD)
- Check network connectivity to the Upstash host
- Ensure Docker daemon is running

## Security Notes

- Connection strings are read dynamically from 1Password - never hardcoded
- The URL is passed to the container by env-name-only (not on the command line)
- No credentials are logged or persisted to disk
