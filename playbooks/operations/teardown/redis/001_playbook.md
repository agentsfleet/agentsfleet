# Redis Teardown

**Owners:** 🤠 Indy authorizes and types the target; 🦉 Orly executes and
verifies.
**Scope:** exactly one of development or production per run.

This permanently executes Redis `FLUSHALL`. It removes every stored key,
including:

- `fleet:{fleet_id}:events` streams and their `fleet_lease` consumer groups
- the global `fleet:ready` readiness index
- the `connector:outbound` delivery stream
- authentication session, approval, and deduplication keys

The `fleet:{fleet_id}:activity` name is an ephemeral publish/subscribe
(Pub/Sub) channel, not a stored key.

## Before running

1. Stop traffic and every `agentsfleetd` machine in the selected environment.
   Otherwise live requests can recreate keys while verification is running.
2. Confirm Docker and 1Password access.
3. Confirm the selected vault item has both:
   - `api-url` for the restricted runtime connection
   - `url` for the root connection used only by this runbook

## Execute

Development:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_REDIS_TEARDOWN=1 \
ENV=dev \
  ./playbooks/operations/teardown/redis/00_gate.sh
```

Production:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_REDIS_TEARDOWN=1 \
ENV=prod \
  ./playbooks/operations/teardown/redis/00_gate.sh
```

The gate rejects `ENV=all` and prompts for the full environment name before
flushing. It forwards the Redis URL to the container by environment name, so
the credential does not appear in the process arguments.

## After the empty-cache check

Restart or redeploy every `agentsfleetd` machine before restoring traffic.
Startup recreates the shared `connector:outbound` group. Fleet event streams,
the `fleet_lease` group, readiness entries, sessions, and deduplication keys
are recreated by their normal write paths.

For a full platform rebuild, continue with founding step 04 for development or
step 07 for production.
