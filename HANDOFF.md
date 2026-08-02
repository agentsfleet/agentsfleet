# Handoff — M154 schema rebuild (merged with main; §8 code done, two tests owed)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull
Request (PR).

## ▶ READ THIS FIRST

**`make test-integration` is the full lane, not `make test-integration-db`** —
the latter skips ~450 tests because the Hypertext Transfer Protocol harness
needs `REDIS_URL_API` and that target never sets it.

**`zig build test-integration` cannot be trusted for pass/fail.** It prints
`failed command` and exits 0. `make` has printed `✓ All integration tests
passed` over a binary that produced no output at all. Always read the count off
the binary:

```bash
make _reset-test-db                       # allowed; teardown.sql, NOT 02_teardown.sh
DATABASE_URL_MIGRATOR="postgres://agentsfleet:agentsfleet@localhost:25832/agentsfleetdb?sslmode=disable" \
  zig build run -- migrate
eval "$(make -s -n test-integration | grep -oE 'AGENTSFLEET_QSTASH_LIVE_[A-Z]+="[^"]*"' | head -2 | sed 's/^/export /')"
zig build test-integration --summary none          # rebuild first — see the STALE BINARY trap
LIVE_DB=1 \
 TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25832/agentsfleetdb?sslmode=disable" \
 TEST_REDIS_TLS_URL="rediss://:agentsfleet@localhost:25833" \
 REDIS_URL_API="rediss://:agentsfleet@localhost:25833" \
 REDIS_TLS_CA_CERT_FILE="$PWD/.tmp/redis-ca.crt" \
 AGENTSFLEET_RUNNER_BIN="$PWD/zig-out/bin/agentsfleet-runner" \
 $(ls -t .zig-cache/o/*/agentsfleetd-integration-tests | head -1)
```

**🚨 STALE BINARY TRAP — this cost a full wrong verdict.** `make
test-integration` builds into `.tmp/zig-local-cache`; a bare `zig build`
builds into `.zig-cache`. Picking the newest binary from the wrong tree runs
code from hours ago. **Check the mtime before believing a result:**
`stat -f "%Sm %N" -t "%H:%M" $(ls -t .zig-cache/o/*/agentsfleetd-integration-tests | head -1)`

**Without `_reset-test-db`, results are garbage** — leftover fixture rows
produced 11 phantom failures once.

## Status

- ✅ **`origin/main` merged in** (`747975560`). Branch is **0 behind / 28 ahead**.
- ✅ **§8.3, §8.4, §8.5 code complete**, all lanes green.
- ✅ **Integration 774 / 7 skipped / 0 failed · Unit 2039 / 278 / 0 · CLI 1430 / 13 / 0**
- ✅ **SQL auditor: 1334 statements, 0 findings** (was 1 — `fleetFromSession` went with §8.4).
- ✅ Everything committed. Tree clean. **Branch NOT pushed, no PR.**
- 🔶 **§8.1 and §8.2 have no tests.** See "What is owed".

## Commits this session (2)

| Commit | Content |
|---|---|
| `747975560` | Merge `origin/main` — 29 commits, 8 conflicts, M156's per-tenant free trial reconciled with the schema rebuild |
| (§8 commit) | §8.3 lease park + §8.4/8.5 deletion set + docs + 3 tests |

## The merge, and what it hid

M156 moved five database-backed billing suites into the integration graph
carrying **pre-rebuild `core.model_library` column names** (`uid`,
`created_at_ms`, `updated_at_ms`). Five files, all SQL strings, invisible to the
compiler. Re-columned to `id` / `created_at` / `updated_at`.

**Two tests had never executed in either lane** — they matched neither the unit
lane's live-database requirement nor the integration lane's name filter. M156's
rename armed them and they failed instantly: the metering suite attributed
debits to a `fleet_id` of `"fleet-test"` against a Universally Unique Identifier
(UUID) column with a foreign key onto `core.fleets`, so every insert was
rejected. Real M154 defect, nothing was watching. Fixed by seeding a real fleet.

**Lesson worth keeping:** a test that matches no lane's filter is dead, and
nothing reports it. Worth a gate.

## What is owed — start here

