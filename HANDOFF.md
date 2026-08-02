# Handoff — M154 schema rebuild (full lane green; §8 half-built)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## ▶ READ THIS FIRST — the previous handoff was wrong about verification

**`make test-integration-db` is NOT the full suite.** It skips 452 of 763 tests.
The HTTP harness needs `REDIS_URL_API`; that target never sets it, so every
handler test returns `SkipZigTest`. The full lane is **`make test-integration`**,
which the spec's own rubric row S3 already names.

Running the real lane for the first time exposed **41 failures** in code the
subset had never executed — including the App ingress path the
`github-pr-reviewer` scenario runs through. All 41 are fixed.

**`zig build test-integration` cannot be trusted for pass/fail.** It prints
`failed command` and exits 0. In one observed run `make` printed
`✓ All integration tests passed` over a binary that produced **no test output at
all**. Always read the count from the binary itself:

```bash
make _reset-test-db                       # allowed; uses teardown.sql, not 02_teardown.sh
DATABASE_URL_MIGRATOR="postgres://agentsfleet:agentsfleet@localhost:25832/agentsfleetdb?sslmode=disable" \
  zig build run -- migrate
LIVE_DB=1 \
 TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25832/agentsfleetdb?sslmode=disable" \
 TEST_REDIS_TLS_URL="rediss://:agentsfleet@localhost:25833" \
 REDIS_URL_API="rediss://:agentsfleet@localhost:25833" \
 REDIS_TLS_CA_CERT_FILE="$PWD/.tmp/redis-ca.crt" \
 AGENTSFLEET_RUNNER_BIN="$PWD/zig-out/bin/agentsfleet-runner" \
 $(ls -t .tmp/zig-local-cache/o/*/agentsfleetd-integration-tests | head -1)
```

The QStash pair is omitted on purpose. `make/test-integration.mk` **derives**
`QSTASH_DEV_TOKEN_LOCAL` from `QSTASH_DEV_IDENTITY` / `QSTASH_DEV_SECRET` rather
than storing it, so pasting the derived value here would put a credential-shaped
literal in the repo — gitleaks blocks that, correctly. Export the pair from the
Makefile instead, which keeps the derivation where it belongs:

```bash
eval "$(make -s -n test-integration | grep -oE 'AGENTSFLEET_QSTASH_LIVE_[A-Z]+="[^"]*"' | head -2 | sed 's/^/export /')"
```

Only the QStash-backed tests need it; everything else runs without.

**Without the reset, results are garbage.** Skipping `_reset-test-db` produced 11
phantom failures from leftover fixture rows.

## Status

- ✅ **Full integration lane: 756 passed, 7 skipped, 0 failed.**
- ✅ **Everything committed. Tree clean. Branch NOT pushed, no Pull Request.**
- 🔶 **§8 is half-built** — see below.

## Commits this session (4)

| Commit | Content |
|---|---|
| `2e0f47dd1` | 9 SQL defects: 3 production, 6 fixtures. 217→227 on the subset lane |
| `6b6b8b92d` | Spec §8 folded into M154_001 (Indy: "not in a new spec but in this PR") |
| `6c9683ba7` | The full lane, 41→0. 1 production bug, 13 fixtures, 1 arm retired |
| `e46ce29fb` | §8 mechanisms: install seeds the grant, RESOLVE_GATE answers it |

## Production bugs fixed (4) — none were greppable

| File | Defect |
|---|---|
| `state/tenant_provider_resolver.zig` | `ORDER BY id` on a `provider`-keyed table |
| `state/account_teardown.zig` | `SELECT w.workspace_id` — `core.workspaces` keys on `id` |
| `state/fleet_telemetry_store.zig` | `recorded_at` in WHERE + both ORDER BYs; column is `created_at` |
| `http/handlers/workspaces/provision.zig` | conflict constraint name stale → duplicate workspace name answered **500 instead of 409** |

Every one had correctly-spelled columns that exist. Grep cannot find this class.

## The two tools that found them — USE THESE, do not grep

**`scripts/audit_sql.py`** — parses every SQL statement in the tree (resolving Zig `++`
const chains, which the old `audit_seeds.py` could not) and diffs it against the
LIVE catalogue. Checks qualified `alias.column` refs against the specific table
the alias binds, bare columns against the union of named tables, NOT NULL columns
an INSERT omits, and **INSERT column/value arity per row of a multi-row VALUES**.
Run `--all` to include tests. Currently: **1334 statements, 1 finding** (the
`fleetFromSession` decision below). It supersedes `audit_seeds.py` entirely.

