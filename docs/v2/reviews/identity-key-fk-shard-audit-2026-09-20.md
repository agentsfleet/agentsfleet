# Identity, keys, foreign keys and shard readiness : Sep 20, 2026 (revision 2)

**FAIL on identifier integrity. PASS on foreign keys. PASS on key correctness, unproved on key
optimality. NOT READY for a billing split, which no document commits to.**

Revision 2 incorporates the adversarial review by Tarzy (Codex) of revision 1, Sep 20, 2026. Every
challenge was re-verified against source before its disposition was recorded; the table in §Dispositions
is the audit trail. The headline change: **revision 1 graded three findings Critical and none of them is
reachable today. All three are High.** One finding revision 1 missed entirely is added as E1, and it is the
only one in this document with a present-day interleaving that loses data.

The schema's UUID work is finished and good. `afd_core::id::Uuid7` enforces one canonical spelling
(`rustd/crates/afd_core/src/id.rs:206`); 29 of 38 tables carry the version CHECK. Beside it runs a second
identifier — `<millis>-<sequence>`, minted at `rustd/crates/afd_admission/src/lib.rs:252` — with no type,
no constraint, and a global-uniqueness property that a money invariant depends on and that
`schema/800_fleet_events.sql:54` explicitly denies. `f71a25fd9` was one bill for that. Revision 1 called it
a single root cause; revision 2 does not — Tarzy's #26 is right that a type fixes representation and
nothing else. The five distinct problem classes are named in §Conclusion.

Audited at `57a77c73c` (`origin/main`), worktree `/Users/kishore/Projects/agentsfleet-identity-audit`. No
code, SQL or database changed. **No live database, EXPLAIN, or test gate was run** — every number below is
read from source, and every performance claim is structural rather than measured. Where slot 914 measured
something, its numbers are cited as slot 914's, not this audit's.

## Inventory, measured

57 SQL files, 38 tables. **53 FK declarations; 52 expected after migrations** (slot 915 drops one).
**47 `ON DELETE CASCADE`; 2 `SET NULL` surviving; 1 `RESTRICT`; 2 no-action.** Foreign key (FK) and
primary key (PK) are abbreviated from here.

```
grep -n "REFERENCES" schema/*.sql | grep -v ':[0-9]*: *--' | wc -l          → 53
grep -n "ON DELETE CASCADE" schema/*.sql | grep -v ':[0-9]*: *--' | wc -l   → 47
grep -rl --include="*.rs" "event_id" rustd/crates/*/src/ | wc -l            → 108 files, 20 crates
grep -rn --include="*.rs" "event_id: &\?\(str\|String\|&'a str\)" rustd/crates/*/src/ | wc -l → 55
```

Revision 1 printed a narrower signature pattern (`\(str\|String\)`) beside the count from the wider one;
the narrower pattern gives 39. The 55 stands and its command is now the one printed. Revision 1 also
claimed the `connector_channels` grep was empty "at all" — it hits `rustd/crates/afd_db/src/migration.rs:123`,
a migration registration, which is not a query consumer. Both corrected per Tarzy #1.

## Relationship to the Sep 18 audit

`docs/v2/reviews/schema-usage-audit-2026-09-18.md` (848 lines, `c6b54a1a5`) covered usage, indexes and
orphans. This audit is about identity and does not re-derive it.

**Restated only where identity is the reason** — F01 orphan tables, F06 orphan session columns, D02
independently-referenced scope fields, D05 recovery scans that grow with history. All four unchanged on
this revision.

**Fixed since** — F04 (the delivery stamp had no usable index) is closed by `schema/914:48`; `MARK_DELIVERED`
at `afd_admission/src/sql.rs:155` qualifies. Slot 914's before/after at `914:11`–`:22` are its measurements.

**Partially fixed** — F05. The fleet rotation is repaired: keyset bound at `afd_admission/src/sql.rs:198`,
per-fleet continuation at `:239` and `reconcile.rs:163`. But the head-probe shortcut remains at
`reconcile.rs:232`, and `afd_admission/src/reconcile/progress.rs:33`–`:40` states the residual plainly:
progress is per-process, a restart resets it, and on a fleet nobody is consuming a partially-repaired
stream's remaining lost rows stay invisible "indefinitely". Revision 1 said "closed"; Tarzy #2 is right
that recovery coverage is still conditional.

**New ground** — identifier typing, the logical id's format and decoders, where the event id enters,
`event_created_at` provenance, `checkpoint_id`, `action_id`'s type, shard readiness, and E1.

---

# Dispositions of the revision-1 review

