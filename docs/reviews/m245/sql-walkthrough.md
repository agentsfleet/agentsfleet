# M245 SQL walkthrough

Every SQL file M245 adds, edits or deletes against `main`, line by line, with
the reason for each change. Line numbers are the file's lines on
`feat/m245-chat-costs-twenty-statements`. A CTE is a common table expression,
one `WITH name AS (…)` block. Lines a file kept from `main` are named once and
not re-explained.

Every change serves one aim: a 10 s chat costs at most 20 Postgres statements,
and no cache serves a stale config, price or key
(`docs/v2/active/M245_001_P1_DOCS_INFRA_OBS_CHAT_COSTS_TWENTY_STATEMENTS.md`).

```
one chat                         the SQL that now does the step
claim ──────────────────────────► lease/claim_admission_head.sql   §4  one statement
gates ──────────────────────────► select_gates.sql                 §5  one read
issue ──────────────────────────► lease/insert_lease_with_event.sql §5 one statement
renew (every 5 s) ──────────────► select_renewals.sql + renew_batch.sql   §7  two per host per tick
                                  (meter_lease.sql: durable-task leases and the fallback only)
expiry ─────────────────────────► bill_expired_slice.sql           §6 numbering
report ─────────────────────────► commit_report.sql                §6  one statement
task notice (durable tasks only) ► apply_report_notice.sql          §6
```

| File | Change | Section |
|---|---|---|
| `schema/20261108121000_vault_secrets.sql` | edited in place | §2 |
| `schema/20261108124600_billing_usage_lines.sql` | edited in place | §11 |
| `afd_vault/src/sql/insert_secret_if_absent.sql` | edited | §2 |
| `afd_credential/src/provider/registered/sql/platform.sql` | edited | §3 |
| `afd_billing/src/sql/select_price_book.sql` | edited | §6 |
| `afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | edited | §4 |
| `afd_fleet/src/lease/sql/lease/stamp_delivery.sql` | deleted | §4 |
| `afd_fleet/src/lease/sql/select_gates.sql` | added | §5 |
| `afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | edited | §5 |
| `afd_fleet/src/lease/sql/commit_report.sql` | added | §6 |
| `afd_fleet/src/lease/sql/apply_report_notice.sql` | added | §6 |
| `afd_fleet/src/lease/sql/meter_lease.sql` | edited | §6 |
| `afd_fleet/src/lease/sql/bill_expired_slice.sql` | edited | §6 |
| `afd_fleet/src/lease/sql/insert_runner_event.sql` | deleted | §6 |
| `afd_fleet/src/lease/sql/select_run_tokens.sql` | deleted | §6 |
| `afd_fleet/src/lease/sql/original_request_owner.sql` | edited | §11 |
| `afd_billing/src/ledger/sql/insert_lines.sql` | edited | §11 |
| `afd_billing/src/ledger/sql/settle_lines.sql` | edited | §11 |
| `afd_billing/src/ledger/sql/select_unsettled.sql`, `select_unsettled_work.sql` | edited | §11 |
| `afd_billing/src/report/sql/charges.sql` | edited | §11 |
| `afd_fleet/src/task/decision/sql/sequence.sql` | edited | §11 |
| `afd_fleet/src/task/sql/bind_lease.sql` | edited | §11 |
| `afd_fleet/src/lease/sql/select_renewals.sql` | added | §7 |
| `afd_fleet/src/lease/sql/select_revision_documents.sql` | added | §7 |
| `afd_fleet/src/lease/sql/renew_batch.sql` | added | §7 |

Every statement file is prepared against the rebuilt schema by
`afd_db`'s `test_every_sql_file_prepares`, which passed on the §6 commit.

## Merge note: two schema files are edited in place

There is no production deploy yet, so a table change edits its own file
(`.orly/LOCAL.md`, SQL): `vault_secrets.sql` and `billing_usage_lines.sql`.
`runner_leases.sql` matches `main` again, since §11 removed the counter §6 had
added. Dev has already applied both files, and sqlx refuses
an applied file whose checksum changed (`afd_db/src/migration.rs:14-16`).
Deploying this branch to dev therefore needs the dev database reset that Indy
authorizes; issue #827 holds the merge order.

## §2 — `schema/20261108121000_vault_secrets.sql`

