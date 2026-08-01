# Handoff — M154 schema rebuild (production SQL converted; test fixtures remain)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## Scope / Status

Rebuilding `schema/` from empty while the dev database is undeployed. Two workstreams, one PR.

- ✅ **M154_001 + M154_002** specs committed, `docs/v2/active/`, `Status: IN_PROGRESS`
- ✅ **All 38 slots authored**; 45 old `schema/0*.sql` deleted; version == slot number
- ✅ **Everything is committed.** Nine commits of real work; working tree clean.
- ✅ **The migration RUNS.** All 38 slots applied clean on the first attempt against local
  Docker Postgres; 32 tables exist. This was the milestone's largest unproven surface.
- ✅ **The budget apportionment is PROVEN** — 7/7 against a live database.
- ✅ **Every production statement is on the rebuilt columns** (see below).
- 🔶 **The integration suite is the current front**, and what is left in it is test
  fixtures. Latest full run: **175 pass, 535 skip, 53 fail** across 23 files (was
  152/76 before the production sweep). Every failure is a fixture on a retired column
  name, never a design fault in the schema.

## Commits on `feat/m154-schema-rebuild`

| Commit | Content |
|---|---|
| `aa7b5db3`, `c157c443`, `fa40300d` | specs + CHORE(open) (pre-existing) |
| `314d1a17` | schema rebuild + state layer |
| `3e130df6` | lease/claim layer — claim slot, lease row, reclaim's event join |
| `585464147` | budget apportionment tests + the floor-arm defect fix |
| `ef2974e4f` | the two fenced money statements |
| `77ee71867` | last inline production statements off the retired columns |
| `7c7bc8fa2` | fleet create + workspace lookups onto the identity columns |
| `0b07918ee` | the live-database failure taxonomy, recorded here |
| `dc60fd39b` | grants and gates: one identity column named `id`, no alias |
| `2b61aea0d` | the seven unconverted `sql.zig` modules, incl. signup bootstrap |
| `9c8f67fd8` | this document |

Branch not pushed. PR not opened. One PR for the milestone, at CHORE(close).

## Resolved decisions (do not reopen)

1. **Wallet privileges — composite role.** `SET ROLE` replaces rather than adds, so no
   per-table role can carry the fenced settle statement. `metering_runtime` (`schema/120`)
   is a member of `billing_runtime` plus direct grants on exactly the three `fleet` tables.
2. **Slot 890 STAYS, untouched.** Both trigger functions are already `SECURITY DEFINER`
   with pinned `search_path`, so elevation never affected them.
3. **Dimension 2.2 reworded** — a foreign key may reference the primary key, a superkey
   strictly containing it, or one allowlisted domain key.
