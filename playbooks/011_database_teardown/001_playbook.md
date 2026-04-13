# Playbook 011: Database Teardown

## Purpose

Permanently delete all data from PlanetScale databases (DEV and/or PROD). This is a **destructive, irreversible operation** that removes all schemas and tables.

## When to Use

- Complete database reset before re-migration
- Cleaning up after testing in DEV
- Emergency data purge (PROD - extreme caution)

## Prerequisites

1. Docker available for psql container
2. 1Password CLI access (desktop app integration or `OP_SERVICE_ACCOUNT_TOKEN`)
3. Required environment approvals set

## Required Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `ALLOW_DATABASE_TEARDOWN` | `1` | Approve destructive database operation |
| `ENV` | `dev` / `prod` | Target environment (**must be explicit - no "all" allowed**) |

## Usage

### Check credentials only (dry run)

```bash
cd playbooks/011_database_teardown
ENV=dev ./01_credential_check.sh
```

### Teardown DEV

**Must run separately from PROD for safety:**

```bash
cd playbooks/011_database_teardown
ALLOW_DATABASE_TEARDOWN=1 ENV=dev ./00_gate.sh
```

### Teardown PROD

**Must run separately from DEV for safety:**

```bash
cd playbooks/011_database_teardown
ALLOW_DATABASE_TEARDOWN=1 ENV=prod ./00_gate.sh
```

### Teardown BOTH

**No "all" option - intentional safety measure. Run separately:**

```bash
# First, teardown DEV
ALLOW_DATABASE_TEARDOWN=1 ENV=dev ./00_gate.sh

# Then, separately, teardown PROD  
ALLOW_DATABASE_TEARDOWN=1 ENV=prod ./00_gate.sh
```

### Verify Teardown

Check what objects remain after teardown:

```bash
ENV=dev ./03_verify.sh
ENV=prod ./03_verify.sh
```

## Shared Teardown SQL

The `teardown.sql` file in this directory is used by:
- **This playbook** for PlanetScale teardown (via psql container)
- **`make _reset-test-db`** for local test database reset

This ensures consistency between local integration testing and production teardowns.

## Safety Mechanisms

1. **No "all" option**: Must explicitly target `ENV=dev` or `ENV=prod` separately - prevents accidental mass destruction
2. **Explicit approval required**: `ALLOW_DATABASE_TEARDOWN=1` must be set
3. **Typed confirmation**: Must type the environment name ("DEVELOPMENT" or "PRODUCTION") to proceed
4. **Credential verification**: Checks 1Password items exist before attempting connection
5. **Docker isolation**: Uses containerized psql - no local PostgreSQL installation required

## What Gets Dropped

The teardown drops these schemas and all their contents:

- `core` - tenants, workspaces, zombies, sessions, activity events
- `billing` - entitlements, billing state, credit state
- `agent` - agent-related tables
- `audit` - audit trails, ops access events
- `vault` - secrets, workspace skill secrets
- `ops_ro` - readonly ops schema
- `memory` - memory entries
- All tables in `public` schema

## Post-Teardown

After teardown, you must re-run database migrations to restore the schema:

```bash
# For local development
make migrate

# For deployed environments
# (Use the appropriate deployment playbook)
```

## Troubleshooting

### "MISSING APPROVAL"

Set the required environment variable:
```bash
export ALLOW_DATABASE_TEARDOWN=1
```

### Connection failures

- Verify PlanetScale credentials in 1Password vaults (ZMB_CD_DEV / ZMB_CD_PROD)
- Check network connectivity to PlanetScale hosts
- Ensure Docker daemon is running

## Security Notes

- Connection strings are read dynamically from 1Password - never hardcoded
- Passwords are passed via environment variables to containers (not command line)
- No credentials are logged or persisted to disk