| Lines | Change | Why |
|---|---|---|
| comment above the table | added | States what the column is for and why it has no default. |
| `resource_version BIGINT NOT NULL` | added | The daemon now caches the decrypted platform key (blocker 1, accepted Nov 10). A cache key must change on every edit of the row, and `id` and `kek_version` survive a same-key-encryption-key replace. |
| no `DEFAULT` | deliberate | The repository keeps values out of schema defaults; the insert binds `afd_vault::SECRET_FIRST_VERSION` (1). |
| `vault.secrets_bump_resource_version()` | added | `NEW.resource_version := OLD.resource_version + 1; RETURN NEW;` |
| `trg_secrets_bump_resource_version`, `BEFORE UPDATE … FOR EACH ROW` | added | Fires on every update from every writer, so a writer added later cannot forget the bump and serve a stale key. |

## §11 — `schema/20261108124600_billing_usage_lines.sql`

Indy's change on Nov 10: every usage line says what it billed, instead of
the number range it falls in saying it.

| Lines | Change | Why |
|---|---|---|
| 9-15 | comment | Names the three kinds, their constant (`af_wire::usage::LineKind`) and how each numbers its lines. |
| 26 | `kind TEXT NOT NULL` | `model`, `runtime` or `decision`. No `CHECK` list and no default: the spellings are application constants (RULE STS), and every writer names its kind. |
| 51 | `UNIQUE (work_type, work_id, kind, seq)` | Each kind numbers its own lines. Before, one `u32` was split into three bands: the runner below `2^31`, runtime lines from `2^31` (`DAEMON_SEQ_BASE`), decisions from `3·2^30` (`DECISION_SEQ_BASE`). Each band was a hidden ceiling, and a row's meaning was read from which band its number sat in. |

The counter §6 added to `runner.leases` (`daemon_line_count`) is gone with it.
A runtime line is numbered by its slice's start, in milliseconds from the
lease's start, read from the lease row the writer locks. The cursor only moves
forward, so no two slices share a start, and two writers billing the same slice
write one line. The longest run fits a `u32` of milliseconds by the existing
compile-time assertion (`af_wire/src/usage/mod.rs`, `RUNTIME_MS_MAX`).

## §2 — `afd_vault/src/sql/insert_secret_if_absent.sql`

| Line | Change | Why |
|---|---|---|
| 1 | `resource_version` added to the column list | The column has no default, so the insert must name it. |
| 2 | `$15` added | Bound to `SECRET_FIRST_VERSION`. |

## §3 — `afd_credential/src/provider/registered/sql/platform.sql`

| Line | Change | Why |
|---|---|---|
| 4 | `s.resource_version AS secret_version` | Read inside `locked`, which takes `FOR SHARE OF p, s, m, r` (line 17), so the version returned is the version locked. |
| 33 | `l.resource_version AS catalogue_version, l.secret_id, l.secret_version` | The three facts are the decrypted key cache's key (`PlatformKeyKey::observed`). The entry is stored under the versions this locked read saw. |

## §6 — `afd_billing/src/sql/select_price_book.sql`

| Line | Change | Why |
|---|---|---|
| 5 | comma, then `(SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1) AS catalogue_version` | The price book carries the catalogue version it was read at (`PriceBook.catalogue_version`). The report statement refuses lines priced under a catalogue that has since moved (`commit_report.sql:62`). `c.id = 1` is the singleton's documented identity exception (`schema/20261108121300_model_catalogue_version.sql:1-3`). |

## §4 — `afd_fleet/src/lease/sql/lease/claim_admission_head.sql`

On `main`, the claim was followed by a second transaction, `record_received`.
It locked the admission, inserted the event (`afd_events`'
`insert_leased_fleet_event.sql`), stamped the delivery (`stamp_delivery.sql`),
read the counters after commit and read the redelivery state
(`lease/event.rs:118-175` on `main`). The claim now does all of it.