1. **§8.1 test — `test_install_seeds_pending_grant_and_gate`.** NOT WRITTEN.
   `http/handlers/fleets/create_grants.zig` is built and green: install seeds a
   `pending` `core.integration_grants` row and raises the approvals-inbox gate
   for every mintable credential the bundle declares, synchronously in
   `create.zig` (NOT in `create_install_steps`' progression — every step there
   is best-effort, and a best-effort seed reproduces the bug §8 removes).
   Nothing asserts any of it.
2. **§8.2 test — `test_gate_approval_arms_webhook_routing`.** NOT WRITTEN.
   `fleet_runtime/sql.zig` `RESOLVE_GATE` is one statement: gate flip and grant
   move commit together; non-approval drives the grant to `revoked`, not
   `pending` (which nothing would re-raise); `action_id` derives from
   (fleet, service) so re-install does not stack inbox duplicates.
   Harness pattern: `http/handlers/fleets/get_integration_test.zig` is the
   compact one. Grant/gate fixtures: `fleet/control_plane_grant_integration_test.zig`.
3. **§7.1 / §7.2 — not started.** `EVENTS_SELECT` in `state/fleet_events_store.zig`
   still selects `request_json` / `response_text` (rubric R4 greps for exactly
   this). The single-event read, its handler, OpenAPI entry and User Interface
   (UI) wiring do not exist. `EventDetailsDialog.tsx` ALREADY EXISTS and reads
   those fields off the list row — that is what §7.1 removes, so it must move to
   fetch-on-open. Spec interface:
   `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}`,
   404 for unknown OR cross-workspace (indistinguishable).
4. **Coverage → 80%, gate updated.** Indy decided: `ZIG_COVERAGE_MIN_LINES`
   (`make/test.mk:29`) moves 60 → 80. **Fix the basis first, in this order:**
   1. Exclude `_test.zig` sources from the kcov denominator. They are 22,994 of
      51,246 measured lines and are themselves 70.6% covered — that is what
      lifts the merged figure to 62.20% over its own 60% gate. Production-only
      is **55.42%**. Without this, "80%" is reachable by adding test files.
   2. Merge the integration lane into the coverage run. `test-coverage-zig` runs
      only unit binaries, so integration-tested handlers read as 0% —
      `ingress/github.zig` shows 24.7% despite documented real coverage. kcov
      over the integration binary works; recipe is the direct-run block above
      plus `kcov --clean --include-pattern="$PWD/src"`.
   3. THEN rank by uncovered lines and write tests. Worst production files at
      last measure: `handlers/schedules/api.zig` (169, 0%), `cron/Store.zig`
      (169, 0%), `route_table_invoke.zig` (135), `auth/sessions.zig` (139, 5%),
      `cron/Service.zig` (130, 0%), `handlers/tenant_provider.zig` (125, 0%),
      `fleet/service_report.zig` (122, 0%).
   4. Raise the gate to 80 in the SAME commit as the tests that clear it.
5. **Index review** — Indy's procedure: `make down` FIRST, fresh up + migrate so
   EXPLAIN reads a cold stack. Seed 10 runners / 100 fleets. An index that buys
   no sort or scan improvement gets DROPPED with the plan as evidence. First
   candidate: `idx_fleet_events_fleet_id_created_at_id`. **Also settle this:**
   `fleet.runner_events` carries 4 indexes plus the primary key, and the planner
   picks `idx_runner_events_type_created_at` (then filters `runner_id`) over the
   composite `index_usage_integration_test` expects.
6. **Skill chain** — origin is already merged in, so start at `/write-unit-test`,
   then `/write-integration-test`, gstack `/review`, changelog + `~/Projects/docs`
   pages + `docs/architecture/**` diff, then CHORE(close).

## Decisions Indy made this session — do not relitigate

1. **Merge `origin/main` before picking a Section.** Done.
2. **CLI fleet-key surface deleted, Files Changed NOT amended.** Dimension 8.5
   requires `core.fleet_keys` be unreferenced tree-wide, and the CLI carried a
   full command surface the spec's blast-radius table never listed. Indy chose
   delete-without-amending. **The spec's Files Changed is therefore knowingly
   incomplete — record this in PR Session Notes at CHORE(close).**
3. **Every fleet-key mention deleted from `docs/AUTH.md`, including the v2.1
   first-class-principal roadmap item.** Also removed from
   `docs/architecture/roadmap.md` and the `README.md` pointer to it. The design
   intent that revamp recorded is gone from the tree by choice.

## Open decisions

