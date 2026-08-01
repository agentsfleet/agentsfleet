# Handoff — M154 schema rebuild (all three commits landed; database now proving the SQL)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## Scope / Status

Rebuilding `schema/` from empty while the dev database is undeployed. Two workstreams, one PR.

- ✅ **M154_001 + M154_002** specs committed, `docs/v2/active/`, `Status: IN_PROGRESS`
- ✅ **All 38 slots authored**; 45 old `schema/0*.sql` deleted; version == slot number
- ✅ **Everything is committed.** Five commits of real work; working tree clean.
- ✅ **The migration RUNS.** All 38 slots applied clean on the first attempt against local
  Docker Postgres; 32 tables exist. This was the milestone's largest unproven surface.
- ✅ **The budget apportionment is PROVEN** — 7/7 against a live database.
- 🔶 **The integration suite is the current front.** Last full run: **152 pass, 535 skip,
  76 fail, 8 leaks**. Every failure so far is a caller still on a retired column name, not a
  design fault in the schema.

## Commits on `feat/m154-schema-rebuild`

| Commit | Content |
|---|---|
| `aa7b5db3`, `c157c443`, `fa40300d` | specs + CHORE(open) (pre-existing) |
| `314d1a17` | schema rebuild + state layer |
| `3e130df6` | lease/claim layer — claim slot, lease row, reclaim's event join |
| `585464147` | budget apportionment tests + the floor-arm defect fix |
| `ef2974e4f` | the two fenced money statements |
| `77ee71867` | last production statements off the retired columns |

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

## Next Steps

1. **The identity-column sweep is the remaining bulk, and part of it is PRODUCTION:**
   - `http/handlers/fleets/sql.zig` — the main fleet-create statement has no `tenant_id`.
     Fleet creation fails outright. Prefer deriving it from the workspace row
     (`SELECT … FROM core.workspaces w WHERE w.id = $2::uuid`) so the caller is unchanged
     and the composite foreign key cannot be fed the wrong tenant — that is what
     `db/test_fixtures.zig` now does.
   - `http/handlers/common_authz.zig:37,122` — workspace authorization, `WHERE workspace_id`.
     This is behind the `common_authz_test` IDOR failure.
   - `events/fleet_set_cache.zig:317`, `http/handlers/admin/platform_keys/sql.zig:4`,
     `http/handlers/integration_grants/workspace.zig:191,205-207`.
2. **Then the test sweep** — ~34 files `INSERT INTO core.fleets` without `tenant_id`,
   ~13 touch `core.workspaces` identity columns. Mostly mechanical, but read each: some
   teardown helpers swallow errors, so a stale statement is invisible rather than red.
3. **`db/pool_test.zig:366`** — `schema_checks` still lists `ops_ro`, which slot 100
   deliberately no longer creates ("no schema is created that holds no tables"). Drop it and
   add `memory`, which is a first-class schema now.
4. **`events/fleet_set_cache_test.zig:110-125`** — its own inline seed carries all three
   identity-column bugs.
5. **§7.1 / §7.2 not started** — the events list must stop selecting `request_json` /
   `response_text`, and the single-event detail read must be added, with handlers, OpenAPI
   and the UI dialog. `state/fleet_events_store.zig`'s `EVENTS_SELECT` still selects bodies.
6. **8 leaks** reported by the last run — not yet triaged.
7. Then: changelog + `~/Projects/docs` pages, `docs/architecture/**` diff, CHORE(close).

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