| Lines | Change | Why |
|---|---|---|
| 1-5 `fleet` | added | The fleet row, read once, only while its status is active (`$12`). Later blocks joined `fleet.fleets` on their own. |
| 6-11 `locked_slot`, `instant` | kept | `FOR UPDATE … SKIP LOCKED`: a competing claimant skips the slot rather than waiting. |
| 18 | `WHERE EXISTS (SELECT 1 FROM fleet)` added | No slot is created or advanced for a fleet that is not active. |
| 19-20 | parenthesised | The old `locked_slot` condition now sits under the fleet check. |
| 21-39 `slot` upsert | kept | The fence arithmetic. |
| 42 | `JOIN fleet f` | Reuses the one fleet read instead of `fleet.fleets`. |
| 66-68 `stamped` | `delivered_at = COALESCE(a.delivered_at, clock_timestamp())` | The body of the deleted `stamp_delivery.sql`, on the row this statement already locks and fences. |
| 80 | `RETURNING WITH (OLD AS prior)` | Postgres 18: returns the row as it was before this update. |
| 82 | also returns `producer`, `created_at`, `task_id` | `opened` and `installed` need them. |
| 83 | `prior.delivered_at IS NOT NULL AS redelivered` | The stamp now happens before the event insert, so the updated row always has `delivered_at`. Only the pre-update row says whether this is a redelivery. |
| 84-93 `prior_lease`, `bumped` | kept | |
| 94-104 `opened` | added | The event insert from `insert_leased_fleet_event.sql:1-10`. |
| 98 | `CASE WHEN a.producer = $14 AND NOT a.redelivered THEN a.created_at ELSE now() END` | The old `COALESCE((SELECT a.created_at … AND a.delivered_at IS NULL), now())`, rewritten on `redelivered`: the old subquery would now see the stamped row. |
| 102-103 | `ON CONFLICT (event_id) DO UPDATE SET resumes_event_id = COALESCE(…)` | Same arm as the old insert: a re-leased event keeps its row. |
| 104 | returns `(e.xmax = 0) AS inserted, e.created_at, e.status` | `status` is new; it replaces the `redelivery_of` read. |
| 105-109 `counters` | added | Stored `event_count`, plus 1 when this statement inserted: a CTE cannot see its own insert's trigger bump. |
| 110-120 `installed` | added | The revision the run uses (the task's accepted revision, else the head, line 117), the session, and the head's definition for comparison. |
| 121-122 `catalogue` | added | The catalogue version, for the price cache key. |
| 123-129 `platform` | added | The platform key's cache key, chosen as `platform.sql` chooses it. No locks: a stale key only misses the cache, and `platform.sql`'s locked read decides. |
| 130-137 `tally` | kept | |
| 139, 144-151 | final row | Returns `redelivered` and every fact above. |
| 152-158 | joins | `CROSS JOIN fleet`: no active fleet, no row. `LEFT JOIN platform`: a deployment may have no platform default. |

## §4 — `afd_fleet/src/lease/sql/lease/stamp_delivery.sql` (deleted)

Two lines: `UPDATE fleet.admissions SET delivered_at = COALESCE(delivered_at,
clock_timestamp()) … WHERE … lease_fence = $3 AND settled_at IS NULL`. The
claim's `stamped` block does the same update and sets the fence itself, and its
caller `record_received` is gone.

## §5 — `afd_fleet/src/lease/sql/select_gates.sql` (added)

The four facts the gates judge before an issue, in one read. They were four
reads: the spend, the event's recorded gate, its standing, and the approved
integrations, which delivery read after the issue.

| Lines | Block | What and why |
|---|---|---|
| 1-7 | header | What the read returns, and why floors are cut to milliseconds. |
| 8-10 | `floors` | Day floor `now() - $3`, cut to the millisecond the `TIMESTAMPTZ(3)` columns hold, so a line stamped exactly at the floor counts. Month floor in UTC. |
| 11-18 | `spend` | Day and month sums in one scan, bounded below by the earlier floor (line 18). The same windows as `afd_billing`'s `select_fleet_spend.sql`. |
| 19-24 | `recorded` | The newest gate this event holds. |
| 25-31 | `standing` | That action's newest row: its status and whether its deadline passed, on the database clock. |
| 32-39 | final row | Both sums, the gate, its standing, and the services the fleet holds a standing grant for, sorted so the answer is deterministic. |

## §5 — `afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql`

| Lines | Change | Why |
|---|---|---|
| 1-4 | header | The caller holds the host row; this snapshot is taken after that lock, so the census sees every earlier claimant's commit. |
| 5-7 `instant` | added | One `clock_timestamp()` for every comparison and stamp. It replaces the Rust-bound `$11`, so every later parameter shifted down by one. |
| 8-17 `host` | added | The capacity census inside the insert: the runner's live leases plus live authoring attempts, below `worker_count`. It was its own statement under the same host lock. |
| 21-25 `held` | rewritten guard | The slot's fence is `$10`; its deadline has not passed on the database clock; `$19`, the deadline the claim answered, has not passed (a pass that outlived its claim writes no lease); and the host has room. |
| 32-35 `inserted` | `$15`, `i.checked_at` | The lease is stamped with the same instant its checks used. |
| 44-49 `chosen` | added | Compare-and-swap on `runner.runners.lease_version = $17`, the value read under the host lock, recording the placement choice `$18` only if a lease was inserted. |
| 50-65 `audit`, `tallied` | renumbered | |
| 66-69 | final row | No row unless `chosen` and `tallied` wrote; the caller then rolls the lease back with it. |

## §6 — `afd_fleet/src/lease/sql/commit_report.sql` (added)

On `main` the report's transaction ran about a dozen statements under the host
lock: meter, settle, close the event, drop other fences' tool calls,
checkpoint, release the slot, read the destination, owe the answer, read the
run's tokens, write the audit row (`lease/commit.rs` on `main`). This statement
does all of it under the lease's own row lock, in the scope, slot, lease order
a renewal takes, and no host lock.

| Lines | Block | What and why |
|---|---|---|
| 1-32 | header | Lock order; why a CTE cannot see its own writes or the triggers'; the guard; the `$1-$49` map. |
| 33-38 | `scope` | `identity.lock_workspace` (`FOR KEY SHARE` on the workspace, `schema/20261108120300_identity_workspaces.sql:44`), then `FOR KEY SHARE` on the fleet. The first lock every writer of the fleet's scope takes, so lock order matches the claim and the renewal. |
| 39-45 | `locked_slot` | `FOR UPDATE` on the slot, only for an active lease of this runner. |
| 46-53 | `probe` | `FOR UPDATE` on the lease, reading its `created_at` and cursor. Under READ COMMITTED a waiter re-reads the latest row, so the cursor is current. |
| 54-55 | `catalogue` | The current catalogue version. |
| 56-66 | `guard` | Returns a row only when: the cursor never moves backwards or past the deadline (57); the lease was not reclaimed (60); posture, provider and model match the priced lines (61); the catalogue matches, with `$14` `NULL` for a self-managed lease priced from no catalogue (62, `usage/mod.rs:108-115`); an unsettled admission carries this fence (63-66). No row means no write below. |
| 67-72 | `measured` | Whole milliseconds since the cursor, capped at `$49` (`u32::MAX`), the rater's cap. |
| 73-81 | `runtime` | The runtime line, only for positive milliseconds. `seq` is the slice's start, milliseconds from the lease's start (76). The charge is the ceiling of `d_ms × nanos-per-second ÷ 1000` in integers (79); `numeric` casts rounded instead. |
| 82-114 | `lines` | One insert of the runner's priced request lines (`unnest` of `$15-$26`, kind `$8`, `model`) and the runtime line (kind `$50`, `runtime`). `ON CONFLICT (work_type, work_id, kind, seq) DO NOTHING` (110): a line already logged under the same kind and number is not doubled. |
| 115-123 | `added` | Counts, nanos and token sums of what this statement inserted. `model_nanos` takes only `kind = $8`, the model lines, for a durable task's model spend (118). |
| 124-131 | `moved` | The lease becomes reported and the cursor moves. |
| 132-138 | `settled` | The admission settles and returns its reply destination. |
| 139-149 | `tally` | The runner's lifetime counters. |
| 150-158 | `closed` | Closes the event, only while it is open (`$35`). This replaces `update_fleet_event_result.sql` on this path; that file stays for fold settlement and the durable-task input receipt. |
| 159-163 | `dropped` | Deletes tool-call records another fence wrote: a reclaimed lease's records describe a run whose answer does not stand. |
| 164-168 | `checkpointed` | The session the fleet's next run resumes from. |
| 169-176 | `released` | Expires the slot now and keeps the sandbox hold the runner asked for, capped at `$39`. |
| 177-187 | `owed` | The delivery obligation, for a known connector and a non-empty answer, once per event. The destination comes from `settled`, so the separate destination read is gone. |
| 188-192 | `audit` | The `lease_released` row; this was `insert_runner_event.sql`. |
| 193-201 | final row, part 1 | `claimed`; the lease's status read `FOR SHARE` (a replay that waited behind the winner sees it settled, not its own snapshot); the lease's own pricing facts and `fence_holds`, so the caller tells settled-before, repriced and fenced apart. |
| 202-206 | | Whether a durable task follows, the reply connector, and the ids written. |
| 207-217 | | Token sums as `numeric`; a sum past `bigint` returns `NULL`, and the report settles and logs `report_run_tokens_unread` instead of failing. |
| 218-238 | | Charge, the closed event row, `cost_nanos` (stored lines plus this statement's, `NULL` only for an event with no line), fleet status, pending approvals, counters plus this statement's nanos. |
| 239-254 | joins | `added`; a `LATERAL` token sum over the lease's stored lines (`select_run_tokens.sql`'s work); `LEFT JOIN`s so exactly one row returns even when nothing was claimed. |

The caller's next step is a pure function of this row (`commit.rs`
`next_step`), with a unit test per arm: settle; already settled; a first
reprice reruns once under the lease's own facts; a second reprice or a moved
fence logs only the runner's known request lines. A statement that wrote
nothing is rolled back before the known lines take a second connection,
because a dropped transaction held its slot and lease locks through that wait.

