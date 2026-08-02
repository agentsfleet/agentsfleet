# Handoff — M154 schema rebuild (identity retired; 10 suite failures left)

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull Request.

## Scope / Status

Rebuilding `schema/` from empty while the dev database is undeployed. Two workstreams, one PR.

- ✅ **M154_001 + M154_002** specs in `docs/v2/active/`, `Status: IN_PROGRESS`
- ✅ **Migration runs 38/38 clean**; budget apportionment proven 7/7
- ✅ **`uid` is retired** — `grep -rn -w "uid" src/ schema/` → 0, the spec's criterion
- ✅ **`fleet.metering_periods` has no live reader** — only historical comments remain
- ✅ **Indy's two decisions applied** (see below)
- ✅ **Everything committed.** Tree clean. Branch NOT pushed, no PR.
- 🔶 **Integration suite: 27 → 10 failures, 218 passing.** Real numbers, re-run this session.

## Commits (20 ahead of origin/main, none pushed)

| Commit | Content |
|---|---|
| `…` … `f56686412` | prior sessions: specs, schema rebuild, state/lease/claim layers, budget apportionment, `uid` retirement |
| `cb4562f6f` | fleet_events identity + the last of metering_periods (24 files) |
| `a74d8ba09` | every statement onto the identity columns the tables have (55 files) |
| `5a11089a3` | the balance gate priced against a real model it seeds itself |

## Indy's decisions this session (both applied — do not relitigate)

1. **Settle-failure tests: DROPPED.** The fault seam is gone with `metering_periods`
   (it raised only because that table had no `ON CONFLICT`). Arm 4 of
   `test_unaccepted_report_never_captures_completion` and the whole
   `"a settle whose database write fails emits nothing"` test are deleted, each
   replaced by a comment saying why it cannot come back. **Coverage loss must be
   recorded in PR Session Notes.**
2. **Count identity → ledger SPAN.** `stageLedgerSpan` replaces `nonZeroDebitCount`;
   `renewal_metering_test` pins `created_at == t1` / `last_charged_at == t3` exactly
   (that test drives the clock, so the assertions are exact, not windowed).
3. **Priced test model:** Indy asked for a real Fireworks model. Used
   `accounts/fireworks/models/glm-5p2` at live rates — but the fixture **seeds the
   catalogue row itself**, because migrations install NO catalogue: those 78 rows
   exist only after `model_library_seed_integration_test` applies `seed.sql`.
   Reading them directly makes a billing invariant depend on suite ordering.

## ▶ START HERE — a production bug, already located

`src/agentsfleetd/state/tenant_provider_resolver.zig:108`

```
SELECT provider, source_workspace_id::text, model, base_url, context_cap_tokens
FROM core.platform_provider_defaults
WHERE active = true
ORDER BY updated_at DESC, id DESC     <-- no `id`; the table keys on `provider`
LIMIT 1
```

6 of the run's Postgres errors are this one statement. It is the **eighth**
production bug of this class in the milestone.

## The tool that finds these — use it, do not grep

Seven production bugs were invisible to grep because the columns were **DROPPED**:
the statements named *nothing*, not something stale. They were found by diffing
every statement against the live catalogue:

```
python3 /private/tmp/claude-501/-Users-kishore-Projects-agentsfleet/\
ccd73bdb-ac8b-48e0-9bef-07590f7c77f0/scratchpad/audit_seeds.py
```

Took the tree from 126 broken INSERTs to 1 (a false positive on `cron/sql.zig`,
whose column list is a `++` concatenation the parser splits). **It only covers
INSERTs** — `tenant_provider_resolver` above is a SELECT, which is why it survived.
Extending it to SELECT/UPDATE/DELETE predicates would likely find the rest of the 10.

Production bugs fixed so far: `workspaces/sql.zig` (create path, 2 statements) ·
`tenant_workspaces.zig` (list + paging, 4) · `secret_reference_txn.zig` (lock
protocol) · `secret_probe.zig` · `user_preferences/sql.zig` ·
`tenant_model_entries/sql.zig`.