| # | Claim | Disposition | What changed |
|---|---|---|---|
| 1 | Evidence | **accept** | signature grep command corrected; `connector_channels` claim narrowed |
| 2 | F05 closed | **accept** | now "partially fixed", `progress.rs:33` cited |
| 3 | A1 Critical | **accept** | High. No reachable collision mechanism today |
| 4 | A1 misses a writer | **accept** | third target `afd_billing/src/sql.rs:136` added; NULL `fleet_id` and rolling-deploy handled |
| 5 | A1 index claim | **accept** | "served equally" → "eligible for a bounded lookup"; migration cost stated |
| 6 | A2 format ≠ provenance | **accept, with one hold** | High; format and provenance separated. Hold: `lease/event.rs:209` skips the stamp for a non-parsing id, so format has a concrete consequence |
| 7 | Not the only appender | **accept** | `afd_bench/src/lane/lease/seed.rs:255` (bench) and `replay.rs:164` (legitimate) qualified |
| 8 | A3 conflates two ids | **accept** | receipt and logical id separated; `afd_dragonfly::streams::EventId` name collision added; `schema_literals.rs:88` cited as precedent |
| 9 | A3 stored column | **accept** | withdrawn pending measurement to slot 914's standard |
| 10 | A4 ORDER BY change | **accept** | withdrawn — `idx_fleet_admissions_fleet_id` carries only `fleet_id` and receipt order ≠ admission order |
| 11 | A5 unrelated to root cause | **accept** | removed from the root-cause story; `approval_route.rs:129` signature check noted |
| 12 | B4/B5 severities | **accept** | no severity; moved to §Checked, no defect |
| 13 | B1 test and NULL bytes | **accept** | null bitmap, not three bytes; test is a diagnostic-copy test; remedy is a null fixture |
| 14 | B2/B3 deletion ≠ resolution | **accept** | stored-data inspection and feature decision before removal |
| 15 | Key-design PASS | **accept** | narrowed to correctness; optimality unproved |
| 16 | C2 915 argument | **accept** | High → investigate; "identical argument" dropped |
| 17 | C3 write order | **accept** | scenario dropped — `pull.rs:187` precedes `deliver.rs:140` |
| 18 | C4 "unscoped" | **accept** | qualified: no constraint, writer derives scope from the gate |
| 19 | "Shards cleanly by fleet" | **accept** | replaced with a transaction-and-routing assessment |
| 20 | D2 rows not equivalent | **accept** | split; obligations and catalogue removed from "does not survive" |
| 21 | D3 Critical, miscites | **accept** | High as a blocker to the split; cites fixed; trigger "fails or is removed" |
| 22 | D4 "makes safe" | **accept** | "necessary, not sufficient"; residual failures listed |
| 23 | Reconciliation | **accept** | reframed as an investigation with its inputs |
| 24 | Omitted clients | **accept** | `cli/src/commands/billing.ts:100`, `fleet_steer_events.ts:87`, `repair.rs:66` added as dependencies |
| 25 | Continuation race | **accept — new finding** | E1, High |
| 26 | Single root cause | **accept** | replaced by five classes |

---

# Stream A : Identifier integrity

## A1 (HIGH) The usage ledger's conflict target is global; the event id it keys on is not

**Evidence.** `CONSTRAINT uq_usage_ledger_event_id_charge_type UNIQUE (event_id, charge_type)`
(`schema/710_usage_ledger.sql:74`) — no tenant, workspace or fleet in the key. Three executable writers
arbitrate on it:

- `rustd/crates/afd_fleet/src/lease/sql/renew.rs:139` — `ON CONFLICT (event_id, charge_type) DO UPDATE SET
  credit_deducted_nanos = … + EXCLUDED.credit_deducted_nanos, …` — accumulates, no ownership re-check