## §6 — `afd_fleet/src/lease/sql/apply_report_notice.sql` (added)

A durable task's settlement can replace the run's answer with a notice, and it
reads the runtime the report statement metered, so it runs after.

| Lines | Block | Why |
|---|---|---|
| 1-4 | header | |
| 5-8 | `event` | The event's answer becomes the notice. |
| 9-11 | `session` | The checkpoint resumes from the notice. |
| 12-14 | `owed` | An answer not yet delivered (`delivered_at IS NULL`) carries the notice. |
| 16 | `SELECT count(*) FROM event` | One row for `fetch_one`. The other two data-modifying CTEs run regardless. |

## §6, §11 — `afd_fleet/src/lease/sql/meter_lease.sql`

| Line | Change | Why |
|---|---|---|
| 48 | `$13` → `$10` | §6 dropped the work-type and seq-range binds of the old count subquery; later parameters renumbered. |
| 104-107 | `runtime_seq` | The runtime line's number: milliseconds from `g.created_at` to `g.last_metered_at`, the slice's start, read through the `FOR UPDATE` probe (15). It replaces a `count(*)` over the lease's daemon-band lines, whose snapshot did not see a racing writer's line; a renewal and a report therefore took the host lock to count correctly (`sql/meter.rs` comment, before). |