## The 10 remaining failures

| Test | Likely cause |
|---|---|
| `index_usage_integration_test` — lifetime counter tally | `column "id" does not exist` (the resolver bug above) |
| `index_removal_integration_test` ×2 | index-name / shape pins vs the rebuilt slots |
| `liveness_sweeper_integration_test` ×2 | as above, plus lock timeout |
| `runner_counters_integration_test` ×4 | `error.WrongNumberOfParameters` — a `$N` bind gap |
| `memories_integration_test` keyset | index-registration pin |

Postgres error census for the run: 6× `column "id" does not exist`, 3× lock
timeout, 2× `t6_check_reject` / `t6_dup_reject` (deliberate fault fixtures — ignore),
2× division by zero (also deliberate).

## Verification commands

```
make test-integration-db                     # full DB suite (~8 min)
KEEP_TEST_STATE=1 TEST_FILTER='<name>' make test-integration-db   # one test, no reset
make _zlint_check                            # lint
zig build test-integration-bin && zig build test-bin   # compile both
```

Read failures from the **Postgres log**, not the Zig traces — traces point at the
seed helper, the log points at the column:

```
docker compose logs postgres --since 15m 2>&1 | grep -oE "ERROR:.*" \
  | sed 's/ at character [0-9]*//' | sort | uniq -c | sort -rn
```

To see the failing STATEMENT, not just the error:
```
docker compose logs postgres --since 15m 2>&1 | grep -A 6 'column "id" does not exist' | head
```

## Remaining work, in order

1. **The resolver bug above**, then re-run and re-triage.
2. **The other 9 failures.** Extend the audit script to predicates first.
3. **`db/pool_test.zig`** — already fixed (`ops_ro` → `memory`), no action.
4. **§7.1 / §7.2 not started.** `EVENTS_SELECT` in `state/fleet_events_store.zig`
   still selects `request_json` / `response_text`; the single-event detail read,
   its handler, OpenAPI entry and UI wiring do not exist. `EventDetailsDialog.tsx`
   ALREADY EXISTS and reads `row.request_json` / `row.response_text` off the list
   row — that is exactly what §7.1 removes, so it must move to fetching on open.
   Spec interface: `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}`,
   404 for unknown OR cross-workspace (indistinguishable).
5. **Memory leaks** — the last full run reported **0**. The handoff's old "8 leaks"
   figure is stale; re-verify before spending time on it.
6. **Indy's Fable table/index review** — see Risks below for the exact procedure.
7. Then: pull origin → `/write-unit-test` → `/write-integration-test` → gstack
   `/review` → changelog + `~/Projects/docs` pages + `docs/architecture/**` diff →
   CHORE(close).

## Risks / Gotchas

