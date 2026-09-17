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
| 7 | Delete `upstash-dev` and `upstash-prod`, with an approver named | `vault-deletion upstash-dev` |

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
