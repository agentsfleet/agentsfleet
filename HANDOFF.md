# Handoff — M154 schema rebuild (§7 and §8 complete; coverage + indexes next)

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

**🚨 THE BRANCH IS PUSHED BUT NO CONTINUOUS INTEGRATION (CI) HAS EVER RUN ON
IT.** All eight workflows (`test`, `test-integration`, `lint`, `memleak`,
`cross-compile`, `gitleaks`, `dry`, `dry-smoke`) trigger on `pull_request`;
plain `push` only triggers on `main`. So 33 commits of schema rebuild have
never seen a Linux runner. Indy chose "push, no Pull Request yet" believing the
push would buy CI — it does not. **He has been told and the decision is open.**
Opening a draft Pull Request is the only way to get CI before CHORE(close).

## Status

- ✅ **`origin/main` merged in** (`747975560`). Branch is **0 behind / 33 ahead**.
- ✅ **§8 COMPLETE** — Dimensions 8.1–8.5 all have passing tests, all marked DONE.
- ✅ **§7 COMPLETE** — 7.1 and 7.2 landed this session; 7.3/7.4 were already done.
- ✅ **Integration 781 / 7 skipped / 0 failed · Unit 2043 · App 2160 across 213 files**
- ✅ **SQL auditor: 1334 statements, 0 findings.**
- ✅ **BRANCH IS PUSHED** (`d94529b52`). Tree clean. **Still no Pull Request.**
- 🔶 **Coverage basis + index review are what remain.** See "What is owed".

## Commits on this branch (recent)

| Commit | Content |
|---|---|
| `747975560` | Merge `origin/main` — 29 commits, 8 conflicts, M156's per-tenant free trial reconciled with the schema rebuild |
| `ec7ef86ed` | §8.3 lease park + §8.4/8.5 deletion set + docs + 3 tests |
| `27493ce74` | §8.1/§8.2 tests — origination and its answer, 4 tests, both mutation-checked |
| `cbd7a945b` | §7.1 + §7.2 — list stops reading bodies, single-event route serves them |
| `d94529b52` | Transcript re-reads its turns; Dimension 8.1 reworded to what it builds |

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

1. **Draft Pull Request — decide first.** See the CI warning at the top.
   Indy has the call; nothing else here is blocked on it.
2. **Coverage → 80%, gate updated.** Indy decided: `ZIG_COVERAGE_MIN_LINES`
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
3. **Index review** — Indy's procedure: `make down` FIRST, fresh up + migrate so
   EXPLAIN reads a cold stack. Seed 10 runners / 100 fleets. An index that buys
   no sort or scan improvement gets DROPPED with the plan as evidence. First
   candidate: `idx_fleet_events_fleet_id_created_at_id`. **Also settle this:**
   `fleet.runner_events` carries 4 indexes plus the primary key, and the planner
   picks `idx_runner_events_type_created_at` (then filters `runner_id`) over the
   composite `index_usage_integration_test` expects.
4. **Skill chain** — origin is already merged in, so start at `/write-unit-test`,
   then `/write-integration-test`, gstack `/review`, changelog + `~/Projects/docs`
   pages + `docs/architecture/**` diff, then CHORE(close).

## Decisions Indy made this session — do not relitigate

1. **Merge `origin/main` before picking a Section.** Done.
2. **CLI fleet-key surface deleted, Files Changed NOT amended.** Dimension 8.5
   requires `core.fleet_keys` be unreferenced tree-wide, and the CLI carried a
   full command surface the spec's blast-radius table never listed. Indy chose
   delete-without-amending. **The spec's Files Changed is therefore knowingly
   incomplete — record this in PR Session Notes at CHORE(close).**
3. **The events table's prose cell shows `No result recorded`.** Indy accepted
   the rendering change rather than keeping bodies on the list — asked and
   answered, verbatim: "yes `No results` is fine...". Clause B of Dimension 7.1
   ("the rendered table is unchanged") was amended out; it could not hold
   alongside clause A.
4. **The transcript re-reads its turns as details** (server-side, parallel,
   bounded by `CHAT_TURNS`), rather than degrading to headers only.
5. **Dimension 8.1's wording amended** rather than adding a bundle reason field.
6. **Every fleet-key mention deleted from `docs/AUTH.md`, including the v2.1
   first-class-principal roadmap item.** Also removed from
   `docs/architecture/roadmap.md` and the `README.md` pointer to it. The design
   intent that revamp recorded is gone from the tree by choice.

## §8 tests — what they cost to get right

`http/handlers/fleets/create_grants_integration_test.zig` (4 tests). Three
things bit, all fixture-shaped, and the next suite that installs a fleet
through the Hypertext Transfer Protocol (HTTP) will hit the same three:

