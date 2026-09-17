# Datastore cutover — Upstash to Dragonfly

The store moves. That is the whole difference between this playbook and
`playbooks/operations/cutover/001_playbook.md`, which swaps a binary and keeps
its data where it was.

**Nothing is migrated.** No key, queue entry or stream is copied out of
Upstash. Both clusters start empty and the Upstash instances keep running,
untouched, until their vault items are deleted in step 7. "Cutover" here means
repointing a connection string.

## Owners

| Role | Who |
|---|---|
| Runs it | whoever is on the change |
| Approves the vault deletion | Indy, by name, in `DATASTORE_CUTOVER_APPROVED_BY` |

## Why the rollback is boring, and where it stops being boring

Steps 1 through 6 revert in a commit: a repointed workflow is a line in a
YAML file, and the old seed is still in the vault and still serving. Roll
back by reverting the commit and redeploying.

Step 7 does not revert. A deleted 1Password item does not come back, which is
why it is last, why it is gated on a named approver, and why it is not part of
the Pull Request that does everything else. Run it by hand, after both
deployments have booted against their own cluster and stayed up.

## Steps

Every step carries a `probe:` tag naming the check that decides it. A step
with no probe is not a step.

| # | Step | probe: |
|---|---|---|
| 1 | Create the dev cluster's vault item, `dragonfly-dev/api-url`, holding the 6PN seed `redis://:<password>@dragonfly-dev.internal:7001` | `seed-is-clean .github/workflows/deploy-dev-fly.yml` |
| 2 | Deploy `dragonfly-dev` — `ensure_fly_app.sh --create-only`, then `ensure_fly_app.sh dragonfly-dev . 1 deploy/fly/dragonfly-dev/fly.toml` | `cluster-ready dragonfly-dev` |
| 3 | Merge to `main` and let `deploy-dev.yml` run: build, Fly, metal, verify, then the three release gates | `cluster-ready dragonfly-dev` |
| 4 | Confirm the daemon's own preflight passed, not just that the socket answered | `cluster-ready dragonfly-dev` |
| 5 | Repeat 1 and 2 for prod, in the `agentsfleet-prod` organisation | `seed-is-clean .github/workflows/release.yml` |
| 6 | Sweep the tree: no workflow, script or fixture resolves the retired store | `zig-citations` |
| 7 | Delete the retired store's vault items, with an approver named. Dev is RETIRED; prod is OUTSTANDING and blocked — see below | `vault-deletion upstash-dev` |

## Step 7 — what has been retired, and what has not

Run by hand, item by item. This section is the record; a reader who wants to
know whether the door is shut reads it rather than guessing from the table.

### `upstash-dev` — RETIRED, Sep 17, 2026: 03:55 AM UTC

Approved by Kishore (Indy), in session, naming the item. Archived out of
`ZMB_CD_DEV` rather than hard-deleted, so 1Password's own archive holds it for
its retention window; nothing in the tree resolves it either way.

The evidence that made it eligible, each line the command that produced it:

- `flyctl status --app dragonfly-dev` — one machine `started`, `1 total, 1 passing`.
- `probes.sh cluster-ready dragonfly-dev` — exit 0, every node online.
- `probes.sh seed-is-clean .github/workflows/deploy-dev-fly.yml` — exit 0.
- `probes.sh seed-is-clean .github/workflows/release.yml` — exit 0.
- `git grep -rin upstash` — no live resolution left. Every remaining hit is
  QStash (a different product, still the cron trigger and NOT retired), the
  platform-ops fixture, or history.
- The daemon boots through `refuse_unsuitable_datastore`
  (`rustd/crates/agentsfleetd/src/serve/runtime.rs:73`), which issues
  `INFO CLUSTER` and refuses a seed answering as a single server. A rolling
  restart of `agentsfleetd-dev` on Sep 17, 2026: 03:54 AM UTC took both
  machines to `become healthy: 1/1`, so the running daemon passed that refusal
  against `DRAGONFLY_URL`. Fly's log buffer does not retain the boot lines —
  DEBUG lease-polling rolls them out inside a minute — so the health gate is
  the record, not a log grep.

`agentsfleetd-dev` also carried a `REDIS_URL_API` secret that no code read
(the only mention is a historical comment at
`rustd/crates/afd_dragonfly/tests/keys_and_config.rs:93`). Unset in the same
window; the rolling restart above is that change landing.

**Still owed, by a human, outside this repository:** the hosted Upstash Redis
database itself is untouched. Deleting its vault item removes the credential,
not the instance — and not its bill. Delete the database in the Upstash
console. Account access is what that needs, not the credential just archived.

### `upstash-prod` — OUTSTANDING, blocked

Not deleted, and not eligible. The invariant is that deletion follows a green
boot in BOTH environments, and production has never booted against a Dragonfly
cluster at all:

- `flyctl apps list` shows `agentsfleetd-prod` and `otelcol-prod` both `pending`.
- There is no `dragonfly-prod` app in the list.

Deleting it now would remove the only rollback for a cutover that has not
happened. `probes.sh vault-deletion upstash-prod` still refuses on an unset
approver, which is the gate doing its job.

**The precondition, in full:** steps 5 and 6 land for prod — `dragonfly-prod`
exists, `cluster-ready dragonfly-prod` exits 0, `agentsfleetd-prod` deploys and
serves against it, and `seed-is-clean .github/workflows/release.yml` still
passes. Then `upstash-prod` becomes eligible on the same evidence bar as dev,
and needs its own named approval. Not before.

## Abort criteria

Stop and revert if any of these is true.

- `cluster-ready` fails twice against a freshly deployed app. A node that
  listens but never bootstraps is a configuration fault, not a slow start, and
  a third attempt finds the same thing.
- The daemon logs `NotACluster`. The class is permanent; no retry clears it,
  and the seed is pointed somewhere wrong.
- A deploy leaves the previous release serving and the new one crash-looping.
  Roll the digest back — `The one-move rollback` in the binary-swap playbook
  still applies to the binary half.

## What this playbook does not cover

- Renaming the ten `redis_*` measurement names. Those move with the Grafana
  queries that read them, in their own change window. A repository-only rename
  breaks the dashboard silently, which is the M181 `LEASE_TTL_MS` failure
  exactly.
- Any PostgreSQL change. The cutover touches one store.
