# Dragonfly Teardown

**Owners:** 🤠 Indy authorizes and types the target; 🦉 Orly executes and
verifies.
**Scope:** exactly one of development or production per run.

The teardown sends `FLUSHALL` to each primary in the environment's Dragonfly cluster.
It then proves `DBSIZE` is 0 on every primary and replica.
It removes every stored key, including:

- the `connector:outbound` delivery stream
- authentication session, approval, and deduplication keys

Accepted inbound work remains in Postgres. Undelivered outbound obligations also remain there for the rebuild pass.
Activity and wake channels are ephemeral; flushing stored keys does not remove their durable history.

## Where it runs, and why

Each environment's cluster is one Fly machine running four member processes,
two primaries and a replica each (`scripts/dragonfly-cluster.sh`). They
announce the machine's private 6PN address, and the app has no public service
(`deploy/fly/dragonfly-{dev,prod}/`). A laptop cannot reach the members, so
the scripts run the work inside the machine over `flyctl ssh console`: the
same route the deploy pipelines use for the cluster's `healthy` check
(`scripts/dragonfly_cluster_ready.sh`).

`cluster_ops.sh` does the work there. It reads the members from the cluster's
own view (`CLUSTER SHARDS` at the seed) and refuses to act unless the view
names all four, each is online, and each answers within 5 seconds. That check
runs before any flush, so a partial cluster is never flushed. `fly_exec.sh`
carries the script into the machine inside the command. Nothing is read from
the vault: the cluster's password stays on the machine, and no connection
string reaches the laptop.

The fifth process in the machine is the TLS test node the integration lane's
trust proof uses. It is not a cluster member, no deployed client uses it, and
the teardown leaves it alone.

## Before running

1. Stop traffic and every `agentsfleetd` machine in the selected environment.
   Otherwise live requests recreate keys and verification fails on them.
2. Authenticate `flyctl`: `flyctl auth login`, or export `FLY_API_TOKEN` from
   the environment's `fly-api-token` vault item.

## Execute

Development:

```bash
ALLOW_DRAGONFLY_TEARDOWN=1 \
ENV=dev \
  ./playbooks/operations/teardown/dragonfly/00_gate.sh
```

Production:

```bash
ALLOW_DRAGONFLY_TEARDOWN=1 \
ENV=prod \
  ./playbooks/operations/teardown/dragonfly/00_gate.sh
```

The gate rejects `ENV=all`. Section 1 is read-only: it prints the four members
the cluster names, which also proves the ssh route works before anything is
destroyed. Section 2 prompts for the full environment name, then flushes each
primary. Section 3 prints every member's key count and passes only when all
are 0.

A refusal names the member: `the cluster names 3 member(s), not 4`,
`member <ip>:<port> (replica) is fail, not online`, or
`… did not answer PING: no answer within 5s`. Fix the cluster first, for
example by restarting the Dragonfly machine, then run the gate again. Do not
treat a refused run as a partial teardown: section 1 refuses before section 2
flushes anything.

## Proof

`verify_test.sh` (run by `make check-playbooks`) drives all three sections
against a stubbed cluster with no server. It covers the member listing,
flushing primaries only, refusing a partial view, an offline member or a silent
member before any flush, a replica still holding keys, and the dev and prod app
targeting.

On Oct 19, 2026, the full gate also ran against a real four-member
`dragonfly:v2.0.2` cluster started by `scripts/dragonfly-cluster.sh`, with
`flyctl ssh console` routed to `docker exec`. Before: 100 keys on each member.
After: `keys=0` on all four. With one replica frozen (`SIGSTOP`), section 1
refused with `did not answer PING: no answer within 5s` and the seeded keys
were left untouched.

The one link that has not run against a live Fly machine is `flyctl ssh
console --command` parsing the quoted `sh -c '…'` command. Section 1's
read-only listing is the first thing to run, so a wrong parse fails there,
before any flush.

## After the empty-cache check

Restart or redeploy every `agentsfleetd` machine before restoring traffic.
Startup recreates the shared `connector:outbound` group. Normal writes recreate authentication sessions and deduplication keys.

Lease polls continue reading the admissions ledger; outbound rebuild retries undelivered obligations.

To remove the cluster entirely rather than empty it, delete the environment's
Dragonfly Fly app; the next deploy recreates it from
`deploy/fly/dragonfly-{dev,prod}/` (`playbooks/founding/03_priming_infra/001_playbook.md`).
For a full platform rebuild, continue with founding step 04 for development or
step 07 for production.