- `rustd/crates/afd_fleet/src/lease/sql/report.rs:187` — the same
- `rustd/crates/afd_billing/src/sql.rs:136` — the receive insert, `ON CONFLICT (event_id, charge_type) DO
  NOTHING` (revision 1 missed this one; Tarzy #4)

`core.fleet_events` states the opposite property about the same value: `schema/800:54` — *"Composite
because `event_id` is unique per fleet, not globally"*. The property the ledger relies on is supplied by one
thing: `seq BIGINT GENERATED ALWAYS AS IDENTITY` on `core.fleet_admissions` (`schema/910:68`), which makes
`<created_at>-<seq>` unique across the deployment. Written down nowhere as a billing invariant.

**Failure shape if it breaks.** Tenant B's renewal takes the conflict arm on tenant A's row. Amounts and
token counts accumulate into A's row; `tenant_id`, `workspace_id`, `fleet_id`, `fleet_name` stay A's. The
wallet debit is correct — it locks `g.tenant_id` (`renew.rs:126`) — so B's balance drains while A's charges
page shows B's spend. Invariant 5 (`schema/710:28`) is falsified both ways; `budget_used_nanos` on A's fleet
advances by B's delta through `schema/890:77`–`:82`. Silent.

**Why High and not Critical (Tarzy #3, accepted).** No reachable collision mechanism exists today. One
global sequence, and every production producer goes through `admit()` — six sites: `afd_events/src/steer.rs:90`,
`afd_ingress/src/deliver.rs:60`/`:61`, `afd_cron/src/fire.rs:85`, `afd_runner/src/sweep/repair.rs:191`,
`afd_approval/src/inbox.rs:349`. Two other appenders exist and neither breaks uniqueness: the replay
sweeper re-appends admitted rows under their own id (`afd_admission/src/replay.rs:164`), and the benchmark
mints ids outside admission (`afd_bench/src/lane/lease/seed.rs:255`) but is not production. Resetting the
sequence alone does not repeat an id unless the millisecond also repeats. Time-partitioning the ledger does
not touch the sequence. So: a money invariant resting on an undocumented property the schema contradicts —
integrity debt, High under the brief's rubric, Critical only once something actually removes the guarantee.

**The case against fixing it.** Three conflict targets, a migration on the money table, an index rebuild on
the hottest write — and slot 914 records that this migrator builds indexes inside a transaction, holding
`SHARE` for the build's duration (`schema/914:37`–`:44`).

**Recommendation, revised.** Widen the arbiter to `(fleet_id, event_id, charge_type)`, with three
conditions revision 1 did not state:

1. **All three targets in one change** — `renew.rs:139`, `report.rs:187`, `afd_billing/src/sql.rs:136`.
   Dropping the old unique while changing two leaves the receive insert without an arbiter.
2. **`fleet_id` NULL handling.** `fleet_id` is nullable (`710:50`) and PostgreSQL treats NULLs as distinct
   in a unique index, so rows with a NULL `fleet_id` would be unarbitrated. Every live writer binds a
   non-null fleet (`renew.rs:134`, `report.rs:182`, `sql.rs:133`), and the repository is not in production
   and rebuilds from empty — so make the column `NOT NULL`. On a populated deployment the alternative is
   `UNIQUE NULLS NOT DISTINCT` (PostgreSQL 15+) and a treatment of pre-915 rows, which `915:41` says cannot
   be reconstructed.
3. **Rolling deploy.** Old binaries name the old arbiter; the migration must land after every replica is
   replaced, or keep both constraints across one deploy. The change is not reversible once two fleets
   legitimately share an id.

The cost subselect at `afd_events/src/history/statement.rs:52`–`:55` binds both `fleet_id` and `event_id`,
so a `(fleet_id, event_id, charge_type)` index is eligible for the same bounded lookup — eligible, not
"equal" in latency or size (Tarzy #5). Neither index covers the summed amount.

**Dependencies outside the database (Tarzy #24).** Three consumers assume global uniqueness and would
still conflate after the schema fix: the command-line billing renderer groups a tenant's charges by
`event_id` alone (`cli/src/commands/billing.ts:100`–`:102`); the repair branch name encodes only the event
id (`afd_gate/src/policy/repair.rs:66`), so two fleets writing one repository collide on a branch; and
`fleet_steer_events.ts:87` parses the millisecond prefix. None is a defect today; all three must move with
any change to identifier scope.

## A2 (HIGH) The event id enters from a queue payload with only a non-empty check — and a bad one skips the delivery stamp

**Evidence.** `rustd/crates/afd_fleet/src/lease/envelope.rs:160` — `event_id: field(FIELD_EVENT_ID)?`,
where `field` (`:145`–`:151`) checks presence and non-emptiness. The comment at `:157`–`:159` says an entry
"appended by something that did not admit it … is refused here"; only the empty one is. Five lines down,
`workspace_id` from the same payload goes through `Uuid7::parse` (`:165`).

**Two problems, which revision 1 ran together (Tarzy #6).**

*Format.* A malformed id has a concrete consequence beyond shape confusion: `stamp_admission_delivered`
parses it with `logical_parts` and **returns `Ok(())` without stamping when the parse fails**
(`lease/event.rs:209`–`:211`). The event runs, is billed, and its admission — if one exists — is never
marked delivered, which is exactly the state the reconcile pass reads as lost work and re-appends
(`event.rs:120`–`:129` explains why that matters). A boundary parse closes this.

*Provenance.* A canonical-looking id belonging to another event passes any parse. Nothing at lease time
checks that an admission row exists, belongs to this fleet, or matches this payload. The stamp at
`MARK_DELIVERED` (`sql.rs:155`) keys on `(fleet_id, created_at, seq)` and accepts zero rows. A newtype does
not fix this and revision 1 implied it would.

**Reach.** Internal. Runners hold no datastore connection (`docs/architecture/runner_fleet.md:657`); the
stream is daemon-only. An integrity boundary, not a tenant-facing one.

**Recommendation.** Two separate items. (1) A newtype for the logical id in `afd_core` — **not named
`EventId`**: that name is already taken by the *receipt* type at `afd_dragonfly/src/streams.rs:148`, which
is the transport entry id and a different value. Call the new one `AdmissionId` or `LogicalId`, and note the
existing name as itself a hazard — a type named `EventId` that holds the thing the event id is *not*. Blast
radius as measured above: 20 crates, 108 files, 55 signatures. (2) Provenance is a separate design question:
whether lease should verify the admission row before running, at the cost of a read on the hot path. Not
answered here.

## A3 (HIGH) The logical id has two spellings and the receipt a third decoder; the crate's own rule forbids the first

**Evidence.** `afd_admission/src/lib.rs:259`–`:261` states the rule — the one crate owns both directions,
"a second parser elsewhere could drift". The same crate spells the logical id a second time, in SQL:

```
rustd/crates/afd_admission/src/sql.rs:126   AND e.event_id = a.created_at::text || '-' || a.seq::text
```

Separately — and revision 1 conflated them (Tarzy #8) — the *receipt*, a different value, is decoded in SQL:

```
rustd/crates/afd_admission/src/sql.rs:128 ORDER BY split_part(a.receipt, '-', 1)::bigint DESC,
rustd/crates/afd_admission/src/sql.rs:129          split_part(a.receipt, '-', 2)::bigint DESC
```

`receipt` is `TEXT` with no CHECK (`schema/910:78`), so a non-numeric value raises SQLSTATE 22P02 inside the
consumer-group recovery statement. Dragonfly mints `<ms>-<seq>`, so this cannot happen today.

**Failure shape.** `SELECT_DELIVERED_CURSOR` (`sql.rs:121`) decides where a lost consumer group is
recreated. A drift between `:126` and `lib.rs:253` returns zero rows silently; per
`docs/architecture/datastore_scaling.md:217` the group is then recreated at a position that re-runs
delivered work or loses accepted work.

**Recommendation, revised.** Revision 1 proposed a `GENERATED ALWAYS … STORED` column. **Withdrawn**
(Tarzy #9): its cost was asserted, not measured, on the table that is the deployment's throughput valve;
stored generated columns are computed on every write and adding one rewrites the table; and it would not
remove `lib.rs:253` or `replay.rs:86` anyway. The cheaper precedent already exists in this repository:
`afd_wire/tests/schema_literals.rs:88` pins schema-side literals to the Rust spelling with a test. A test
that renders `logical_id(created_at, seq)` and asserts it equals what `sql.rs:126` would compute — or, more
simply, one integration case that inserts an admission and reads it back through `SELECT_DELIVERED_CURSOR`
— pins the two spellings without a column. For the receipt, constrain the shape at the column or decode in
Rust; either is cheaper than a statement abort on the recovery path.

## A4 (HIGH) `SELECT_DELIVERED_CURSOR` scans a fleet's whole admission history, sorted by an expression

**Evidence.** `sql.rs:121`–`:130`: predicate `a.fleet_id = $1 AND a.receipt IS NOT NULL`. The partial indexes
cover `receipt IS NULL` (`910:90`), `receipt IS NOT NULL AND delivered_at IS NULL` (`910:114`), and
`delivered_at IS NULL` (`914:48`) — the read wants the complement of all three. It falls to
`idx_fleet_admissions_fleet_id` (`910:124`), joins each row to `core.fleet_events`, and sorts by an
expression nothing indexes. Same class as the defect 914 measured one statement over (`914:10`–`:14`); D05
of the Sep 18 audit flagged the sort.

**Recommendation, revised.** Revision 1 proposed ordering by `(created_at, seq)` instead. **Withdrawn**
(Tarzy #10), for two independent reasons: `idx_fleet_admissions_fleet_id` carries only `fleet_id` and could
not supply that ordering either; and the statement wants the greatest *receipt* (`sql.rs:111`), which a
replayed admission can hold out of `(created_at, seq)` order — the very case `910:101`–`:106` describes.
What remains is a performance investigation: an EXPLAIN on a fleet with realistic history, then a decision
between a partial index on the delivered set and a Rust-side selection. High as recovery debt; no incident
is demonstrated.

## A5 (HIGH) `action_id` is a minted UUIDv7 living in a TEXT column

**Evidence.** `core.fleet_approval_gates.action_id TEXT NOT NULL` (`schema/810:33`), bare index at `:56`.
Both writers mint a `Uuid7`: `afd_approval/src/request.rs:211` (`self.mint` → `Uuid7::encode`, `:268`) and
`afd_gate/src/gate/sql.rs:100` (`pub action_id: &'a Uuid7`), bound as text at `:127`. The connector
callback verifies the webhook signature first (`approval_route.rs:129`), then reads `action_id` as a bare
`String` (`:83`), checks `is_empty()` (`:135`), and probes the index (`afd_approval/src/sql.rs:157`, `:190`).

**What it is and is not.** One canonical UUID spelling stored in a different SQL type — 36 bytes, a text
btree, no CHECK where 29 sibling tables have one. It is *not* the logical-id/UUID confusion of `f71a25fd9`
and revision 1 was wrong to group it there (Tarzy #11).

**Recommendation.** Parse at `approval_route.rs:135` with `Uuid7::parse`, after confirming every connector
that posts here sends the canonical spelling. Leave the column type until a migration touches the table for
another reason; the append-only trigger (`810:95`, body at `833:11`) governs row updates, and an
`ALTER COLUMN TYPE` needs its own analysis, not a blanket "impossible". High under the debt rule; operational
impact today is low.

## A6 (MEDIUM) `910:22` "Not an identity column" reads against `910:68`

`schema/910:22`–`:24` says `seq` is "Not an identity column: `id` is the row's identity"; `:68` declares
`GENERATED ALWAYS AS IDENTITY`. The design is coherent — the prose means "not the row's identity", the DDL
uses the mechanism. One precise SQL term used in its opposite sense above the DDL that uses it correctly.
Three-word fix. Medium because it is editorial, not debt; Tarzy #12 concurs.

## A7 : externally-supplied identifiers — cleared

`core.users.oidc_subject` (`220:18`), `core.api_keys.created_by` (`240:24`, deliberately not an FK per
`240:7`–`:10`), `core.connector_installs.external_account_id` (`550:25`),
`core.connector_channels.external_channel_id` (`560:22`), `core.model_library.model_id` (`400:53`,
distinguished from `id` at `400:7`–`:12`), the `repair_*` provider identifiers (`831:14`, `834:8`, `834:12`),
`fleet.runners.host_id` (`600:53`), `receipt` on both ledgers (`910:78`, `913:64`). All TEXT, all correctly
so. No finding.

---

# Stream B : Redundant identity

## B1 (HIGH) `core.fleet_events.checkpoint_id` has no writer on this revision and is published on every response

**Evidence.** Declared at `schema/800:43`. `INSERT_FLEET_EVENT` does not list it (`afd_events/src/sql.rs:31`–`:33`);
no UPDATE sets it on this revision (`grep -rn --include="*.rs" "checkpoint_id" rustd/ | grep -iE
"insert|update|set "` → nothing). It is selected on every read (`history/statement.rs:49`), decoded, and
carried on the wire (`afd_wire/src/event.rs:258`, `:347`; `tail.rs:72`). The dashboard copies it into the
diagnostic payload unconditionally (`ui/packages/app/components/domain/EventDetailsDialog.tsx:306`).

**Corrections to revision 1 (Tarzy #13).** A NULL costs no value bytes — it lives in the tuple's null
bitmap; "three bytes" was wrong. The test at `event-details-dialog.test.tsx:269`–`:300` is a diagnostic-copy
and redaction test, not a rendering branch; its `checkpoint_id: "checkpoint_1"` fixture (`:278`, asserted at
`:293`) is a value production cannot emit, but the test's purpose is the redaction at `:297`–`:299`, and
removing it loses that coverage. And "no writer on this revision" does not prove an upgraded database holds
no historical values.

**Recommendation.** Decide ownership. If resumable checkpoints are not coming: inspect stored values, then
drop column, projection, wire field and the fixture value together — a user-surface change with a
`~/Projects/docs` branch. If they are: the column shipped without its writer; say so in the spec that writes
it. Either way, give the test a null fixture; do not delete it.

## B2 (HIGH) `core.fleet_sessions.execution_id` / `execution_started_at` — F06, unchanged

`schema/510:29`–`:30`. `afd_fleet/src/lease/sql/fleet.rs:21`–`:29`: no production writer of a value, no
production reader. Credit to the Sep 18 audit. Same remedy shape as B1 — decision, stored-data inspection,
then a forward migration.

## B3 (HIGH) Two runtime-orphan tables — F01, unchanged — and one holds the schema's only FK to `core.fleet_events`

`core.connector_channels` (`schema/560:17`) and `core.repair_run_results` (`schema/831:5`). On this revision
neither has an executable query consumer: outside `schema/`, `docs/v2/` and the migration array
(`afd_db/src/migration.rs:123`, `:144`), `grep -rn "connector_channels\|repair_run_results" --exclude-dir=node_modules .`
finds nothing.

The identity angle: **`core.repair_run_results` carries the schema's only FK to `core.fleet_events`** —
`FOREIGN KEY (fleet_id, event_id) REFERENCES core.fleet_events(fleet_id, event_id) ON DELETE CASCADE`
(`831:19`–`:21`). Eight other columns hold an event identifier and none is constrained (C3). The one
enforced edge is on a table nothing writes.

**Recommendation.** F01's remedy stands: decide feature ownership, inspect stored rows, then remove or
restore — the Sep 18 audit's own caution at `schema-usage-audit-2026-09-18.md:822`, that deleting schema can
conceal a missing feature, applies. Whichever way it goes, `831:19` is the template C3 wants.

---

# Checked, no defect

Recorded so nobody removes them later (Tarzy #12: these carry no severity).

**`core.fleet_admissions`'s three keys each have a consumer.** `id` (`910:66`) is the handle the receipt
writes address (`sql.rs:64`, `:99`; projected `:77`). `seq` (`:68`) is half the public id and the tiebreak in
three indexes (`:91`, `:115`, `914:49`). `UNIQUE (producer, producer_key)` (`:83`) is the retry arbiter
(`sql.rs:51`). `core.fleet_obligations` is the same shape (`913:56`, `:58`, `:75`). `billing.usage_ledger.id`
is the keyset tiebreak (`afd_billing/src/tenant_sql.rs:50`, `:63`; indexed `720:31`) and reaches the wire
(`:43`). Having a reader rejects blind surrogate removal; it does not prove the cheapest design. Admissions
carries six indexes after 914 (two constraint-backed, three in 910, one in 914) and obligations five; the
full write lifecycle across the partial indexes is unmeasured (Tarzy #15).

**PK shape matches access.** `core.fleet_events` PK `(fleet_id, event_id)` (`800:56`) serves the detail read
(`statement.rs:118`), the history keyset (`:94`–`:99`), and the reclaim join (`610:15`). The parent-keyed
one-to-one tables (`430:29`, `510:23`, `630:51`, `650:32`, `700:30`, `880:23`) each carry exactly one unique
index — the `ON CONFLICT` property `docs/SCHEMA_CONVENTIONS.md:41` exists for.

**Composite scope keys are correct and not free.** `500:61`–`:63` and `610:76`–`:78` need the parent unique
at `500:69`, copied scope columns, and an insert-time check. `610:28`'s "no extra join at settle" is true;
"optimal" is unproved.

---

# Stream C : Foreign keys

## C1 : the census

52 expected live FKs. **Enforcing what the application cannot** — the two composite scope keys above.
`610:23`–`:28` states the stake: settle locks the wallet found through the lease's own `tenant_id`
(`renew.rs:84`–`:87`), so an unconstrained copy would let a lease-issue bug debit another tenant.

**Restating what the application guarantees** — the ~40 single-column parent edges. Every writer derives them
from a trusted row. Kept because they make erasure complete: `500:29`–`:33` records the hand-maintained
delete order that silently missed a table.

## C2 (HIGH, investigate) `usage_ledger.workspace_id` is still `SET NULL`; the retention policy is undecided

Slot 915 dropped the `fleet_id` FK because `SET NULL` "stripped a surviving charge of the only thing naming
what it paid for" (`915:7`–`:9`) and kept the name because a bill has to say what it paid for
(`915:36`–`:37`). `workspace_id` is still `REFERENCES core.workspaces(id) ON DELETE SET NULL` (`710:49`),
with `idx_usage_ledger_workspace_id` kept to serve the action (`720:64`).

**Revision 1 said 915's identical argument requires dropping it. It does not** (Tarzy #16). 915's concrete
argument is fleet-specific — the callsign derivation at `ui/packages/app/lib/fleets/identity.ts:91` reads
`fleet_id` and nothing equivalent reads `workspace_id` from a charge. `SET NULL` also has a defensible
meaning: the charge survives and the reference records that its workspace is gone. Dropping the FK admits
values that name nothing and cannot be restored on retained rows.

**What remains.** Two neighbouring columns on the money table with opposite retention semantics and no
sentence saying why. `915:104` documents `fleet_id`'s; `710:49` has none. That is the finding: decide
whether workspace attribution survives a workspace delete, and write the decision beside the column either
way.

## C3 (HIGH) Nine columns hold an event identifier; one is constrained to a real event

| Column | Line | FK to `core.fleet_events`? |
|---|---|---|
| `fleet.runner_leases.event_id` | `610:57` | no |
| `billing.usage_ledger.event_id` | `710:51` | no |
| `core.fleet_events.event_id` | `800:33` | (is the key) |
| `core.fleet_events.resumes_event_id` | `800:44` | no — self-reference |
| `core.fleet_approval_gates.event_id` | `811:21` | no |
| `core.repair_pr_links.event_id` | `830:22` | no |
| `core.repair_run_results.event_id` | `831:10` | **yes** — `831:19`–`:21` |
| `core.repair_verifications.verifier_event_id` | `835:11` | no |
| `core.fleet_obligations.event_id` | `913:62` | no |

**Revision 1's dangling-lease scenario is withdrawn** (Tarzy #17). The write order is the reverse of what it
claimed: `record_received` inserts the event row at `afd_fleet/src/lease/pull.rs:187`, the money pass runs
at `:257`, and the lease is issued "LAST, and only once everything above succeeded" at
`afd_fleet/src/lease/deliver.rs:140`. A lease cannot precede its event. The autocommit note at
`lease/event.rs:120`–`:129` concerns the admission stamp after the event insert, not the lease.

**What stands.** `800:10`–`:13`'s "both tables cascade from the same fleet, so the join cannot dangle" proves
the rows die together, not that the child was written. Eight columns carry an event identifier the database
does not check. High under the debt rule as an unenforced relationship on the money and evidence tables,
with no demonstrated present failure.

**Recommendation.** Differentiate. `core.fleet_obligations` and `core.fleet_approval_gates` are written
after the event row exists and already cascade with it — candidates for the `831:19` pattern, with the
deletion behaviour tested. `fleet.runner_leases` is written after the event too (above), so it is also a
candidate. `billing.usage_ledger` must outlive a purge (`915:3`–`:6`) and should stay unconstrained — and
`710` should say so the way `915:104` does for `fleet_id`.

## C4 (HIGH) `resumes_event_id` is unconstrained — and E1 shows it is also racy

`schema/800:44`, partial index at `:80`–`:82` with no runtime lookup (D08, Sep 18). The writer derives the
predecessor from the resolved gate (`afd_approval/src/inbox.rs:269`–`:270`) and inserts the successor with
that gate's fleet and workspace (`:364`–`:371`), so the current writer does not accept a foreign-fleet
predecessor (Tarzy #18). What is missing is any database proof that the predecessor exists and shares the
fleet. A composite self-reference would supply both and would still admit NULL — which is what E1 produces.

## C5 : FKs that would cross a proposed boundary

Billing boundary (`billing.*` to its own store):

| FK | Line | Verdict |
|---|---|---|
| `billing.tenant_wallet.tenant_id → core.tenants` CASCADE | `700:30` | replace with an explicit cross-store erasure step — which reintroduces the ordered delete list `500:29` warns about |
| `billing.usage_ledger.tenant_id → core.tenants` CASCADE | `710:48` | same; `710:24`'s erasure requirement becomes a two-store operation |
| `billing.usage_ledger.workspace_id → core.workspaces` SET NULL | `710:49` | resolve C2 first |

Control-plane boundary (`fleet.*` from `core.*`):

| FK | Line | Verdict |
|---|---|---|
| `fleet.runner_leases (fleet_id, workspace_id, tenant_id) → core.fleets` CASCADE | `610:76` | **keep, or do not split** — the money guarantee; a pre-insert check does not stop a concurrent parent delete (Tarzy #22) |
| `fleet.runner_affinity.fleet_id → core.fleets` CASCADE | `630:51` | application-managed lifecycle |
| `fleet.runners.tenant_id → core.tenants` CASCADE | `600:58` | drop — NULL on every row today (`600:76`) |

`memory.memory_entries.fleet_id → core.fleets` (`820:44`) was listed under this boundary in revision 1
without saying why memory would move; it is removed from the table. `820:12`–`:18` records that the edge is
what stopped an erased account keeping its memory, and nothing proposed here moves the memory schema.

---

# Stream D : Shard readiness

## D0 : is the schema ready

**Not for a billing split, and nothing asks it to be.** `docs/architecture/scaling.md:384` puts Postgres
scaling at pgbouncer and plan sizing; sharding Dragonfly is the committed direction
(`datastore_scaling.md:27`); sharding Postgres is written down nowhere. Under the architecture-consult rule
the document wins. What follows answers "what breaks", not "do this".

Revision 1 said "shards cleanly by fleet". **Withdrawn** (Tarzy #19). A `fleet_id` on every row is routing
material, not transaction locality, and the transactions cross fleets: two fleets of one tenant share the
wallet locked at `renew.rs:84`–`:87`; fleet names are unique per workspace (`500:57`); the workspace history
page reads across a workspace's fleets (`statement.rs:94`); and the operator plane lists one runner's leases
across every fleet it touched, joined to event history with the metering mirrors
(`afd_fleet_ops/src/sql.rs:19`–`:30`). A fleet shard is a routing exercise with a shared-wallet transaction
at its centre.

## D1 : the shard key per table

By **fleet** — present on every row below `core.fleets`: `510:23`, `520:16`, `540:24`, `560:23`, `610:54`,
`630:51`, `800:31`, `810:29`, `811:21`, `820:43`, `830:21`, `831:9`, `835:9`, `880:23`, `910:69`, `913:59`.

By **tenant** — absent from four high-volume tables: `core.fleet_events` (`800:31`–`:32`),
`core.fleet_approval_gates` (`810:29`–`:30`), `core.fleet_admissions` (`910:69`–`:70`),
`core.fleet_obligations` (`913:59`–`:60`); `memory.memory_entries` carries `fleet_id` only (`820:43`). This
is routing and enforcement work — placement can be decided from the parent before a child query runs
(Tarzy #19) — not a prohibition. `billing.*` is the one area already keyed by tenant (`700:30`, `710:48`):
the opposite axis from the rest.

## D2 (HIGH, one row) Global state, split by what actually breaks

| State | Line | Under a fleet shard |
|---|---|---|
| `core.fleet_admissions.seq` IDENTITY | `910:68` | **breaks A1** — independent sequences end global uniqueness of the event id, and nothing else supplies it |
| `core.fleet_obligations.seq` IDENTITY | `913:58` | fine — ordering is promised per destination (`913:94`), which independent sequences keep |
| `core.model_catalogue_revision.revision` | `410:34` | fine — a global catalogue stays with the catalogue (`410:4`); not every table distributes |
| `fleet.runner_affinity.fencing_seq` | `630:53` | fine — per fleet |
| `billing.tenant_wallet.balance_nanos` | `700:31` | fine — per tenant |
| both counter tables | `650:36`, `880:27` | fine — per entity |

Revision 1 listed all three sequences as "does not survive"; only the first matters (Tarzy #20).

Under a **tenant** shard, nine lookups run before the tenant is known and need a global directory:
`users.oidc_subject` (`220:28`), `api_keys.key_hash` (`240:31`), `cli_credentials.credential_hash`
(`250:72`), `runners.token_hash` (`600:73`), `connector_installs (provider, external_account_id)`
(`550:35`), `fleet_library.id` (`450:38`), `model_library (provider, model_id)` (`400:61`),
`platform_provider_defaults.provider` (`420:44`), and `connector_channels` (`560:29`, dead). Revision 1
counted eleven; `fleet_admissions (producer, producer_key)` (`910:83`) and `usage_ledger (event_id,
charge_type)` (`710:74`) already have scope on the row and belong under A1, not here.

## D3 (HIGH) A billing split breaks one atomic statement, one trigger, and one hot-path read

**The statement.** `RENEW_AND_METER` (`afd_fleet/src/lease/sql/renew.rs`) locks the lease and the affinity
slot `FOR UPDATE OF l, a` (`:82`), locks the wallet `FOR UPDATE OF tb` (`:87`), fences on
`fencing_token >= fencing_seq` (`:100`), then in one transaction extends the lease (`:102`), advances the
durable metering cursor (`:111`), debits the wallet (`:120`) and accumulates the ledger (`:129`). The
property this buys is stated at `schema/610:44`–`:45` — a re-sent renewal "double-bills nothing" — and at
`renew.rs:54`–`:58`, a re-sent renewal "charges approximately zero" because the cursor it diffs against
already advanced. (Revision 1 cited `renew.rs:40`, which is `SELECT_LEASE_FOR_RENEW`; corrected, Tarzy #21.)
`CLAIM_AND_SETTLE` (`report.rs:176`, `:208`) shares the chain, and `report.rs:12` explains the reclaim race
the common lock prevents.

Put the fence in one store and the wallet in another and that property is gone: a cursor advance that
commits before a debit that fails has metered tokens nobody paid for; the reverse re-charges the delta on
the next tick.

**The trigger.** `trg_usage_ledger_bump_budget` fires on `billing.usage_ledger` and inserts into
`core.fleet_activity_counters` (`schema/890:75`–`:89`). Across stores the target table is absent, so the
trigger **fails the ledger write loudly** — or someone removes it and the counter goes silently stale.
Revision 1 said only "silently stops"; both halves are stated now. Either way, the counter is
trigger-maintained precisely so no runtime role can write it (`890:6`–`:13`), and there is no fallback.

**The reads.** The cost subselect on every events page (`statement.rs:52`–`:55`, spliced at `:92`, `:115`,
`:135` and `afd_events/src/sql.rs:54`) becomes a batched second round trip — survivable. The budget drain
(`schema/720:37`–`:38`) runs on every receive and renewal, roughly every 25 seconds per live run
(`720:38`), and is not batchable.

**Why High.** Colocation is what preserves the guarantee today, and nothing proposes ending it. This is a
constraint on a hypothetical redesign, not a present defect. It is the finding that decides whether the
split is affordable; the answer is that it is not, on this schema, without the design D4 describes.

## D4 : what a billing split would need — necessary, not sufficient

Revision 1 called this "the smallest change set that makes it safe". **It does not make it safe**
(Tarzy #22). It is the minimum before the question can be asked.

1. **Move the metering cursor with the money.** `fleet.runner_affinity.metered_*` / `last_metered_at`
   (`630:55`–`:58`) and the lease mirror (`610:64`–`:67`) into the billing store; the fence stays with the
   lease. Note A1's wider key separates *events*; it does not identify a renewal *attempt* — the conflict arm
   is an accumulator (`renew.rs:139`), not a deduplicator. Idempotency across stores needs an attempt id.
2. **Replace the trigger with an application write** — giving up privilege containment and transactional
   counter maintenance (`890:6`–`:18`).
3. **Drop the three billing FKs** (C5), reintroducing an explicit cross-store erasure order.
4. **Add `tenant_id` to the four tables in D1** if events are ever to shard on the same axis.

Residual failures a design must answer before any of this ships: fleet-side success followed by
billing-side failure with no durable handoff; a delayed renewal crossing a reclaim or fresh-event cursor
reset (`lease/sql/lease.rs:49`, `:62`); terminal settlement and reclaim losing their common lock
(`report.rs:12`); ledger success followed by remote counter failure; erasure racing a delayed billing
write; and the operator read at `afd_fleet_ops/src/sql.rs:24` that reads the mirrors item 1 moves.

**Reconciliation is an investigation, not a procedure** (Tarzy #23). A ledger row accumulates in place
(`710:8`–`:15`): `last_charged_at` is its latest update and `credit_deducted_nanos` its running total, so
selecting rows updated in a window does not isolate money charged in that window — the budget drain itself
apportions across the run span (`afd_billing/src/sql.rs:52`). Deriving a wallet also needs its funding
history; `700:23` names the starter grant. The inputs exist; the procedure does not.

---

# E1 (HIGH) Continuation linkage has a present-day write race — found by the review, missed by revision 1

**Evidence.** `continue_from` (`afd_approval/src/inbox.rs:343`) first calls `admit()` (`:348`). Inside
`admit`, the entry is appended to the fleet's stream (`afd_admission/src/admit.rs:217`) and the fleet is
marked ready (`:277`) **before `admit` returns**. Only then does the approval path insert the narrative row,
binding the predecessor as `resumes_event_id` (`inbox.rs:364`–`:371`, `$7` = `event_id`).

A runner polling the now-ready fleet can lease the continuation first. Its `record_received` runs the same
`INSERT_FLEET_EVENT` with `resumes_event_id` bound to `Option::<&str>::None`
(`afd_fleet/src/lease/event.rs:113`). The approval path's later insert hits `ON CONFLICT (fleet_id, event_id)
DO NOTHING` (`afd_events/src/sql.rs:34`) and cannot restore the linkage. No UPDATE sets `resumes_event_id`
anywhere. The run continues correctly; the history loses the edge that says which run it continued — the
edge `inbox.rs:326` says the history is meant to keep.

**Status.** A source-derived interleaving, not a reproduced incident. It attacks the lineage requirement
more directly than anything in Stream C, and a composite FK on `resumes_event_id` would not prevent it — the
racing insert writes NULL, which any FK admits.

**Recommendation.** Reproduce first: two concurrent writers, one through `continue_from`, one through the
lease path, on the same admitted continuation. Then either carry `resumes_event_id` on the stream entry so
the lease path binds it, or move the approval path's narrative insert ahead of the append inside `admit` —
the second is the shape `core.fleet_admissions` already argues for (`910:8`–`:10`: the row is the acceptance,
the entry is a receipt).

---

# Conclusion

Revision 1 ended with one sentence: give the event id a type. Tarzy #26 is right that this is tidy and
wrong. A type addresses **representation** — the `f71a25fd9` class, A2's format half, A3's Rust half, and
the `EventId`-that-is-a-receipt naming hazard. It does nothing for the other four classes, which are
separate work:

- **Scope** — A1, and its three out-of-database dependents (`billing.ts:100`, `repair.rs:66`,
  `fleet_steer_events.ts:87`): the event id's uniqueness domain is undeclared.
- **Provenance** — A2's other half: nothing at lease proves the admission exists or matches.
- **Relationships** — C3, C4, B3: eight event-identifier columns the database does not check.
- **Transaction boundaries** — D3, D4, E1: what is atomic today, and the one place it already is not.

# High summary

| # | Finding | Evidence |
|---|---|---|
| A1 | Ledger `ON CONFLICT` target is global over an event id the schema says is per-fleet; three writers; held only by one undocumented sequence | `710:74`; `renew.rs:139`; `report.rs:187`; `afd_billing/src/sql.rs:136`; contradicted by `800:54` |
| A2 | Event id enters from a queue payload with an `is_empty` check; a non-parsing id silently skips its delivery stamp; nothing establishes provenance | `envelope.rs:160` vs `:165`; `lease/event.rs:209` |
| A3 | Logical id spelled in Rust and SQL against the crate's own rule; receipt decoded by an unguarded cast on the recovery path | `lib.rs:253`, `:259`; `sql.rs:126`, `:128` |
| A4 | `SELECT_DELIVERED_CURSOR` reads a fleet's whole admission history and sorts an expression | `sql.rs:121`–`:130`; `914:10` |
| A5 | `action_id` is a minted UUIDv7 in a TEXT column | `810:33`; `gate/sql.rs:100`; `approval_route.rs:135` |
| B1 | `checkpoint_id` has no writer and is published on every response | `800:43`; `afd_events/src/sql.rs:31`; `EventDetailsDialog.tsx:306` |
| B2 | `fleet_sessions.execution_id` — no writer, no reader (F06) | `510:29`; `lease/sql/fleet.rs:21` |
| B3 | Two orphan tables; one holds the only FK to `core.fleet_events` (F01) | `560:17`; `831:5`, `:19` |
| C2 | Two money-table columns with opposite retention semantics and no stated policy | `710:49`; `915:104` |
| C3 | Eight event-identifier columns unconstrained; the constrained one is on a dead table | `610:57`, `710:51`, `800:44`, `811:21`, `830:22`, `835:11`, `913:62` |
| C4 | `resumes_event_id` unconstrained | `800:44` |
| D2 | `fleet_admissions.seq` is the only thing holding A1's uniqueness; does not survive a shard | `910:68` |
| D3 | A billing split breaks the atomic meter-and-debit, the counter trigger, and the hot-path drain | `renew.rs:82`–`:139`; `890:87`; `720:37` |
| **E1** | **Continuation linkage race: a polling runner can write the narrative row with a NULL predecessor before the approval path does, and `DO NOTHING` cannot repair it** | `inbox.rs:348`, `:364`; `admit.rs:217`, `:277`; `lease/event.rs:113`; `afd_events/src/sql.rs:34` |

Critical: none. Medium: A6. Ship-blocking on this revision: nothing in the schema; E1 is the one item with
a present-day interleaving and should be reproduced before the next change to the approval path.