## §6, §11 — `afd_fleet/src/lease/sql/bill_expired_slice.sql`

| Line | Change | Why |
|---|---|---|
| 4 | `d_ms` computed once in `probe` | The slice's milliseconds were written out twice, once to bill and once to number; now once. |
| 5-6 | `runtime_seq` | Numbered as the meter numbers it: the slice's start. |
| 22-23 | `p.d_ms`, `p.runtime_seq` | The count subquery and its parameters `$4-$6` went. |

## §11 — `afd_fleet/src/lease/sql/original_request_owner.sql`

| Line | Change | Why |
|---|---|---|
| 1 | `0::bigint AS runtime_seq` | The known-request path measures no runtime; the column renamed with the slice it describes. |

## §11 — the ledger: `afd_billing/src/ledger/sql/*.sql`

| File, lines | Change | Why |
|---|---|---|
| `insert_lines.sql` 3, 8, 13-15 | `kind` column, value and a `$25::text[]` array | Every line the shared ledger writes names its kind. |
| `insert_lines.sql` 16 | `ON CONFLICT (work_type, work_id, kind, seq)` | The new key. |
| `insert_lines.sql` 19-20 | `FILTER (WHERE kind = $26)` | A durable task's model spend is its model lines; it was `seq < DAEMON_SEQ_BASE`. |
| `settle_lines.sql` 4 | `(kind, seq) IN (SELECT * FROM unnest($3::text[], $4::bigint[]))` | Settles exactly the lines named, by kind and number. |
| `select_unsettled.sql` 1, `select_unsettled_work.sql` 1, 9 | `kind` read; `ORDER BY kind, seq` | The relay rebuilds each line with its kind. A line of a kind this build cannot name skips its whole group, as an unknown work type already does (`sweep.rs`). |

## §11 — `afd_billing/src/report/sql/charges.sql`

| Lines | Change | Why |
|---|---|---|
| 1-3 | comment | Why the order changed. |
| 4 | `kind` returned | The charges API and the dashboard show what each charge billed: "Model call", "Runtime" or "Decision call", read from the row, not guessed from which counts are non-zero. |
| 11 | `(requested_at, id) > ($7, $11)` | The page cursor. `seq` is unique only within a kind now, so it cannot resume a page; `(requested_at, id)` is unique and chronological. The cursor stays an opaque string to clients. |
| 14 | `ORDER BY requested_at, id` | A work's charges in the order they happened. |

## §11 — decisions and task spend