- **The workspace id is not yours to choose.** `authorizeWorkspace` reads the
  workspace out of the JavaScript Object Notation Web Token (JWT) claims, so a
  private tenant/workspace UUID is a flat 403 on install, not an isolated
  fixture. Use the token fixture's own ids (`…6f01` / `…6f11`) like every other
  HTTP suite; isolate instead on the fleet ids (minted by the handler, purged
  per test) and a suite-private webhook repository.
- **Re-runs collide unless the fleet name carries a stamp.** A fleet name is
  unique per workspace and feeds the library row's `(workspace, content_hash)`
  unique — so a run that died before its purge 409s every run after it. The
  name is `{label}-{nowMillis}`.
- **`h.install_wg.wait()` is the sync primitive**, never a sleep. It also
  guarantees the row reached `active`, which the ingress read requires.
- **Gate rows are append-only**, so fixture cleanup opens a transaction and
  sets `approval_gate_db.SET_GATE_PURGE_BYPASS_SQL`. Neighbouring suites do a
  bare DELETE and swallow the raise — their gate rows just accumulate.

**Both dimensions were mutation-checked; keep the habit.** Disabling
`seedForInstall` in `create.zig` fails three of the four; appending `AND false`
to `RESOLVE_GATE`'s grant-arm predicate fails exactly the two resolution tests.
Iterate with `zig build test-integration
-Dtest-filter=create_grants_integration_test` — after a DB reset.

**Also settled:** `test_no_handler_local_authentication` is NOT dead despite
lacking the `integration:` name prefix. The lane passes two filters and Zig
ORs them, so the `_integration_test` FILE filter catches it. Verified running
at 354/785.

## §7 — what it cost, and what it left behind

**The section's premise was wrong and the spec now says so.** §7.1 claimed the
bodies were wanted only on expansion. Three rendered surfaces read them: the
events table's prose cell, the fleet header's outcome line, and the fleet
thread's transcript — and the transcript is the fleet page's DEFAULT view.
Indy's call was to drop the columns anyway; the table and strip now state the
outcome, and the transcript re-reads its turns as details.

- **The `left(...)` detour is dead — do not revive it.** 7.1's own acceptance
  text says "the plan reads no oversized-attribute storage", and a bounded
  prefix still reads it. Two hours went into arguing a design that failed the
  spec's own criterion. Read the acceptance text before proposing a shape.
- **🚨 The browser holds no dashboard credential.** The first cut of the dialog
  fetched client-side with `getToken()`, and
  `tests/grep-gates/no-api-template-mint.test.ts` failed it. Client components
  get server data through a **Server Action** (`lib/actions/with-token.ts`),
  never a browser fetch. That gate is load-bearing security architecture.
- **Never ask the Server-Sent Events (SSE) route a question in a test.**
  `/events/stream` never closes its connection; a test that requests it hangs
  the whole suite for as long as you let it. The route-shadowing assertion
  lives at `router.match()` in `handlers/fleets/event_detail.zig` instead.
- **`/events/stream` and `/events/{event_id}` are both six segments** and
  `event_id` is free-form TEXT, so nothing about its shape excludes the word
  `stream`. Only router ORDER keeps them apart — the stream matcher is tried
  first. `S_EVENTS` is spelled once in `route_matchers_fleet_leaf.zig` for
  exactly this reason.
- **`route_matchers.zig` hit the 350-line cap.** The per-fleet leaf matchers
  now live in `route_matchers_fleet_leaf.zig` and are re-exported, so call
  sites are unchanged.
- **The §7.1 test asserts the plan, not a proxy.**
  `events_payload_free_integration_test.zig` seeds 200 events each carrying a
  20 kB body on both sides and asserts three things: no body field, response
  under 256 kB, and `pg_statio_all_tables.toast_blks_*` for
  `core.fleet_events` moving by **exactly zero**. That last one is the
  assertion a prefix-selecting query would fail while passing the other two.

## ⚠️ Unresolved flake — app suite

The FIRST `git push` was blocked by the pre-push hook: `1 failed | 2159 passed`
in `ui/packages/app`. **It has never reproduced** — four subsequent full runs
came back 2160/2160 and the retry pushed clean. The failing test was not
captured. If CI goes red on the app lane once a Pull Request exists, this is
the likely cause and NOT a regression from the diff. Worth capturing next time
it fires (`bun run test` in `ui/packages/app`, keep the output).

## Open decisions

**Nothing blocks §7.** Two former questions are settled: `list_aggregate`
coverage loss is a recording (in the spec's Discovery log), not an approval;
and `fleetFromSession` was approved for deletion and is now deleted.

**Settled this session.** Dimension 8.1's "bundle's stated reason" — Indy chose
to amend the wording, done in `d94529b52`. A bundle-authored justification is
recorded in the spec as a FEATURE, not a gap; do not treat it as owed work.

**The one live question is the draft Pull Request** (see the top). Everything
else has a stated procedure.

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