It needs the live database (it reads `information_schema` through `docker exec`),
so it is a local audit tool beside the other `scripts/check_*` helpers, not a
Continuous Integration gate. Making it one would need a database in the gate job
— worth considering, given it found four production bugs.

**Constraint-name sweep** — the `provision.zig` bug was invisible to the SQL
auditor because it is a Zig string compared to a Postgres constraint name, never
parsed as SQL:

```bash
docker exec agentsfleet-m154-schema-rebuild-postgres-1 psql -U agentsfleet -d agentsfleetdb -Atc \
  "select conname from pg_constraint where connamespace::regnamespace::text in ('core','fleet','billing','memory','vault','audit','ops')" > /tmp/live.txt
grep -rhoE '"(uq|ck|fk|pk)_[a-z0-9_]+"' src --include="*.zig" | tr -d '"' | sort -u | \
  while read c; do grep -qx "$c" /tmp/live.txt || echo "MISSING: $c"; done
```
Expect exactly 3 benign hits: `ck_test_reclaim_fail`, `ck_test_release_fail`
(created at runtime by fault-injection tests) and `uq_workspaces_other` (a
deliberately-wrong name inside a unit test). Anything else is a real bug.

## The dominant fixture failure class — recognise it instantly

§7.3 moved the event body off the lease, so `reclaim.reclaimPriorActive` reads it
through an **INNER JOIN `core.fleet_events` ON (fleet_id, event_id)**. Any fixture
that seeds a lease without its event row reclaims nothing, and the test sees
"no lease" / "no work" / `NoActiveLeaseToReclaim`. Five helpers had this. If a
lease-related test fails that way, check for the event seed first.

The production join is deliberate — its comment says an event deleted under a
live lease *should* yield nothing. Fix fixtures, never the join.

## §8 — what is built and what is not

**Built (`e46ce29fb`), green, but UNPROVEN by tests of its own:**
- **8.1** `http/handlers/fleets/create_grants.zig` — install seeds a `pending`
  `core.integration_grants` row and raises the approvals-inbox gate for every
  mintable credential the bundle declares. Uses `secrets_resolve.mintableId`,
  the same classifier the lease path uses, so ask and enforcement cannot drift.
  Runs synchronously in `create.zig`, NOT in `create_install_steps`' progression
  (every step there is best-effort; a best-effort seed reproduces the bug).
- **8.2** `fleet_runtime/sql.zig` `RESOLVE_GATE` is now one statement: the gate
  flip and the grant move commit together. Non-approval outcomes drive the grant
  to `revoked`, not `pending` (which nothing would re-raise). `action_id` derives
  from (fleet, service) so re-install does not stack inbox duplicates.

**NOT built:**
- **8.3 lease-time park.** `fleet/service.zig` (grant gate, ~line 245) still
  drops an ungranted mintable silently. Parking needs an abort threaded through
  `resolveExecutionPolicy`'s return type and unwound at the caller — a refactor,
  not a line change. It is the safety net for chat/cron fleets; webhook fleets
  are covered by install-time seeding.
- **8.4 / 8.5 the deletion set**: `integration-requests` route,
  `authenticateFleet`, `fleetFromSession`/`S_SESSION`, `core.fleet_keys` + its
  management routes + the fleet-key half of `auth/api_key.zig`,
  `webhooks/grant_approval.zig` + the Redis grant nonce + `grant_notifier`.
- **Tests** `test_install_seeds_pending_grant_and_gate` and
  `test_gate_approval_arms_webhook_routing`. Neither exists.

**Why §8 exists** (do not relitigate — Indy decided, quotes are in the spec's
Discovery log): `core.integration_grants` is the enforcement spine for internal
credential minting, but the only statement that could CREATE a grant sat behind
the external fleet-key route. An internally-installed fleet could never obtain
one — the App ingress query inner-joins on `status='approved'`, so no event was
ever written and the fleet was silently inert. Origination moved to install.

## Open decisions — BLOCKING CHORE(close)

1. **`list_aggregate` coverage loss needs an Indy-acked verbatim quote.** Its
   orphan-ledger-row arm seeded a charge under a non-identifier to prove the
   aggregate ignored it. `usage_ledger.fleet_id` is now UUID with an FK to
   `fleets(id)`, so the driver refuses the value and the key would refuse a
   well-formed stranger. Removed with a comment; the invariant moved into the
   schema. **Asked twice, not yet answered.**
2. **`fleetFromSession`** — the last audit finding. Indy has the full analysis
   and approved deleting it as part of §8's deletion set, which is not done yet.

