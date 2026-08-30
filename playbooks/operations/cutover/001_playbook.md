# Cutover — the Rust daemon

**Owners:** 🤠 Indy authorizes the swap and types the target; 🦉 Orly executes and verifies.
**Scope:** one environment per run. Staging rehearses; production follows a green rehearsal.
**Status:** SKELETON. This half carries the shape, the register and the rollback
rule. The swap milestone fills the rows marked `M181_002` and records evidence.

Every step below is a command, not a description, and every step carries a probe
tag. `playbooks/operations/cutover/probes.sh` runs the probes and refuses when a rubric row
of the merged milestones has none.

## Why the rollback is boring

No schema change, no data migration, no store change. The swap replaces one
binary with another against the same Postgres, the same Redis and the same
ledger. That is the whole rollback story, and it is a property this milestone
family protects rather than a plan it writes down: the absence of `schema/` from
Files Changed is what makes it true, and the SCHEMA GUARD is what keeps it true.

## Drain order

`M181_002` fills the per-machine sequence. The ORDER is fixed here because it is
the part a swap-day decision must not re-derive:

1. Remove the machine from the load balancer and wait for in-flight requests to
   finish. Draining before stopping is what turns a swap into a rolling deploy
   rather than an outage.
2. Stop the process. Supervised background tasks join on termination — that is
   graded by the swap milestone's `test_boot_supervises_otlp_export`, and a task
   that does not join is what leaves a lease half-written.
3. Start the replacement binary and wait for `/readyz`, not `/healthz`.
   `/healthz` answers for the process; `/readyz` answers for its dependencies,
   and a machine returned to the balancer on `/healthz` serves traffic it cannot
   complete.
4. Return the machine to the balancer. Only then move to the next one.

## Abort criteria

Abort is a decision made BEFORE the swap, so that swap-day judgment is a lookup
rather than an argument. `M181_002` sets the thresholds from a measured
baseline; the criteria themselves are fixed:

- Any parity differ against a route not in the register below.
- Error rate or p95 past the budgets `make bench-cutover` refuses to run
  without.
- A supervised task that does not join on termination.
- Any metric family that stops resolving on a dashboard.

## The one-move rollback

**The rollback path invokes no migration command.** Rollback serves an older
binary against a ledger it already understands; a migration there is at best a
no-op and at worst the single command that can refuse mid-incident, while the
incident is in progress. `probes.sh` asserts that absence mechanically rather
than trusting this paragraph — see `test_rollback_carries_no_migrate`.

The move itself is: drain, serve the previous image digest, wait for `/readyz`,
return to the balancer. The registry retains the digest, so rollback is a
deploy of something that already exists rather than a build.

## Declared-divergence register

A parity differ means one of two things, and the register is what separates
them: a difference listed here is DECLARED and expected; anything else is a
regression and aborts the swap.

| # | Divergence | Declared by | Why it is intended |
|---|---|---|---|
| D1 | The Rust lease handler carries no wire-version negotiation; the differ drives current-shape requests only. | M175_001 addendum A1 | There is one shipped wire shape and one implementation of it. Negotiation code with a single version to negotiate is a branch nothing takes. |
| D2 | `GET /metrics` is declared in `public/openapi.json` and is NOT served by the Rust daemon. | M181_001 §3 review — "no `/metrics` endpoint ever (push-only confirmed)" | The Rust daemon is a pure OTLP pusher. A pull endpoint would be a second export path to keep true, and nothing scrapes it: the string `metrics` appears in neither `deploy/fly/agentsfleetd-dev/fly.toml` nor `deploy/fly/agentsfleetd-prod/fly.toml`, so the scrape this endpoint would serve is not configured in any environment. |

**A register entry is not a licence to differ quietly.** Each row names the
document that declared it, and `make test-parity` reads this table so a declared
difference does not fail the lane while an undeclared one still does.

## Evidence

`M181_002` records the rehearsal and the swap here: the staging rollback
rehearsal, the soak numbers against the budgets, and the post-swap probe run.