| File, line | Change | Why |
|---|---|---|
| `afd_fleet/src/task/decision/sql/sequence.sql` 3 | `COALESCE(MAX(usage_sequence) + 1, 0)` | A lease's decision calls count from 0 under their own kind; they started at `DECISION_SEQ_BASE`. Setup chat's decision calls (`afd_authoring`) bind their call index the same way. |
| `afd_fleet/src/task/sql/bind_lease.sql` 22 | `u.kind = $5` | A task's model spend is its model lines; it was `u.seq < $5` with the daemon base. |

## §6 — deleted

- **`insert_runner_event.sql`:** one `INSERT` of the release audit row; now `commit_report.sql`'s `audit` block.
- **`select_run_tokens.sql`:** the token sums over a lease's lines; now the `LATERAL` at `commit_report.sql:241-251`, plus this statement's own lines.

## §7 — `afd_fleet/src/lease/sql/select_renewals.sql` (added)

The pre-read a host's renewal frame needs before its statement, for every lease
it names at once.

| Lines | What and why |
|---|---|
| 1-7 | header: why each fact is read, and that `renew_batch.sql` guards every fact it acts on. |
| 8-10 | Each lease's status, cancel mark, payer, fleet, posture, provider, model and task, and when it began: the daemon prices request lines in Rust and routes a task lease to the per-lease path. |
| 10-12 | The fleet's head revision (whose definition holds the ceilings) and the catalogue version, which key the definitions and price-book caches. |
| 13-15 | Only the leases named, and only this runner's: another runner's lease answers not found. |

## §7 — `afd_fleet/src/lease/sql/select_revision_documents.sql` (added)

| Lines | What and why |
|---|---|
| 1-7 | The setup documents of the revisions the definitions cache missed, in one read for all of them. A warm cache never runs it. |

## §7 — `afd_fleet/src/lease/sql/renew_batch.sql` (added)

One host's renewals in one statement. Each lease is judged on its own; a lease
that fails a check extends nothing and changes no other lease's answer.

| Lines | Block | What and why |
|---|---|---|
| 1-31 | header | Lock order (host shared, slot, lease), why the host row is shared and never exclusive, why a task lease is refused here, the guards, the `$1-$37` map. |
| 32-37 | `asked` | The per-lease arrays as rows: priced-at facts, runtime price and line id, ceilings and the revision they came from. |
| 38-49 | `host` | The host row `FOR SHARE` (49): renewable state, not degraded, the opening's credential and connection (42-43, `lock_host.sql`'s `authorized`), occupancy within `worker_count`. Shared so renewals never wait on each other; a credential rotation updating the row fences every renewal behind it, which then re-reads the new credential and refuses the old connection. The states come from `afd_runner::capacity::renewable_states()`, the list the locked check reads too. |
| 50-57 | `locked_slot` | `FOR UPDATE` on each lease's slot, in fleet order (56), so two statements never wait on each other in a cycle. |
| 58-68 | `probe` | `FOR UPDATE` on each lease, in the same order (67), reading its fence, deadlines, cursor, cancel mark and task. |
| 69-71 | `instant` | The instant every deadline is judged at, read once the locks are held. |
| 72-91 | `catalogue`, `floors`, `spend` | The current catalogue version, and each budgeted fleet's day and month spend, windowed and floored as `select_fleet_spend.sql` does. |
| 92-120 | `judged` | Every check as its own column, so the caller tells each refusal apart: the fence and deadlines; below the hard cap; priced as held, with `$6` `NULL` for a lease priced from no catalogue; ceilings current against the head revision (103); spend at or above a ceiling (107, reached as `budget::covers` reads it); the host (111); an unsettled admission. |
| 121-129 | `guard` | The leases every check passed, with the slice's milliseconds capped as the rater caps them. |
| 130-138 | `runtime` | Each passing lease's runtime line, numbered by its slice's start (134) and charged in integers, rounded up. |
| 139-150 | `moved`, `slot` | Both deadlines move to the same capped instant, under the lease's fence. |
| 151-181 | `lines` | The request lines (`unnest`, kind `$31`, `model`) and the runtime lines (kind `$32`) of passing leases, keyed `(work_type, work_id, kind, seq)` (179). |
| 182-195 | final rows | One row per lease asked about: each check, the new deadline, whether the slot moved too (190: a lease whose slot did not move is lost, never extended past a reclaim), and what its lines charged. |

## Identifiers

No statement binds or casts an identifier between `text` and `uuid`. Every
`uuid` cast is uuid to uuid on a `Uuid7` bind; every `bigint` cast is a count,
a duration, nanos or a version; every `text` cast is an answer, an event type
or a connector name.