**None blocking.** Both former questions are settled: `list_aggregate` coverage
loss is a recording (in the spec's Discovery log), not an approval; and
`fleetFromSession` was approved for deletion and is now deleted.

## Traps

- **🚨 STALE BINARY.** See the top. Check mtimes before believing any result.
- **The constraint-name sweep now expects FOUR benign hits, not three.**
  `ck_test_reclaim_fail`, `ck_test_release_fail`, `uq_workspaces_other`, **and
  `uq_workspaces_tenant_id_name`**. The fourth is a blind spot in the sweep, not
  a bug: it is a partial unique INDEX, and the sweep only queries
  `pg_constraint`, which does not carry bare indexes. Verified:
  `CREATE UNIQUE INDEX uq_workspaces_tenant_id_name ON core.workspaces (tenant_id, name) WHERE (name IS NOT NULL)`,
  `pg_constraint` count 0. Union it with `pg_indexes` to stop the false alarm.
- **Four doc comments name a constraint that does not exist** —
  `uq_workspaces_tenant_name`, one word off from the real
  `uq_workspaces_tenant_id_name`, and one still cites `schema/001`, a slot the
  rebuild renumbered: `state/signup_bootstrap_store.zig:102`,
  `state/heroku_names.zig:5`, `state/signup_bootstrap.zig:197`,
  `db/test_fixtures.zig:112`. Comments only. Fold into the next commit.
- **Never rebuild a Zig line when scripting SQL edits.** Substitute WITHIN a
  line only. A whole-statement rewrite strips `\\` multiline markers.
- **`git commit` runs the full gate suite and exceeds 2 minutes.** Background it
  and check `git log` — its exit code lies through a pipe. Blocked twice this
  session on `zig fmt` and once on an unused import ZLint caught.
  `make _zlint_check` is NOT enough; the hook runs `make lint-zig`.
- **The dominant fixture failure class:** §7.3 moved the event body off the
  lease, so `reclaim.reclaimPriorActive` reads it through an INNER JOIN
  `core.fleet_events ON (fleet_id, event_id)`. A fixture seeding a lease without
  its event row reclaims nothing — "no lease" / "no work" /
  `NoActiveLeaseToReclaim`. Fix fixtures, never the join; the join is deliberate.
- **Metering cursors live on `fleet.runner_affinity`**, not the lease. The
  renewal probe INNER JOINs it; a lease without an affinity row renews as `lost`.
- **A lease's `created_at` must be recent.** The guard caps a run at
  `created_at + MAX_RUNTIME_MS`; an epoch-zero lease is born already expired.
- **Migrations install NO catalogue.** A test needing a priced model seeds its
  own `core.model_library` row (columns are `id`, `created_at`, `updated_at` —
  NOT `uid`/`*_ms`; main's incoming helpers had the old names).
- **Intermittent suite hang.** `TestHarness.start` blocks in `Thread.join` with
  the server thread stuck in `httpz.listen`. Diagnose with
  `sample <pid> 2 -mayDie`. **Kill the PARENT `build` process FIRST** — killing
  the child lets the parent respawn it, and two suites against one database
  produce phantom failures.
- **The wire field is not the column.** `recorded_at`, `occurred_at`,
  `fleet_key_id`, `grant_id` survive in JSON/OpenAPI while the columns differ.
- **Two audit tools, use them — do not grep.** `scripts/audit_sql.py --all`
  (parses every statement, resolves Zig `++` const chains, diffs against the
  LIVE catalogue) and the constraint-name sweep. They found four production bugs
  grep could not, because every one had correctly-spelled columns that exist.
- **Docker.** Compose project `agentsfleet-m154-schema-rebuild`, ports
  25832/25833/25834, database `agentsfleetdb`. Always `docker ps` first.
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — Indy
  calls it manually. `make _reset-test-db` (teardown.sql) is fine.

## Deliberately parked

- Repo-wide bind-arity comptime checker — its own spec, not M154.
- `http/handlers/workspaces/sql.zig` `INSERT_WORKSPACE` is a near-twin of
  `state/sql.zig`'s. NOT merged on purpose: that one ends `ON CONFLICT DO
  NOTHING`, the handler needs the unique violation to surface a name conflict.
- SQL centralisation: 195 statements in a `sql.zig`, 130 still inline across ~46
  production files (≈60%). Recommendation was to convert only what §8 touches
  and spec the rest as its own milestone.
