# Handoff — M154 schema rebuild (mid-EXECUTE, commit 1 of 3 done)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## Scope / Status

Rebuilding `schema/` from empty while the dev database is undeployed. Two workstreams, one PR.

- ✅ **M154_001 + M154_002** specs committed, `docs/v2/active/`, `Status: IN_PROGRESS`
- ✅ **All 38 slots authored** (37 + new `120_metering_role.sql`), gates clean
- ✅ **45 old `schema/0*.sql` deleted**; `embed.zig` + migration array rewritten, **version == slot number**
- ✅ **Both formerly-open decisions RESOLVED** (Indy, 2026-08-01 "go") — see below
- ✅ **§4 accrual endpoint removed end to end** (store, handler, routing, OpenAPI, manifest, text artifacts)
- ✅ **Commit 1 of 3 (state layer) complete** — wallet rename, receive writer, `event_created_at` threading
- ⏳ **Commits 2 and 3 not started** — lease/claim layer, then the two fenced money statements
- 🔴 **NOTHING has been committed.** All work above is uncommitted in the working tree.
- 🔴 **NO SQL has ever run against a database.** Every check so far is compile-time or text-level.

## Resolved decisions (do not reopen)

**1. Wallet privileges — build it in full via a composite role.** `SET ROLE` replaces rather than adds
privileges, so no per-table role can carry the fenced settle statement (it spans `fleet.*` +
`billing.*`). `metering_runtime` (`schema/120`) is a member of `billing_runtime` plus direct grants on
exactly the three `fleet` tables the statement touches. **The fenced statement is not modified.**
A second-model review (Fable) rejected all three originally-framed options.

**2. Slot 890 STAYS, untouched.** The proposal to delete the counter triggers rested on a false
premise — both functions are already `SECURITY DEFINER` with pinned `search_path` (890:31, 890:65),
so elevation never affected them. Inlining would be a *regression*: `schema/880` grants `api_runtime`
SELECT-only, and inline arms would need write grants on the counter table for every writing role.

**3. Dimension 2.2 reworded** — a foreign key may reference the primary key, a superkey strictly
containing it, or one allowlisted domain key. Twins still forbidden.