4. **Budget attribution — option B (Indy's pick).** Apportion by window overlap.

## Proven against a live database

- **Migration**: 38/38 slots, first attempt, no manual repair.
- **Apportionment**: 7/7. Straddling the floor, entirely before, entirely after, point-span
  on the floor, run ending on the floor, span growing across renewals, and the front-loaded
  `evt-budget-multi` fixture. Handoff's predicted numbers confirmed exactly —
  `evt-budget-multi` is **113** (old model said 23) and `evt-budget-longrun` is **458**
  (old model said 500).
- **Privilege boundary**, queried directly: `api_runtime` has full write on the four `core`
  lifecycle tables; `billing.usage_ledger` SELECT-only; `billing.tenant_wallet` and
  `vault.secrets` unreachable at table level. The wallet being unreachable *proves* the
  `WITH INHERIT FALSE, SET TRUE` P0 fix — a bare membership grant would show `true` here.

## Defects found and fixed this session

- **The drain's floor arm was off by one comparison.** It tested `last_charged_at <= floor
  THEN 0`, but the row filter admits `>= floor` and an existing test asserts a charge stamped
  exactly on the month start counts. A one-shot charge on the boundary was silently dropped.
  Now `< floor`; a real span *ending* on the floor still contributes zero via the fraction
  arm, so nothing was lost.
- **`test_fixtures.zig` still used the retired identity columns** (`core.tenants.tenant_id`,
  `core.workspaces.workspace_id`). This blocked *every* database-backed test from seeding.
- **Account erasure could not complete** — purge deleted `core.tenants WHERE tenant_id`, and
  `WS_OF_TENANT` selected `workspace_id`. Both columns are `id` now.
- **Two explicit ledger DELETEs** (account purge, fleet delete) had to go: no role holds
  DELETE on the ledger by design, so both would fail closed.
- **51 MILESTONE-ID gate violations** blocked the very first commit — the previous agent had
  written `(M154_001 §7.3)`-style citations throughout the schema comments. Gate is
  user-override-only; all rewritten to describe purpose rather than lineage.
- **4 dead constants** left by the accrual-endpoint removal (zlint `unused-decls`).
- **`fleet/sql.zig` breached the 350-line cap**; the budget drain split to
  `sql_budget_drain.zig`, re-exported (the `sql_lease_row.zig` pattern).

## The budget apportionment (still the subtle part)

A run can last `MAX_RUNTIME_MS` = 12h against a **rolling 24h** window, and one ledger row
now accumulates a whole run. `billing.usage_ledger.last_charged_at` + `created_at` give the
run's span and both drain queries apportion the total by window overlap. `DRAIN_BACKOFF_MS`
is retired — the row filter is exact rather than a heuristic.

**Known approximation, visible in a fixture.** Apportioning assumes spend is spread evenly.
`evt-budget-multi` is deliberately front-loaded, so the old per-slice model counted 23 and
the new one counts 113. Real runs are near-uniform (time-based run fee, ~20s renewals), so
that fixture is the adversarial case, not the typical one. It is pinned deliberately, and
the test says so. If the error direction ever matters, the fix is storing the
run-fee/token-cost split so the time-proportional half apportions exactly.

## All production SQL is converted; the tail is test fixtures

Every statement in the 62 production files is on the rebuilt columns, verified by
grepping the full retired-name set. Two hits look like misses and are not: a local
variable named `uid` holding `principal.user_id` in `handlers/fleets/messages.zig`,
and `billing.tenant_wallet.tenant_id`, which genuinely is that table's key.

Seven `sql.zig` modules had never been touched and were the reason so much failed:
`cron`, `api_keys`, `state/model_library`, both connector modules,
`handlers/fleet`, and `state/sql.zig` — the last of which meant **account creation
could not complete**, since `core.tenants`, `core.users`, `core.memberships` and
`core.workspaces` were each addressed by their retired twin.

### The test-fixture sweep, and why it is not a `sed`

RULE SQLMOD exempts test fixtures from living in `sql.zig`, so 116 test files carry
inline SQL that no module change reaches. Roughly 350 edits across ~60 files:

| pattern | hits / files | rule |
|---|---|---|
| `\buid\b` | 148 / 46 | → `id`, **except** `runner_lifetime_counters` (parent-keyed on `runner_id`, has neither) and `runner_affinity` (keyed on `fleet_id`) |
| `request_json` | 59 / 41 | **drop only on `fleet.runner_leases`** — it is still a real column on `core.fleet_events`, which is where the body lives |
| `_at_ms` | 63 / 29 | `last_metered_at_ms` → `last_metered_at`; `created_at_ms`/`updated_at_ms` → unsuffixed |
| `fleet_execution_telemetry` | 41 / 17 | → `billing.usage_ledger`, and the insert must now supply `event_created_at` and `last_charged_at`, both NOT NULL |
| `metering_periods` | 31 / 12 | table is **gone** — delete the statement, do not rename it |
| `occurred_at` | 12 / 4 | drop from the column list AND drop the duplicated value; no bind renumbers |
| `core.tenants (tenant_id` | 14 / 9 | → `(id` |
| `core.workspaces (workspace_id` | 7 / 5 | → `(id` |
| `INSERT INTO core.fleets` | ~34 files | needs `tenant_id`; derive it from the workspace row so the composite foreign key cannot be fed a mismatched tenant |

Many of these sit in teardown helpers that swallow their errors, so a stale
statement is invisible rather than red — grep, do not rely on the suite going green.

## The remaining failures, counted

Read them from Postgres, not from Zig stack traces — the traces point at the seed helper,
the log points at the column:

```
docker compose logs postgres --since 45m 2>&1 \
  | grep -oE "ERROR:.*" | sed 's/ at character [0-9]*//' | sort | uniq -c | sort -rn
```

Latest run — **53 failures across 23 files**, all of them test fixtures:

| n | error | rule |
|---|---|---|
| 14 | `column "workspace_id" does not exist` | `core.workspaces.id` |
| 11 | `column "request_json" of relation "runner_leases"` | drop it; the body lives on `core.fleet_events` |
| 9 | `column "uid" of relation "model_library"` | → `id` |
| 9 | `column "tenant_id" does not exist` | `core.tenants.id` |
| 7 | `column "tenant_id" of relation "tenants"` | same, insert side |
| 7 | `column "id" of relation "runner_affinity"` | slot is keyed on `fleet_id`; drop the column and its bind |
| 5 | `column "uid" of relation "memory_entries"` | → `id` |
| 4 | `fleet.metering_periods does not exist` | delete the statement |
| 4 | `core.fleet_execution_telemetry does not exist` | → `billing.usage_ledger` |
| 4 | `null value in column "tenant_id" of relation "fleets"` | derive from the workspace row |
| 3 | `operator does not exist: uuid ~~ unknown` | a `LIKE` against a now-UUID column |
| 2 | `division by zero` | **not a bug** — `poisonTransaction`'s deliberate `SELECT 1/0` |

## Next Steps

1. **The test-fixture sweep above** is the whole remaining tail. Work it pattern by
   pattern, building between, and read each site — the table tells you which rule
   applies.
2. **`db/pool_test.zig:366`** — `schema_checks` still lists `ops_ro`, which slot 100
   deliberately no longer creates ("no schema is created that holds no tables"). Drop it and
   add `memory`, which is a first-class schema now.
3. **`events/fleet_set_cache_test.zig:110-125`** — its own inline seed carries all three
   identity-column bugs.
4. **§7.1 / §7.2 not started** — the events list must stop selecting `request_json` /
   `response_text`, and the single-event detail read must be added, with handlers, OpenAPI
   and the UI dialog. `state/fleet_events_store.zig`'s `EVENTS_SELECT` still selects bodies.
5. **8 leaks** reported by the last run — not yet triaged.
6. Then: changelog + `~/Projects/docs` pages, `docs/architecture/**` diff, CHORE(close).

## Risks / Gotchas

**Hard constraints from Indy:**
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — he calls it manually.
  (The `_reset-test-db` make target uses `teardown.sql` against this worktree's own Docker
  Postgres. That is the normal integration path and is not the same thing.)
- ✅ Prove against local Docker Postgres (this worktree's own stack) first.
- ✅ **Commit → migrate → test**, in that order. One PR for the milestone.

**Docker.** This worktree's stack is compose project `agentsfleet-m154-schema-rebuild` on
ports **25832/25833/25834**. A sibling stack `agentsfleet-playbooks-production-rebuild` runs
on 28739–41 — it is UP, it is someone else's, leave it alone. Always `docker ps` first.

**Traps:**
- **The wire field is not the column.** `occurred_at` (runner events) and `recorded_at`
  (charges) survive in JSON, OpenAPI and the client types while the columns beneath them are
  `created_at`. Do not sweep them in the row-mapping structs or the contract.
- **`PreflightContext` is shared with span emitters.** `event_created_at` is deliberately a
  *parameter* on `debitReceive`/`debitAndInsert`, not a struct field. Do not tidy it back.
- **The ledger's identity columns are UUID.** `row.get([]const u8, …)` on them returns raw
  binary with no error at compile time OR runtime — just garbage. Casts are `::text`.
- **`fleet.runner_lifetime_counters` is parent-keyed** on `runner_id` — no `uid`, no `id`.
- **The two fenced statements are deliberate near-twins.** Diff them against each other after
  any edit; every remaining difference should be one of exactly four: the runtime-cap check,
  `ext_lease` vs `claim`, the tally arm, and the `$` offsets.
- **Schema files are capped at 100 lines** and several sit exactly at it.
- **The pre-commit hook refuses a file with both staged and unstaged edits** — you cannot
  split one file across two commits.
- **Don't edit Zig sources while `make test-integration-db` is compiling.** The results
  become ambiguous and you will re-run 20 minutes. (Cost me one run.)

**Deliberately parked (not forgotten):**
- Repo-wide bind-arity comptime checker — its own spec, not M154.
- §3.3 erasure delete-order trim — needs the live erasure test to prove.
