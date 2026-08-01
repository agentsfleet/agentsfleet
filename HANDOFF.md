# Handoff — M154 schema rebuild (identity retired; the slice-table tail remains)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## Scope / Status

Rebuilding `schema/` from empty while the dev database is undeployed. Two workstreams, one PR.

- ✅ **M154_001 + M154_002** specs committed, `docs/v2/active/`, `Status: IN_PROGRESS`
- ✅ **All 38 slots authored**; migration runs 38/38 clean; budget apportionment proven 7/7
- ✅ **The `uid` spelling is retired**, and the spec's acceptance criterion —
  `grep -rn -w "uid" src/ schema/` → 0 matches — **passes**.
- ✅ **Everything is committed.** Working tree clean. Branch not pushed, no PR.
- 🔶 **The integration suite has NOT been re-run since this session's edits.** The
  53-failure taxonomy in the previous handoff is now stale; treat the numbers below
  as unverified and re-run before trusting anything.

## Commits on `feat/m154-schema-rebuild`

| Commit | Content |
|---|---|
| `aa7b5db3` … `3165ca1ca` | (prior session) specs, schema rebuild, state/lease/claim layers, budget apportionment, production SQL conversion |
| `e5b3a9475` | retire `uid` everywhere including the wire — 115 files |
| (this session's second commit) | the model-library seed fixture + its generator, metering_periods teardowns, the last `uid` prose |

## What this session actually changed

### 1. Production statements the earlier grep could not see

The prior handoff claimed "every production statement is on the rebuilt columns."
That was verified by grepping the **retired names**, which cannot catch a statement
naming a column that was simply *dropped*. Five were live bugs:

- **`core.fleet_sessions`** — `UPSERT_FLEET_SESSION` still inserted an `id` and
  `checkpointFleetSession` minted a value for it. The table is parent-keyed on
  `fleet_id`. The `alloc` parameter went with the generator call.
- **`core.platform_provider_defaults`** — `UPSERT_ACTIVE_DEFAULT` inserted an `id`;
  the table keys on `provider`. `generatePlatformLlmKeyId` had no other caller and
  is gone.
- **`core.fleet_keys`** — `INSERT_FLEET_KEY`, `SELECT_FLEET_KEYS_FOR_WORKSPACE` and
  `DELETE_FLEET_KEY` all addressed the dropped `fleet_key_id` twin. The *public
  field name* `fleet_key_id` is unchanged; schema/530 says it is aliased at the
  boundary.
- **`core.api_keys` list** — every sort arm paged `ORDER BY created_at DESC, uid DESC`.
  A keyset over a column that does not exist.
- **`samples/fixtures/model-library/seed.sql`** — a committed, byte-stable artifact
  that `model_library_seed_integration_test.zig` applies to a live database.
  Seventy-seven statements on `uid` / `created_at_ms` / `updated_at_ms`. Its emitter
  (`scripts/seed-models.mjs`, `emit()` + `idFor()`) is corrected in the same commit —
  fix one without the other and the next `--emit-fixture-sql` regresses it.

**Lesson for the next sweep: grep the schema for what each table HAS, not the code
for what it used to be called.** `schema_truth.txt` is the shape to diff against:

```
docker exec agentsfleet-m154-schema-rebuild-postgres-1 psql -U agentsfleet -d agentsfleetdb \
  -Atc "select t||' :: '||cols from (select table_schema||'.'||table_name as t,
        string_agg(column_name, ', ' order by ordinal_position) as cols
        from information_schema.columns
        where table_schema not in ('pg_catalog','information_schema')
        group by 1) s order by t;"
```

### 2. `{uid}` → `{id}` on the wire

`/v1/admin/models/{uid}` is `/v1/admin/models/{id}`, and the response field follows,
across the Zig handler, `state/model_library_store.zig`, the split OpenAPI, the
bundled `public/openapi.json`, the route-coverage gate's own pin in
`scripts/check_openapi_route_coverage_test.py`, and the dashboard client + its three
vitest suites. `schema/400` already documented the route that way.

**Not renamed, deliberately** — flag if you disagree, do not "finish the job":
`{grant_id}` disambiguates against `{fleet_id}` in the same path template, and
`fleet_key_id` is the documented public field name. Neither is a column.

### 3. The parent-keyed slots

`fleet.runner_affinity` (keyed on `fleet_id`) and `fleet.runner_leases` (never carried
the body) had 15 seeds naming a phantom `id` and 24 naming `request_json`. Two lease
seeds left a `$N` hole once the body value went and were renumbered rather than just
trimmed — **if you write a script that drops a column, check for placeholder gaps
afterwards**:

```
python3 - <<'PY'   # gap detector, adapt the table name
import pathlib, re, subprocess
files = subprocess.run(["grep","-rl","INSERT INTO fleet.runner_leases","--include=*.zig","src/"],
                       capture_output=True,text=True).stdout.split()
STMT = re.compile(r"INSERT INTO fleet\.runner_leases.*?(?=\n\s*(?:, \.\{|;\n|\\\\ON CONFLICT))", re.S)
for f in files:
    for b in STMT.findall(pathlib.Path(f).read_text()):
        ns = sorted({int(x) for x in re.findall(r"\$(\d+)", b)})
        if ns and ns != list(range(1, max(ns)+1)):
            print(f, "missing", [i for i in range(1, max(ns)+1) if i not in ns])
PY
```

## ⚠️ Two tests need YOUR call before they can be finished

Both live in the `fleet.metering_periods` removal and **neither is mechanical**.
I left them untouched rather than guess. They are the only reason the sweep is not
finished.

### (a) The settle-failure fault seam is gone

`integration_roundtrip_test.zig` ("Database failure at settlement") and
`credit_metric_reconciliation_test.zig` ("a settle whose database write fails emits
nothing") both worked by occupying the `(event_id, slice_seq)` slot the settle was
about to write. `metering_periods` had **no `ON CONFLICT` clause**, so that raised —
a real database error at the settlement write with no fault-injection seam in
production code.

That seam no longer exists, and its absence is the point of the rebuild: the ledger
insert arbitrates `ON CONFLICT (event_id, charge_type) DO UPDATE`, the counters
upsert arbitrates its own primary key, and `fleet.runner_events`' unique index is
partial (`WHERE event_type = 'runner_offline'`). I checked all three.

The invariant is still real and worth keeping — *settle write fails → 500, no metric
sample, no completion capture*. Options:

1. **Revoke `INSERT ON billing.usage_ledger FROM metering_runtime` for the test,
   restore in `defer`.** Same code path, deterministic, reversible. Cost: grants are
   database-wide, so it is shared state — the class of thing the `cp.cleanupAll`
   comment says already burned this suite once. Safe only because tests run
   sequentially in one process.
2. **Drop both tests.** Honest about the coverage loss; the fail-closed paths keep
   only their unit coverage.
3. **A fault-injection seam in production code.** The original comment says there is
   none *by design*, so this reverses a deliberate decision.

`poisonTransaction` (`budget_integration_test.zig:462`) does **not** work here —
settle acquires its own pooled connection.

### (b) `nonZeroDebitCount` has lost its source of truth

`credit_metric_reconciliation_test.zig` asserts two identities: emitted **total** ==
committed total, and emitted **count** == non-zero committed debit rows. The value
identity survives (drop the `metering_periods` term from `committedDebitTotal`). The
**count** identity does not: one receive + two renewals + a settle used to be four
rows and is now two, because the stage row accumulates. Four samples, two rows.

A faithful replacement exists — assert the ledger row's **span** rather than a row
count, since each write advances `last_charged_at`:

```
created_at == t1  (first renewal created the row)
last_charged_at == t3  (the settle was the last write)
```

That is arguably stronger than counting rows: it pins exactly the span the budget
apportionment reads. `renewal_metering_test.zig`'s three `stage.slices` assertions
(lines ~200, ~223, ~288) want the same treatment. **But it changes what the test
proves, so it is your call, not mine.**

## Remaining work

1. **The two decisions above**, then finish `metering_periods`: `readSlice` in
   `service_token_splits_wire_test.zig`, the `slices`/`auditSum`/`token_cost_nanos`
   assertions in `concurrency_renew_test.zig`, the seed + assertion in
   `account_teardown_test.zig:150/164/237`, and `budget_gate_integration_test.zig`'s
   `teardownSpend` DELETE + its stale §47 comment.
2. **Re-run the suite.** Nothing has been run since these edits — only compiled.
3. **`db/pool_test.zig`** `schema_checks` still lists `ops_ro`, which slot 100
   deliberately no longer creates. Drop it, add `memory`.
4. **`events/fleet_set_cache_test.zig:110-125`** — its inline seed carries all three
   identity-column bugs.
5. **§7.1 / §7.2 not started** — `EVENTS_SELECT` in `state/fleet_events_store.zig`
   still selects `request_json` / `response_text`; the single-event detail read, its
   handler, OpenAPI and the UI dialog do not exist.
6. **8 leaks** from the last full run, untriaged.
7. Then changelog + `~/Projects/docs` pages + `docs/architecture/**` diff + CHORE(close).

## Risks / Gotchas

**Hard constraints from Indy:**
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — he calls it
  manually. (`_reset-test-db` against this worktree's own Docker Postgres is the
  normal integration path and is not the same thing.)
- ✅ Prove against local Docker Postgres first. Commit → migrate → test. One PR.

**Docker.** This worktree is compose project `agentsfleet-m154-schema-rebuild` on
ports **25832/25833/25834**. Sibling `agentsfleet-playbooks-production-rebuild` runs
on 28739–41 — someone else's, leave it. Always `docker ps` first. Note the database
is `agentsfleetdb`, not `agentsfleet`.

**Traps:**
- **The wire field is not the column.** `occurred_at`, `recorded_at`, `fleet_key_id`
  and `grant_id` survive in JSON/OpenAPI while the columns beneath them differ. Do
  not sweep them in row-mapping structs or the contract.
- **`git commit` runs the full gate suite and exceeds a 2-minute timeout — background
  it, and read the log rather than trusting the exit code.** A backgrounded
  `git commit … | tail` reports the *pipeline's* status: my first attempt returned 0
  while pre-commit had actually failed on `zig fmt` + zlint and nothing was committed.
  Check `git log` afterwards, every time.
- **Dropping a helper's parameter cascades.** Removing `session_id` from
  `seedFleetSession` orphaned nine `const SESSION_ID` declarations that zlint fails
  the commit on. Run `zlint` before staging.
- **Don't edit Zig sources while a build or `make test-integration-db` is running.**
- **The ledger's identity columns are UUID.** `row.get([]const u8, …)` returns raw
  binary with no error at compile time OR runtime. Casts are `::text`.
- **`fleet.runner_lifetime_counters` is parent-keyed** on `runner_id`; **`runner_affinity`**
  on `fleet_id`. Neither has an `id`.
- **The two fenced statements are deliberate near-twins.** Diff them after any edit;
  every remaining difference should be one of exactly four: the runtime-cap check,
  `ext_lease` vs `claim`, the tally arm, and the `$` offsets.
- **Schema files are capped at 100 lines**; several sit exactly at it.
- **The pre-commit hook refuses a file with both staged and unstaged edits.**

**Cosmetic, not blocking:** `uid_value` locals (26), `BILL_UID` / `UID_GLM` test
constants. None trips the `-w uid` criterion (`_` is a word character). `uid` in
`docs/v2/done/**` is historical record — leave it.

**Deliberately parked (not forgotten):**
- Repo-wide bind-arity comptime checker — its own spec, not M154. (Would have caught
  the `$N` gaps above.)
- §3.3 erasure delete-order trim — needs the live erasure test to prove.
