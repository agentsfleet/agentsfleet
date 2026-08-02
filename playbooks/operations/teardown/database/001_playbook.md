# Database Teardown

**Owners:** 🤠 Indy authorizes and types the target; 🦉 Orly executes and
verifies.
**Scope:** exactly one of development or production per run.

This permanently drops every user-created schema and every table in `public`
from the selected PlanetScale database. The gate reads the migrator connection
string from 1Password, runs `teardown.sql` in `postgres:18-alpine`, and verifies
the result.

## Before running

1. Stop traffic and every writer in the selected environment.
2. Confirm Docker and authenticated 1Password Command-Line Interface (CLI)
   access.
3. Confirm the selected vault item has `migrator-connection-string`.
4. Confirm the target with 🤠 Indy. Production approval is separate from
   development approval.

## Execute

Development:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_DATABASE_TEARDOWN=1 \
ENV=dev \
  ./playbooks/operations/teardown/database/00_gate.sh
```

Production:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_DATABASE_TEARDOWN=1 \
ENV=prod \
  ./playbooks/operations/teardown/database/00_gate.sh
```

The gate rejects `ENV=all`, checks credentials, prompts for the full
environment name, executes the teardown, and runs verification. It forwards
the database URL to the container by environment name, so the credential does
not appear in process arguments.

## What remains

`teardown.sql` discovers user schemas from the catalog instead of maintaining
a static schema list. It excludes PostgreSQL system schemas and
PlanetScale-managed schemas. Database-level application roles remain because
dropping schemas does not remove roles.

## After verification

Keep traffic stopped until the selected environment has been rebuilt and its
migrations are green. Continue with founding step 04 for development or step
07 for production. Do not use a production teardown as part of a routine
deployment.
