# Doctor

## Overview

The `doctor` command runs a suite of health checks to verify that a UseZombie component is correctly configured and can reach its dependencies. There are two modes: API mode and worker mode.

## API mode

```bash
zombied doctor
```

Checks run in API mode:

| Check | What it verifies | If it fails |
|-------|-----------------|-------------|
| PostgreSQL connection | Can connect and run a query against the database | Verify `DATABASE_URL_API` is correct. Check that PostgreSQL is running and reachable from this host. |
| Redis connection | Can connect and ping Redis | Verify `REDIS_URL_API` is correct. Check that Redis is running and reachable. |
| Clerk configuration | `CLERK_SECRET_KEY` is set and non-empty | Set the variable in your `.env` or environment. Obtain the key from the Clerk dashboard. |
| Required env vars | All required API variables are present | See [Environment variables](/operator/configuration/environment) for the full list. |
| Port availability | API port (default 3000) is not already bound | Stop the conflicting process or change `PORT`. |

## Worker mode

```bash
zombied doctor worker
```

Runs all API-mode checks plus additional worker-specific checks:

| Check | What it verifies | If it fails |
|-------|-----------------|-------------|
| Executor socket | `/run/zombie/executor.sock` exists and is connectable | Verify `zombied-executor.service` is running: `systemctl status zombied-executor`. |
| Executor health | Executor responds to `healthCheck` RPC | Restart the executor: `systemctl restart zombied-executor`. Check executor logs for startup errors. |
| Sandbox backend | `SANDBOX_BACKEND` is a recognized value (`host` or `bubblewrap`) | Correct the value in `.env`. |
| GitHub App key | GitHub App private key is accessible | Verify the key file path or 1Password reference is valid. Re-download from GitHub App settings if needed. |
| Landlock support | Kernel supports Landlock (Linux 5.13+) | Upgrade the kernel or set `SANDBOX_BACKEND=host` for dev use. |
| cgroups v2 | cgroups v2 is mounted and writable | Verify `/sys/fs/cgroup` is cgroups v2. Some older systems need `systemd.unified_cgroup_hierarchy=1` on the kernel command line. |

## Example output

```
$ zombied doctor worker
[OK]   PostgreSQL connection
[OK]   Redis connection
[OK]   Clerk configuration
[OK]   Required env vars (6/6)
[OK]   Executor socket (/run/zombie/executor.sock)
[OK]   Executor health (responded in 2ms)
[OK]   Sandbox backend (bubblewrap)
[OK]   GitHub App key
[OK]   Landlock support (ABI v3)
[OK]   cgroups v2

All checks passed.
```

```
$ zombied doctor worker
[OK]   PostgreSQL connection
[OK]   Redis connection
[FAIL] Clerk configuration: CLERK_SECRET_KEY is empty
[OK]   Required env vars (5/6 — missing: CLERK_SECRET_KEY)
[OK]   Executor socket (/run/zombie/executor.sock)
[FAIL] Executor health: connection refused
[OK]   Sandbox backend (bubblewrap)
[OK]   GitHub App key
[OK]   Landlock support (ABI v3)
[OK]   cgroups v2

2 checks failed. Fix the issues above and re-run.
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | One or more checks failed |

## Schema Gate (Operator Validation)

Use schema gate mode to verify binary/schema compatibility during local triage and deploy diagnostics:

```bash
zombied doctor --schema-gate --format json
```

Schema gate checks use the migrator connection (`DATABASE_URL_MIGRATOR`) and compare:
- expected migration count from binary (`common.canonicalMigrations()`)
- applied migration count from DB (`audit.schema_migrations`)

`schema_gate_compat.detail` includes:
- `expected_versions=<n>`
- `applied_versions=<n>`
- `reason_code=<SCHEMA_...>`

Reason codes:
- `SCHEMA_COMPATIBLE`
- `SCHEMA_BEHIND_BINARY`
- `SCHEMA_AHEAD_OF_BINARY`
- `SCHEMA_FAILED_MIGRATIONS`

Local triage flow:

```bash
# 1) Run gate in JSON mode
zombied doctor --schema-gate --format json

# 2) If behind, apply migrations
zombied migrate

# 3) Re-run gate and readiness
zombied doctor --schema-gate --format json
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```