**Hard constraints from Indy:**
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — he calls it
  manually. (`_reset-test-db` against this worktree's Docker Postgres is fine.)
- ✅ Prove against local Docker Postgres first. Commit → migrate → test. One PR.
- 📌 **Indy's order for the index review:** `make down` FIRST, then bring the stack
  up fresh and migrate, so `EXPLAIN` numbers come off a cold stack rather than
  warm buffers. Seed 10 runners / 100 fleets. **An index that buys no sort or scan
  improvement at that scale gets DROPPED, with the plan output as evidence.**
  Use the Fable model. Overlaps spec §5 (5.1 named reader, 5.2 keyset without a
  Sort node, 5.3 memory composite). First candidate:
  `idx_fleet_events_fleet_id_created_at_event_id`, whose `(fleet_id, …)` prefix now
  duplicates the new `fleet_events_pkey (fleet_id, event_id)`.
- 📌 **Indy's order for the skill chain:** pull origin into the branch BEFORE running
  `/write-unit-test`, `/write-integration-test` and `/review`.

**Docker.** Compose project `agentsfleet-m154-schema-rebuild`, ports 25832/25833/25834,
database `agentsfleetdb` (not `agentsfleet`). Sibling
`agentsfleet-playbooks-production-rebuild` on 28739–41 is someone else's — leave it.
Always `docker ps` first.

**Traps:**
- **The wire field is not the column.** `occurred_at`, `recorded_at`, `fleet_key_id`
  and `grant_id` survive in JSON/OpenAPI while the columns beneath them differ.
  `runner_events.occurred_at` is now the COLUMN `created_at`, but the Zig parameter
  and wire field keep the old name deliberately.
- **`git commit` runs the full gate suite and exceeds 2 minutes.** Background it and
  READ THE LOG — a backgrounded `git commit | tail` reports the pipeline's status,
  not git's. Check `git log` afterwards, every time. Two commits this session were
  blocked by gates (MILESTONE-ID on a fixture literal; UFS on a duplicated
  `@import` path) — both mechanical.
- **Never rebuild a Zig line when scripting SQL edits.** Only substitute WITHIN a
  line. A whole-statement rewrite strips the `\\` multiline-literal markers and
  turns string content into source; two repair passes then over-corrected in
  opposite directions and the tree had to be reset (Indy approved the discard).
  The working scripts that respect this are in the scratchpad: `sweep.py`,
  `sweep_fleets.py`, `fix_predicates.py`.
- **`$N` gaps.** Dropping a column from a statement leaves a placeholder hole.
  One seed still bound `$0::uuid` (parameters are 1-based) — a leftover of the
  retired `id` on a table now parent-keyed on `fleet_id`.
- **Intermittent suite HANG** (2 of 6 runs). `TestHarness.start`
  (`test_harness.zig:295`) blocks in `Thread.join`: when a sibling test in the same
  file fails before `deinit`, the previous harness's `httpz` server thread never
  shuts down. Always in `tenant_billing_integration_test`. Diagnose with
  `sample <pid> 2 -mayDie`. It is a CONSEQUENCE of the failures, so it should clear
  as they do — but it turns one failure into a stalled suite and deserves its own
  fix eventually. Kill with `pkill -9 -f agentsfleetd-integration` (also kill the
  parent `build` process, or it respawns the child).
- **The ledger's identity columns are UUID.** `row.get([]const u8, …)` returns raw
  binary with no error. Casts are `::text`. Bare `$N` against a uuid column fails —
  the driver sends text.
- **`fleet.runner_lifetime_counters` is parent-keyed** on `runner_id`;
  **`runner_affinity`** on `fleet_id`; **`fleet_sessions`** on `fleet_id`;
  **`platform_provider_defaults`** on `provider`. None has an `id`.
- **The two fenced statements are deliberate near-twins.** Diff them after any edit;
  every remaining difference should be one of exactly four: the runtime-cap check,
  `ext_lease` vs `claim`, the tally arm, and the `$` offsets.
- **Schema files are capped at 100 lines**; several sit exactly at it.
- **The pre-commit hook refuses a file with both staged and unstaged edits.**
- Run `zlint` before staging — dropping a helper parameter orphans `const`
  declarations and fails the commit.
- Don't edit Zig sources while a build or `make test-integration-db` is in flight.

**Not renamed, deliberately — flag before anyone "finishes the job":**
`{grant_id}` as a path segment (it disambiguates against `{fleet_id}` in the same
template) and `fleet_key_id` as the public field name (schema/530 says it is
aliased at the boundary). Neither is a column.

**Cosmetic, not blocking:** `uid_value` locals, `BILL_UID` / `UID_GLM` test
constants. None trips the `-w uid` criterion. `uid` in `docs/v2/done/**` is
historical record — leave it.

**Deliberately parked (not forgotten):**
- Repo-wide bind-arity comptime checker — its own spec, not M154. (Would have caught
  the `$0` gap.)
- §3.3 erasure delete-order trim — needs the live erasure test to prove.
- `http/handlers/workspaces/sql.zig` `INSERT_WORKSPACE` is a near-twin of
  `state/sql.zig`'s. NOT merged on purpose: that one ends `ON CONFLICT DO NOTHING`,
  while the handler needs the unique violation to surface a name conflict.