**4. Budget attribution — option B (Indy's pick).** See "The budget apportionment" below.

## Defects found and fixed while implementing

- **P0 — the privilege boundary did not exist.** `schema/110`'s membership grants were bare, so
  PostgreSQL applied them *ambiently*: every handler held vault + billing privileges with no
  `SET ROLE`. The comment above the grants asserted the opposite. Fixed with
  `WITH INHERIT FALSE, SET TRUE` on all three, guarded by `test_role_membership_is_dormant_until_set_role`
  (red-green proved).
- **P1 — `schema/710` never granted the ledger to `api_runtime`**, which would have answered four
  readers with `insufficient_privilege`. `api_runtime` now holds SELECT only.
- **P1 — §4's "no consumer" claim was false.** The budget drain read `fleet.metering_periods` for
  per-slice window attribution. Spec corrected; see below.
- **P1 — the drain's index was deleted out from under it.** `schema/720` claimed "no read filters by
  fleet any more" — true before the drain moved onto this table, false after. Replaced with
  `(fleet_id, workspace_id, last_charged_at)`; `fleet_id` LEADS so the fleet `SET NULL` still gets
  its prefix (a `workspace_id`-leading index could not serve it).
- **Vault §2's premise was wrong** — several vault reads span `vault` + `core` in ONE statement, so
  whole-transaction elevation is too coarse. `api_runtime` now holds a **column-scoped** SELECT on the
  six non-secret columns; the seven envelope columns stay unreachable (red-green proved).

## The budget apportionment (the subtle part — read before touching `fleet/sql.zig`)

A run can last `MAX_RUNTIME_MS` = **12h** against a **rolling 24h** budget window, and one ledger row
now accumulates a whole run. Stamping that total on one instant makes the daily check all-or-nothing,
which **under-enforces** (a long run's spend falls out of the window). `metering_periods` used to
solve this with per-slice timestamps — `budget.zig` documented that hazard on `DRAIN_BACKOFF_MS`.

Resolution: `billing.usage_ledger.last_charged_at` (new) + `created_at` give the run's span, and both
drain queries APPORTION the total by window overlap. `DRAIN_BACKOFF_MS` retired — the row filter is
now exact rather than a heuristic.

**Known approximation, and it is visible in a fixture.** Apportioning assumes spend is spread evenly
across a run. `budget_integration_test.zig`'s `evt-budget-multi` is deliberately front-loaded
(100 nanos at 25h ago, 20 at 20h, 3 at 13h): old model counts `20+3 = 23`, new model counts
`123 × 11/12 ≈ 113`. Real runs are near-uniform (time-based run fee, ~20s renewals), so this is the
adversarial case, not the typical one. **The existing budget test expectations still encode the OLD
semantics and must be recomputed against a live database — this was not doable by inspection.**
If the error direction ever matters, the fix is storing the run-fee/token-cost split so the
time-proportional half apportions exactly; deliberately not done.

## Working Tree

```
 40 added · 45 deleted · 64 modified · 1 renamed     (~150 paths)
```

- Branch: `feat/m154-schema-rebuild`, 3 commits ahead of `origin/main`, **not pushed**
- Those 3 commits are **specs + CHORE(open) only** — every line of code above is uncommitted
- PR: **not opened**. One PR for the whole milestone, at CHORE(close).

## Running Processes

- No tmux sessions.
- ⚠️ **The two sibling stacks are now DOWN** (only `buildx_buildkit_ci-zig-builder0` remains). The
  earlier warning about `agentsfleet-postgres-1` no longer applies — but re-check `docker ps` before
  migrating, and confirm any container you use carries an `m154` project prefix.

## Tests / Checks

- ✅ `make test-unit-agentsfleetd` — **2061 pass, 0 fail, 299 skip**
- ✅ `zig build -Dtarget=x86_64-linux` and `-Dtarget=aarch64-linux`
- ✅ `zig fmt --check`, `python3 lint-zig.py`, `gitleaks detect`
- ✅ `make check-openapi` — bundle + lint + **route coverage 77 routes, all documented**
- ✅ `audits/spec-template.sh --staged` — clean, 2 specs
- ✅ Every schema file ≤100 lines; `common.zig` 262 (split); `migration_policy_test.zig` 114
- 🔴 **`make test-integration`, `make memleak`, migration — NEVER RUN.** The 299 skips are the
  database-gated tests. Every piece of SQL in this milestone is unexecuted.

## Next Steps

1. **Commit 2 — lease/claim layer.** `CLAIM_AFFINITY_SLOT` (drop the removed `id` column + its bind),
   `RESET_AFFINITY_METERS` (drop `meter_slice_seq`, rename the cursor), `INSERT_LEASE_WITH_EVENT`
   (`sql_lease_row.zig` — drop `request_json`, cursor rename, counters to the parent-keyed shape),
   `reclaim.zig` (join `core.fleet_events` for the body per Dimension 7.3, tally stays in the same
   statement per 7.4).
2. **Write the apportionment + reconciliation tests RED**, before commit 3. Cases: run straddling the
   floor, entirely before, entirely after, point-span at the floor, and a span growing across
   renewals. None exist today.
3. **Commit 3 — the two fenced statements**, converted together. Target ledger arm shape:
   - `billing.usage_ledger`, column list `(id, tenant_id, workspace_id, fleet_id, event_id,
     charge_type, posture, model, credit_deducted_nanos, token_count_input,
     token_count_cached_input, token_count_output, wall_ms, event_created_at, created_at,
     last_charged_at)`
   - drop `uid` and the TEXT `'mtr_' || event_id` mint; drop the `::text` casts on workspace/fleet
   - `ON CONFLICT … DO UPDATE` gains `last_charged_at = GREATEST(existing, EXCLUDED.last_charged_at)`
     — a **clock-skewed replica must not drag the span backwards** and pull a live run out of the
     drain's filter. Must NOT set `event_created_at` (schema/710:45).
   - delete the `breakdown` CTE arm (metering_periods) and every `next_seq` / `meter_slice_seq`
     reference (probe select, guard compute, `ext_aff` SET) in both files
   - **`event_created_at` is NOT reachable in `guard` today** — add `l.event_created_at` to each
     probe select (`renewal.zig:161-163`, `renewal_settle.zig:64-65`); the `SELECT *` chain carries it
   - bind renumbering is trivial: renewal's breakdown uuid is the LAST param (pure deletion); settle
     needs one rename, `$18::boolean → $17::boolean`, twice, both in the tally arm
   - **diff the two SQL literals against each other afterwards** — they are deliberate near-twins, so
     any asymmetry beyond the known ones (cap check, `claim` vs `ext_lease`, tally arm, `$` offsets)
     is a bug findable without reading Zig
4. **Remaining stores/handlers**: `state/fleet_events_store.zig:49` (cost join),
   `http/handlers/fleets/delete.zig:143` (its explicit ledger DELETE should GO — the FK is
   `ON DELETE SET NULL` now), `state/sql.zig` wallet statements already renamed.
5. **Then**: commit → migrate → `make test-integration` under the real `metering_runtime` role.
   Expect findings; this is the first execution of any of it.
6. Handlers/OpenAPI/UI for §7 (events list drops bodies, detail read added), fixtures, remaining
   dimension tests, changelog + `~/Projects/docs` pages, architecture doc, CHORE(close).

**~135 files still carry old names** (`uid` 318 hits/92 files being the bulk). `uid` is also a local
variable name in places — do not blind-sweep it.

## Risks / Gotchas

**Hard constraints from Indy:**
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — he calls it manually.
- ✅ Prove against local Docker Postgres (this worktree's own stack) first.
- ✅ **Commit → migrate → test**, in that order. One PR for the milestone.

**Traps that cost time this session:**
- **`PreflightContext` is shared with span emitters.** `event_created_at` was deliberately made a
  *parameter* on `debitReceive`/`debitAndInsert`, NOT a struct field — `service_report`'s `Lease`
  does not carry the value and the span does not need it. Do not "tidy" it back into the struct.
- **The ledger's identity columns are UUID now.** `row.get([]const u8, …)` on them returns raw binary
  with no error at compile time OR runtime — just garbage in the charges JSON. Casts are `::text`.
- **`recorded_at` survives at the API boundary.** The column is `created_at`, but the charges
  endpoint's JSON field stays `recorded_at` (spec Interfaces: shape unchanged). `TelemetryRow` keeps
  the old field name on purpose.
- **`fleet.runner_lifetime_counters` is parent-keyed** on `runner_id` — it has no `uid` AND no `id`.
  A mechanical `uid → id` sweep produces `ON CONFLICT (id)` against a column that does not exist.
- **M157_001 (pending, other worktree)** was written against `schema/047_repair_proposals.sql`.
  Amended to `830` this session; `migration_policy_test.zig` now refuses any slot below 100.
- Schema files are capped at 100 lines and several sit exactly at it. Adding a column means trimming
  prose — check `wc -l` before assuming room.

**Deliberately parked (not forgotten):**
- Repo-wide bind-arity comptime checker — its own spec, not M154.
- §3.3 erasure delete-order trim — needs the live erasure test to prove.
- `token_count_cached_input` is now stored but no writer sets it yet except the receive path
  (which passes null); the fenced statements should populate it from `g.d_cached`.
