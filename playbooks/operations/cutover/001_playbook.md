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

## Swap-day environment preconditions

**Set these BEFORE the image flips, or the swap aborts at `/readyz`.** Both are
knobs the Zig daemon tolerated and the Rust daemon does not, which is why
neither shows up as a route difference and neither can live in the register
below: `scripts/parity_lane.sh` reads that table as `METHOD /path` rows, so an
environment knob has no row shape there. Both were found by booting the Rust
daemon from `docker-compose.yml` for the first time (M181_001 §4.3).

| Knob | What breaks | Why nobody noticed |
|---|---|---|
| `CLERK_API_BASE` | Preflight refuses boot. `rustd/crates/agentsfleetd/src/preflight/read.rs` reads it with `required`, and no Fly configuration sets it — not `deploy/fly/agentsfleetd-dev/fly.toml`, not `deploy/fly/agentsfleetd-prod/fly.toml`, and not the `flyctl secrets set` block in `.github/workflows/deploy-dev-fly.yml`. | `src/agentsfleetd/auth/clerk_backend_config.zig` carries the vendor root as a compiled-in `API_BASE` and returns it when the override is absent, so the Zig daemon has never needed the knob. Set it to that same vendor root. |
| `REDIS_URL_API` | Preflight refuses boot with `Invalid database number` if the URL carries any path segment. The Rust client reads the segment after the host as a database INDEX. | `src/agentsfleetd/queue/redis_config.zig` slices the URL at the first `/` and never reads past it, so a segment selected nothing and the Zig daemon always used db 0. Confirm the vault's Upstash entry has no path before the swap. |

Verify both against a machine's live configuration rather than against this
table — a knob added since it was written is exactly the case the table cannot
see.

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

## The collector hop in front of the export

**This step changes no binary.** A collector is deployed per environment and the
daemon's export endpoint is repointed at it by configuration. The collector holds
the vendor credential and owns the fan-out, so adding or moving a backend later
is collector configuration rather than a daemon redeploy.

**Order it against the daemon's exporter, not against a binary swap.** An earlier
draft of this section had the collectors going up under the Zig daemon first, so
that infrastructure change and binary change stayed separately attributable. That
premise is dead: the shipped `agentsfleetd` is already the Rust binary
(`Dockerfile:39`), and it exports nothing until its OTLP work lands. A collector
in front of a daemon that sends nothing proves nothing, so do this step after the
exporter exists, not before.

Run per environment, development first. Production follows a development run
whose panels stayed continuous.

1. Stand the collector up and give it the vendor credentials. The deploy
   workflow does this on its own (`Ensure the OTLP collector is running`), and
   a from-scratch stand-up can be forced ahead of a deploy:
   `flyctl deploy deploy/fly/otelcol-dev --app otelcol-dev --wait-timeout 60`.
   The positional path is the build context; without it the image's
   `COPY config.yml` fails.
2. Confirm the collector accepts all three signals BEFORE the daemon points at
   it. From a machine on the private network —
   `flyctl ssh console --app agentsfleetd-dev` — run the probe with
   `OTLP_COLLECTOR_URL=http://otelcol-dev.internal:4318` plus
   `OTLP_INGEST_USER` and `OTLP_INGEST_PASSWORD`. The collector's address
   resolves only inside Fly's 6PN network, so this cannot be run from a laptop
   and a probe that appears to pass from one is testing something else.

   **The receiver requires Basic auth**, so read a failure carefully: `401` is
   the gate working and the probe holding the wrong pair, while a connection
   refusal is the collector not serving. Those are different incidents and the
   revert below only addresses the second. The pair is the one the daemon
   already sends, so a 401 from the probe means the daemon would be refused too.
3. Repoint the daemon. `GRAFANA_OTLP_ENDPOINT` is a staged Fly secret, so it
   takes effect on the next deploy rather than immediately — the endpoint is
   the collector's address and the auth pair stays as it is. The collector
   holds its own copy of the vendor endpoint and forwards there.
4. Watch every dashboard panel across the deploy. The deliverable is that
   nothing changes: same families, same labels, same panels. A renamed or
   missing series is a failure of this step, not a property of it.

**Revert is one configuration edit.** Set `GRAFANA_OTLP_ENDPOINT` back to the
vendor endpoint the collector is forwarding to and redeploy; the daemon posts
direct again and the collector becomes an idle app. Nothing else moves — the
auth pair the daemon carries was never changed, which is what makes the revert
one line rather than a credential rotation. State this before making the change,
not after.

**That claim has one dependency, and it is a process one.** Revert is cheap only
while the daemon's dormant copy of the auth pair still works. After this change
the collector's copy is the live one, so a routine Grafana key rotation applied
to the collector alone leaves the daemon holding a stale credential — and the
revert that was one line becomes the credential rotation it promised to avoid,
discovered mid-incident. Rotate both, or drop the daemon's copy once the Rust
daemon (whose OTLP knobs do not require it) is the one serving.

**Abort criterion for this step:** any panel that stops resolving, or any signal
type whose series stop arriving while the other two continue. A partial
delivery is the failure mode worth naming separately — a collector can be
healthy as a process while one pipeline is misconfigured, and liveness alone
would read that as success.

## Evidence

`M181_002` records the rehearsal and the swap here: the staging rollback
rehearsal, the soak numbers against the budgets, and the post-swap probe run.

**The collector hop.** One row per environment, filled at
the change window:

| Environment | Collector deployed | Endpoint repointed | Panels continuous | Probe run | Notes |
|---|---|---|---|---|---|
| development | | | | | |
| production | | | | | |