## Remaining work

1. **§8 completion** — 8.3, the deletions, and the two named tests.
2. **§7.1 / §7.2** — not started. `EVENTS_SELECT` in `state/fleet_events_store.zig`
   still selects `request_json` / `response_text` (rubric R4 greps for exactly
   this). The single-event detail read, its handler, OpenAPI entry and UI wiring
   do not exist. `EventDetailsDialog.tsx` ALREADY EXISTS and reads those fields
   off the list row — that is what §7.1 removes, so it must move to fetch-on-open.
   Spec interface: `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}`,
   404 for unknown OR cross-workspace (indistinguishable).
3. **Coverage → 80%.** Baseline `make test-coverage-zig` = **62.20%**, gate is 60
   (`ZIG_COVERAGE_MIN_LINES` in `make/test.mk:29`). **Fix the measurement first**:
   22,994 of 51,246 measured lines are `_test.zig` files, covered at 70.6%, which
   is what carries the number over its own gate. Production-only is **55.42%**.
   The coverage lane also runs only unit binaries, so integration-tested handlers
   read as 0%. Exclude test sources and merge the integration lane before writing
   a single test, or you will be chasing an artifact.
4. **Index review** — Indy's procedure: `make down` FIRST, fresh up + migrate so
   EXPLAIN reads a cold stack. Seed 10 runners / 100 fleets. An index that buys
   no sort or scan improvement gets DROPPED with the plan as evidence. First
   candidate: `idx_fleet_events_fleet_id_created_at_id`. **Also settle this**:
   `fleet.runner_events` carries 4 indexes plus the primary key, and the planner
   picks `idx_runner_events_type_created_at` (then filters `runner_id`) over the
   composite `index_usage_integration_test` expects.
5. **Skill chain** — pull origin into the branch FIRST (Indy's order), then
   `/write-unit-test`, `/write-integration-test`, gstack `/review`, changelog +
   `~/Projects/docs` pages + `docs/architecture/**` diff, then CHORE(close).

## Traps

- **Never rebuild a Zig line when scripting SQL edits.** Only substitute WITHIN a
  line. A whole-statement rewrite strips `\\` multiline markers. Evidence: four
  files carried a subquery spliced into a SET list from an earlier session's sed.
- **`git commit` runs the full gate suite and exceeds 2 minutes.** Background it
  and check `git log` — its exit code lies through a pipe. Three commits this
  session were blocked: MILESTONE-ID (a `M154_001` marker in a production doc
  comment — production code must be milestone-free), `zig fmt`, and an unused
  import. `make _zlint_check` is NOT enough; the hook runs `make lint-zig`.
- **Intermittent suite hang.** `TestHarness.start` blocks in `Thread.join` with
  the server thread stuck inside `httpz.listen`. Seen in
  `control_plane_policy_integration_test`, NOT `tenant_billing` as previously
  recorded. Diagnose with `sample <pid> 2 -mayDie`. **Kill the PARENT `build`
  process FIRST** — killing only the child lets the parent respawn it, and two
  suites against one database produce phantom failures.
- **The wire field is not the column.** `recorded_at`, `occurred_at`,
  `fleet_key_id`, `grant_id` survive in JSON/OpenAPI while the columns beneath
  them differ.
- **Metering cursors live on `fleet.runner_affinity`**, not the lease. The
  renewal probe INNER JOINs it; a lease without an affinity row renews as `lost`.
- **A lease's `created_at` must be recent.** The guard caps a run at
  `created_at + MAX_RUNTIME_MS`; an epoch-zero lease is born already expired.
- **Migrations install NO catalogue.** A test needing a priced model seeds its
  own `core.model_library` row; reading the 78 seeded rows makes a billing
  invariant depend on suite ordering.
- **Docker.** Compose project `agentsfleet-m154-schema-rebuild`, ports
  25832/25833/25834, database `agentsfleetdb`. Always `docker ps` first.
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — Indy
  calls it manually. `make _reset-test-db` (teardown.sql) is fine.

## Deliberately parked

- Repo-wide bind-arity comptime checker — its own spec, not M154.
- `http/handlers/workspaces/sql.zig` `INSERT_WORKSPACE` is a near-twin of
  `state/sql.zig`'s. NOT merged on purpose: that one ends `ON CONFLICT DO
  NOTHING`, the handler needs the unique violation to surface a name conflict.
- SQL centralisation: 195 statements housed in a `sql.zig`, 130 still inline
  across ~46 production files (≈60%). Indy asked; recommendation was to convert
  only what §8 touches and spec the rest as its own milestone.
