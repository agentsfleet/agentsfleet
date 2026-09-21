<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M203_001: A continuation's history names what it resumed under every write order, and a ledger row accumulates only its own fleet's charges

**Prototype:** v2.0.0
**Milestone:** M203
**Workstream:** 001
**Date:** Sep 20, 2026
**Status:** DONE
**Priority:** P1 — operator-facing history loses lineage today under a real interleaving; the ledger key is a money invariant held by an undocumented sequence
**Categories:** API | CLI
**Batch:** B1 — standalone; no other M203 workstream
**Branch:** `feat/m203-continuation-lineage-and-ledger-key`
**Baseline revision:** `57a77c73cdfe30b66cc1a765da91a64406fb37f0`
**Test Baseline:** `unit=2644 integration=550` at the comparison revision — `make test-unit-all` (2644 Rust passed / 0 failed, plus app 2871 / design-system 142 / website 631 TypeScript) and `make test-integration-rustd`, whose 550 is derived rather than measured because its own build exhausted the disk: the branch's measured 561 less the 11 integration tests this diff adds and the 0 it removes. Branch: unit 2648 Rust / 2870 app TypeScript, integration 561, both lanes exit 0 — delta +4 unit, +11 integration, −1 TypeScript.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M203_001-57a77c73c.md`
**Depends on:** none. M202_001 is in flight on `fix/m202-close-and-acceptance-lanes` and edits `afd_approval/src/request.rs` and `afd_fleet/src/lease/{mint,deliver}.rs`; this spec edits `inbox.rs` and `lease/event.rs` — disjoint files, same crates; rebase onto whichever lands first.
**Provenance:** LLM-drafted (Claude Fable 5.1, Sep 20, 2026) from findings E1 and A1 of `docs/v2/reviews/identity-key-fk-shard-audit-2026-09-20.md`, revision 2
**Canonical architecture:** `docs/architecture/data_flow.md` §The five durable stores

---

## Overview

**Goal (testable):** a `core.fleet_events` continuation row carries its `resumes_event_id` whether the approval path or the lease path wrote the row first, and `billing.usage_ledger` refuses to merge two fleets' charges under one event id.
**Problem:** an operator opening a continuation in the event history can see no "resumes" link even though a human approved exactly that resumption — the link is lost when the runner leases the continuation before the approval path records it. Separately, the usage ledger's uniqueness key omits the fleet, so its correctness rests on one global sequence the schema never names as a billing guarantee and `schema/800_fleet_events.sql:54` contradicts.
**Solution summary:** the shared narrative insert converges on conflict instead of doing nothing — a predecessor known to any writer is kept — and first-delivery detection moves from "rows affected" to "did this statement insert", so classification is unchanged. A forward migration makes `fleet_id` part of the ledger's arbiter and `NOT NULL`; the three writers and the command-line charge grouping name the same scope. No stream, HTTP or response shape changes.

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(events): lineage survives the lease race; ledger key carries fleet`
- **Intent (one sentence):** a continuation always shows what it resumed, and a charge can only ever land on the row of the fleet that incurred it.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

Authoring assumptions, for the handshake to confirm: (1) not in production, schema rebuilds from empty, no backfill (Indy, Sep 20, 2026 — Discovery); (2) every live ledger writer binds a non-null `fleet_id` (`renew.rs`, `report.rs`, `afd_billing/src/sql.rs` all select it from the lease or fleet row), so `NOT NULL` is a fact, not a migration risk; (3) the logical event id keeps its `<millis>-<seq>` spelling — no newtype in this spec.

## Implementing agent — read these first

1. `rustd/crates/afd_events/src/sql.rs` — `INSERT_FLEET_EVENT` is the one statement both writers run; its `ON CONFLICT DO NOTHING` arm is what loses the predecessor, and its doc comment explains why it is shared.
2. `rustd/crates/afd_fleet/src/lease/event.rs` — `record_received`: first-delivery is derived from `rows_affected()`; the admission stamp runs on both arms. Mirror its shape; change only how "inserted" is read.
3. `rustd/crates/afd_approval/src/inbox.rs` — `continue_from`: `admit()` appends and marks the fleet ready before the narrative row is written. This ordering is the race and it stays; the insert converges instead.
4. `rustd/crates/afd_admission/src/sql.rs` — `INSERT_ADMISSION` already uses `ON CONFLICT DO UPDATE … RETURNING (xmax = 0) AS inserted` to learn on the conflict arm; the same idiom here.
5. `schema/915_usage_ledger_retains_fleet_identity.sql` — the nearest forward migration on the same table, and the reasoning style a slot on `billing.usage_ledger` carries.
6. `docs/SCHEMA_CONVENTIONS.md` — additive migrations resume after the rebuild; a destructive change needs an owner decision, recorded in Discovery.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_events/src/sql.rs` | EDIT | narrative insert converges on conflict and reports whether it inserted |
| `rustd/crates/afd_fleet/src/lease/event.rs` | EDIT | first-delivery read from the returned flag, not rows affected |
| `rustd/crates/afd_approval/src/inbox.rs` | EDIT | the same read for the frame-once and counters branches |
| `rustd/crates/afd_approval/tests/integration_inbox_continuation.rs` | EDIT | both write orders, and the redelivery-never-clobbers case |
| `schema/916_usage_ledger_fleet_scoped_key.sql` | CREATE | `fleet_id NOT NULL`; arbiter becomes `(event_id, charge_type, fleet_id)` |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | register slot 916 |
| `rustd/crates/afd_db/tests/migrations.rs` | EDIT | position assertion for the new slot |
| `rustd/crates/afd_db/tests/integration_ledger_identity.rs` | EDIT | constraint shape on fresh bootstrap and on an upgraded database |
| `rustd/crates/afd_fleet/src/lease/sql/renew.rs` | EDIT | conflict target |
| `rustd/crates/afd_fleet/src/lease/sql/report.rs` | EDIT | conflict target |
| `rustd/crates/afd_billing/src/sql.rs` | EDIT | conflict target on the receive insert |
| `rustd/crates/afd_fleet/tests/integration_ledger_scope.rs` | CREATE | two fleets, one event id, two rows; accumulate still converges per fleet |
| `rustd/crates/afd_wire/tests/schema_literals.rs` | DELETE | removed at Indy's direction at close (Discovery); its `mod` line in `wire_suite.rs` and the three comments naming it go with it |
| `rustd/crates/afd_events/src/history/statement.rs` | EDIT | doc comment names the new unique; no statement change |
| `cli/src/commands/billing.ts` | EDIT | charge summaries group by `(fleet_id, event_id)` |
| `cli/test/billing-effect.unit.test.ts` | EDIT | grouping keeps two fleets' same-id charges apart |
| `docs/architecture/data_flow.md` | EDIT | the ledger's uniqueness scope, in the durable-stores table and the partitioning note |
| `rustd/crates/afd_approval/tests/integration_inbox_tail.rs` | EDIT | splits by concern at the 350-line cap; keeps the announcement proofs |
| `rustd/crates/afd_approval/tests/integration_inbox_tail_continuation.rs` | CREATE | the continuation proofs, including the converged-announces-nothing one |
| `rustd/crates/afd_approval/tests/integration_inbox_tail_runless.rs` | CREATE | the runless proofs |
| `rustd/crates/afd_approval/tests/support/tail_watch.rs` | CREATE | the two reads and the spellings all three tail suites share |
| `rustd/crates/afd_approval/src/inbox/resolve.rs` | CREATE | `inbox.rs` splits at the 350-line cap on the read-versus-move seam |
| `rustd/crates/afd_wire/src/tenant.rs` | EDIT | `ChargeSummary.fleet_id` is not optional after slot 916 |
| `rustd/crates/afd_billing/src/tenant/mod.rs` | EDIT | `ChargeRow.fleet_id` likewise |
| `rustd/crates/afd_api_tenant/src/handler/tenant/billing.rs` | EDIT | the row-to-wire mapping loses its `map` |
| `rustd/crates/afd_api/tests/tenant_shape_parity.rs` | EDIT | the charge fixture carries a bare fleet |
| `rustd/crates/afd_fleet/tests/integration_ledger_reads.rs` | EDIT | the surviving-identifier assertion compares strings, not options |
| `ui/packages/app/lib/types.ts` | EDIT | the charges response item's `fleet_id` is `string` |
| `ui/packages/app/tests/billing-charges.test.ts` | EDIT | the three proofs of a charge with no fleet go |
| `ui/packages/app/lib/fleets/agent-label.test.ts` | EDIT | the composer-equivalence proof needs no null charge |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, NLR (each edited file loses any legacy it carries), UFS (the new constraint name and the `inserted` alias are named once), NSQ, STS (slot 916 names no application value), TST-NAM (no `m203` in test names — the existing `test_m201_*` names are the pattern NOT to copy), TCF (each new test is made red first: revert the converge arm, revert one conflict target), TVR (the only NULL `fleet_id` written is the refusal test), MIG (slot registered and its position asserted), ORP (the retired constraint name grepped out of every non-frozen file), ERR-RS (any new fallible signature).
- `dispatch/write_rust.md` — every `*.rs` edit.
- `docs/RUST_ERROR_STANDARD.md` — the crate error shape if a new error variant is needed; none is expected.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — slot 916 is a forward migration; shipped slots stay frozen.
- `dispatch/write_ts_adhere_bun.md` — the `cli/` edits.
- `docs/LOGGING_STANDARD.md` — no new log line is planned; applies if one is added.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — slot 916 carries `ALTER … SET NOT NULL`, `DROP CONSTRAINT`, `ADD CONSTRAINT` | forward migration, never an edit to 710; the destructive half is an owner decision recorded in Discovery before EXECUTE |
| RUST ERR | yes — `*.rs` edits | no hand-written error type; the `Result` alias per crate stays |
| UFS GATE | yes | constraint name and `inserted` alias declared once each and imported |
| MILESTONE-ID GATE | yes | no `M203`, `§`, or dimension numbers in source or test names |
| UI GATE / DESIGN TOKEN | yes for `cli/*.ts` | no rendering change; grouping key only |
| LOGGING | no new lines planned | if a converge-arm log is added, scoped event + error_code per the standard |
| GREPTILE | yes | rule IDs above are the pre-commitment |
| File & Function Length (≤350/≤50/≤70) | `inbox.rs` is at 405 already | the edit removes lines (one `rows_affected` branch becomes a flag read); no growth. If any touched file crosses 350, split before commit |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_admission/src/sql.rs` `INSERT_ADMISSION` — `ON CONFLICT DO UPDATE … RETURNING (xmax = 0) AS inserted` is the house idiom for "learn on the conflict arm"; §1 mirrors it exactly.
- **Reference:** `schema/915_usage_ledger_retains_fleet_identity.sql` — a forward migration on this table that changes a constraint and states its reasoning; §2 follows its shape. The 710 unique is named, so it is dropped by name.

## Sections (implementation slices)

### §1 — Continuation lineage converges, whichever writer lands first

Today `INSERT_FLEET_EVENT` is `ON CONFLICT (fleet_id, event_id) DO NOTHING`. `continue_from` calls `admit()`, which appends the entry and marks the fleet ready before returning; a polling runner can then run `record_received` first, binding no predecessor, and the approval path's later insert is a no-op. The link is gone and no UPDATE anywhere restores it.

The statement's conflict arm becomes a converge: `resumes_event_id` is set to the existing value if present, else the incoming one — never the reverse — and the statement returns whether it inserted. Both callers read that flag where they read `rows_affected()` today, so first-delivery, the receive debit, the once-only tail frame and the counters read behave exactly as before. The write order between approval and lease stays as it is; it simply stops mattering.

**Implementation default:** converge on the shared statement rather than carry the predecessor on the stream entry, because it touches three files, changes no wire type, and makes lineage a property of the row rather than of which process got there first. The entry-carried design is named in Decomposition as the refactor.

- **Dimension 1.1** DONE — lease first, approval second: the row ends with the predecessor set → Test `lineage_survives_lease_before_approval`
- **Dimension 1.2** DONE — approval first, lease second: the predecessor is kept and the second writer observes the conflict arm, as today → Test `lineage_kept_when_approval_writes_first`
- **Dimension 1.3** DONE — a redelivery with no predecessor never clears one already set → Test `a_redelivery_never_clears_lineage`
- **Dimension 1.4** DONE — only a fresh insert reports `inserted`; a converging write does not → Test `only_a_fresh_insert_reports_inserted`
- **Dimension 1.6** DONE — with the lease path's row already present, the approval path publishes no tail frame and moves no counter → Test `a_converged_continuation_announces_nothing`
- **Dimension 1.5** DONE — the five existing continuation proofs still pass unchanged → Test `integration_inbox_continuation`

### §2 — The ledger's arbiter carries the fleet

Slot 916: `billing.usage_ledger.fleet_id` becomes `NOT NULL`; `uq_usage_ledger_event_id_charge_type` is dropped by name; `uq_usage_ledger_event_id_charge_type_fleet_id UNIQUE (event_id, charge_type, fleet_id)` replaces it. NULL semantics are the reason `NOT NULL` is part of the change: PostgreSQL treats NULLs as distinct in a unique index, so a nullable `fleet_id` in the key would leave those rows unarbitrated. The slot carries the reasoning `710` cannot (it is frozen), and names `800:54` as the fact it reconciles.

All three writers change together: the renewal and report accumulate arms and the receive insert's `DO NOTHING`. The three targets are listed in Interfaces; a fourth writer with the old key fails at the constraint on its first run, since no matching unique exists after slot 916.

The events-page cost subselect binds both `fleet_id` and `event_id`, so the new index serves it; the executing agent captures one `EXPLAIN` of `SELECT_PAGE` against the compose lane in Session Notes showing the index name, as evidence rather than as a rubric gate.

**Implementation default:** `SET NOT NULL` refuses if a NULL row exists rather than deleting it, because the only database that can hold one is a developer's, and a migration that deletes money rows is the wrong reflex even there. The operator cleans and reruns.

- **Dimension 2.1** DONE — fresh bootstrap yields the composite unique, no old unique, and `fleet_id NOT NULL` → Test `ledger_key_shape_on_fresh_bootstrap`
- **Dimension 2.2** DONE — a database provisioned through 915 upgrades to the same shape → Test `ledger_key_shape_after_upgrade`
- **Dimension 2.3** DONE — two fleets charged under one event id string hold two rows, each with its own amounts → Test `same_event_id_two_fleets_two_rows`
- **Dimension 2.4** DONE — forty renewals on one fleet's event still accumulate into one stage row → Test `renewal_accumulates_per_fleet_event`
- **Dimension 2.5** DONE — a redelivered receive insert still writes nothing → Test `receive_insert_dedups_per_fleet_event`
- **Dimension 2.6** DONE — an insert with NULL `fleet_id` is refused → Test `ledger_refuses_null_fleet`

### §3 — Dependents move with the scope

The command-line charge renderer groups a tenant's rows by `event_id` alone; after §2 two fleets may legitimately share an id, so the group key becomes `(fleet_id, event_id)`. Rendered output is unchanged for every input the daemon produces today. `docs/architecture/data_flow.md` states the ledger's key twice as `(event_id, charge_type)`; both lines change, and the partitioning note gains one sentence saying the key is fleet-scoped.

Slot 916 also settles a type. `ChargeRow.fleet_id` and `ChargeSummary.fleet_id` were `Option`, documenting a charge written before slot 915 whose fleet the foreign key of the day had nulled. `NOT NULL` closes that state, and no database holds such a row — nothing is deployed and the schema rebuilds from empty — so the option described a row that cannot exist and the dashboard carried a `DELETED AGENT` cell no charge could render. The three types narrow together with the TypeScript response item. `agentDisplayName` KEEPS its absent-identifier arm: `AgentLabel` takes `fleetId: string | null` as its own prop, so leases, approvals and events still pass one.

- **Dimension 3.1** DONE — two rows, same `event_id`, different `fleet_id` → two summaries → Test `two fleets sharing one event id render as two rows`
- **Dimension 3.3** DONE — a charge's fleet is not optional on the wire, because slot 916 made the column `NOT NULL` → Test `a_charge_row_carries_its_whole_provenance`
- **Dimension 3.4** DONE — narrowing the charge keeps the shared label's absent-identifier arm, which leases and events still reach → Test `names a deleted fleet rather than deriving a callsign for it`

## Interfaces

```
afd_events::sql::INSERT_FLEET_EVENT
  ON CONFLICT (fleet_id, event_id) DO UPDATE
    SET resumes_event_id = COALESCE(core.fleet_events.resumes_event_id, EXCLUDED.resumes_event_id)
  RETURNING (xmax = 0) AS inserted
  -- binds unchanged: $1 fleet, $2 event, $3 workspace, $4 actor, $5 type, $6 body, $7 resumes, $8 now, $9 status

afd_fleet::lease::event::Leases::record_received(&Acquired, UnixMillis) -> Result<Received>
  -- signature and Received { delivery, counters } unchanged; `delivery` derives from `inserted`

billing.usage_ledger
  fleet_id UUID NOT NULL
  CONSTRAINT uq_usage_ledger_event_id_charge_type_fleet_id UNIQUE (event_id, charge_type, fleet_id)
  -- renew.rs, report.rs, afd_billing/src/sql.rs: ON CONFLICT (event_id, charge_type, fleet_id)

cli/src/commands/billing.ts groupRowsByEvent(rows) -> EventSummary[]
  -- key: `${fleet_id}\u0000${event_id}`; EventSummary shape unchanged

Stream entry fields, /v1/runners, /v1/tenants/me/billing/charges, event history responses: unchanged.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Lease wins the race | runner polls between `mark_ready` and the approval path's insert | row written with NULL predecessor; approval path's insert takes the converge arm and fills it; approval path sees `inserted = false`, publishes no second frame |
| Approval wins the race | the common case | row written with predecessor; lease insert takes the converge arm, keeps it, classifies as redelivery — exactly today's behaviour, preserved on purpose |
| Redelivery after lineage set | reclaim or replay | converge arm's `COALESCE` keeps the existing value; NULL never overwrites |
| Approval path dies after `admit`, before its insert | process crash in the window | pre-existing and out of scope here: the lease writes the row with no predecessor and nothing repairs it; recovery evidence is the gate row's own `event_id` (`schema/811`). Recorded in Discovery |
| Old binary against slot 916 | a replica not yet replaced runs `ON CONFLICT (event_id, charge_type)` | PostgreSQL refuses the statement (no matching unique); the renewal or report fails loudly and the runner retries. Dev-only exposure; the daemon applies migrations at boot, so binary and schema move together on a single deploy |
| NULL `fleet_id` write | a writer that lost its fleet | the money statement fails at the constraint, the lease is not extended, the error carries the column — never a silent row without attribution |
| Populated dev database holds a NULL `fleet_id` row | pre-915 history | `SET NOT NULL` refuses; migration fails loudly at boot; operator cleans the row and reruns. No automatic deletion |
| Colliding event ids across fleets | any future loss of global uniqueness | two rows, two fleets, two correct amounts — the outcome §2 exists for |

## Invariants

1. A known predecessor is never lost — enforced by the converge arm (`COALESCE(existing, incoming)`) on the one shared statement; proved by Dimensions 1.1–1.3 under both orders.
2. First-delivery classification is a property of "this statement inserted", unchanged in every case — enforced by `RETURNING (xmax = 0)` read at both callers; proved by 1.2 and 1.4.
3. A ledger row's arbiter includes its fleet and `fleet_id` is never NULL — enforced by the constraint and `NOT NULL`; proved by 2.3 and 2.6.
4. Every ledger writer names one arbiter — held by the constraint: after slot 916 the only unique on the table is the composite, so a writer naming the old target fails at its first `ON CONFLICT`. The source test that pinned the three spellings was removed at close (Discovery); 2.4 and 2.5 exercise the renewal and receive writers, and `integration_ledger_fleet_name.rs` drives the report writer's `CLAIM_AND_SETTLE`.
5. Rendered charge summaries never merge two fleets — enforced by the composite group key; proved by 3.1.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

The race's occurrence rate is not instrumented: the converge arm makes it harmless, and a counter for a race the design absorbs would be a number nobody acts on. Discovery records "Metrics review: no analytics/funnel playbook update required — internal correctness change, no user-visible event".

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `lineage_survives_lease_before_approval` | run `INSERT_FLEET_EVENT` for (F, X) binding no predecessor, then again binding P → the row's `resumes_event_id` = P, one row, second write reports `inserted = false` |
| 1.2 | integration | `lineage_kept_when_approval_writes_first` | the same two writes in the other order → `resumes_event_id` = P survives a writer that binds none, one row, second write reports `inserted = false` |
| 1.3 | integration | `a_redelivery_never_clears_lineage` | row with P set; three further writes binding None → still P, one row, every one reports `inserted = false` |
| 1.4 | integration | `only_a_fresh_insert_reports_inserted` | first write reports `inserted = true`; an identical second and a converging third both report `false` |
| 1.6 | integration | `a_converged_continuation_announces_nothing` | with the lease path's row already present for (F, X), `continue_from` publishes no `EventReceived` and leaves `events_processed` unmoved. The existing `a_second_answer_does_not_continue_the_run_again` does NOT cover this: its second resolve is stopped by the gate's `WHERE status = 'pending'` guard before it reaches `continue_from` |
| 1.5 | integration | `integration_inbox_continuation` | its five tests pass unchanged against the converged statement (regression); run of Sep 20, 2026 recorded all five green |
| 2.1 | integration | `ledger_key_shape_on_fresh_bootstrap` | `pg_constraint` holds `uq_usage_ledger_event_id_charge_type_fleet_id`, not the old name; `attnotnull` true for `fleet_id` |
| 2.2 | integration | `ledger_key_shape_after_upgrade` | apply slots through 915, seed one charge, apply 916 → same shape, row retained |
| 2.3 | integration | `same_event_id_two_fleets_two_rows` | two fleets in one tenant charged under one `event_id` string → two rows; each `credit_deducted_nanos` equals its own charge |
| 2.4 | integration | `renewal_accumulates_per_fleet_event` | forty renewals on (F, X) → one stage row; sum equals the forty deltas (regression) |
| 2.5 | integration | `receive_insert_dedups_per_fleet_event` | the receive insert twice for (F, X) → one row (regression) |
| 2.6 | integration | `ledger_refuses_null_fleet` | insert with NULL `fleet_id` → SQLSTATE 23502 naming `fleet_id` (negative) |
| 3.1 | unit | `two fleets sharing one event id render as two rows` | rows [(F1, X), (F2, X), (F1, X)] → two summaries; F1's carries two rows' totals |
| 3.3 | unit | `a_charge_row_carries_its_whole_provenance` | `ChargeSummary` is constructed with a bare `Cow` fleet and its field list is asserted unchanged; an `Option` spelling no longer compiles |
| 3.4 | unit | `names a deleted fleet rather than deriving a callsign for it` | `agentDisplayName(null)` still returns the deleted label and `AgentLabel fleetId={null}` still renders it — the arm the charge no longer reaches is still proven where a lease does |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Lineage survives both write orders (§1) | `cargo test -p afd_approval --test integration_inbox_continuation -- --include-ignored` | exit 0 | P0 | |
| R2 | Two fleets, one id, two rows; per-fleet accumulate intact (§2) | `cargo test -p afd_fleet --test integration_ledger_scope -- --include-ignored` | exit 0 | P0 | |
| R3 | Ledger key shape on fresh and upgraded databases (§2) | `cargo test -p afd_db --test integration_ledger_identity -- --include-ignored` | exit 0 | P0 | |
| R4 | No writer names the old arbiter | `grep -rn "ON CONFLICT (event_id, charge_type)" rustd/ cli/` | 0 matches | P0 | |
| R5 | Charge summaries group by fleet and event (§3) | `cd cli && bun test test/billing-effect.unit.test.ts` | exit 0 | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `make lint-all` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** A **deferral** leaves work unowned inside a closed spec. A **transfer** moves the criterion whole — Dimensions, verification and rubric row — into a named successor spec that carries it as its own P0. Mark such a row `MOVED to M{N}_{NNN} R{n}` only when all three hold: the successor exists and carries the row; both specs record the mapping; Discovery carries the owner's verbatim quote authorising it. A MOVED row is never rendered ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File | Why |
|------|-----|
| `rustd/crates/afd_wire/tests/schema_literals.rs` | removed at Indy's direction at close (Discovery); took four tests with it — the two ledger-key pins this spec added and the two `current_setting` pins from the grant-request milestone |
| `rustd/crates/afd_api/tests/http_plane_dependency_graph.rs` | text-matched `{crate}.workspace = true` to find a dependency cycle cargo refuses at build time; its own doc comment records that the roster went stale and missed the cycle it existed to catch |
| `rustd/crates/afd_bench/tests/bench_targets.rs` | asserted that `make/bench.mk` contains two literal strings, one of them help text with its double space; reformatting the makefile turned it red and nothing else |
| `rustd/crates/afd_wire/tests/manifest.rs` | hand-rolled TOML (`starts_with("default = [")`, `contains("optional = true")`); `cargo metadata` carries the same facts structurally, and `afd_core/tests/workspace.rs` already reads it that way |
| `rustd/crates/afd_events/tests/fleet_event_writers.rs` | `is_a_use` — a `starts_with("//")` filter standing in for a parse, walking all 932 files under `crates/*/src`. The roster it guarded is a visibility question, recorded below as follow-on work |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `uq_usage_ledger_event_id_charge_type` | `grep -rn "uq_usage_ledger_event_id_charge_type" rustd/ cli/ ui/ docs/architecture/ \| head` | 0 matches (the name survives only in frozen `schema/710` and `schema/916`'s drop, and in `docs/v2/reviews/`) |
| `schema_literals` | `grep -rn "schema_literals" rustd/ --include="*.rs" --include="*.toml"` | 0 matches (`docs/v2/` history keeps its citations) |
| `http_plane_dependency_graph`, `bench_targets`, `fleet_event_writers` | `grep -rn "http_plane_dependency_graph\|bench_targets\|fleet_event_writers" rustd/ --include="*.rs" --include="*.toml"` | 0 matches; the three `mod` declarations in `http_substrate_suite.rs`, `wire_suite.rs` and `events_suite.rs` went with them |
| `test_m201_index_comment_names_surviving_reader` | `grep -rn "test_m201_index_comment" rustd/` | 0 matches — it asserted the wording of a comment in `schema/720`, and carried the milestone-prefixed name this spec's Applicable Rules names as the pattern not to copy |

## Out of Scope

- A typed logical event id and any provenance check at lease (audit A2) — its own spec; this one keeps `String`.
- `SELECT_DELIVERED_CURSOR`'s scan and the receipt cast (A3, A4); `action_id`'s column type (A5); `checkpoint_id` and `execution_id` removal (B1, B2); the orphan tables (B3); `usage_ledger.workspace_id`'s retention policy (C2); FKs on the other event-identifier columns (C3, C4).
- The repair branch name (`afd_gate/src/policy/repair.rs`) encoding only the event id — unchanged; a cross-fleet collision on one repository is impossible while the admission sequence is global, and the branch is M202's surface.
- The crash window between `admit()` returning and the approval path's insert — pre-existing; recorded in Discovery with its recovery evidence.
- The pre-existing classification of a continuation as a redelivery when the approval row lands first — preserved unchanged here and recorded in Discovery as a separate finding.
- Carrying the predecessor on the stream entry — the refactor named in Decomposition, not taken.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator expands a continuation in the event history and reads "resumes `<event>`" — the event a human unblocked — no matter how fast a runner picked the continuation up.
2. **Preserved user behaviour** — approving a gate continues the run exactly once; the charges page and `agentsfleet billing` show the same rows and totals; every existing continuation and money proof stays green.
3. **Optimal-way check** — the unconstrained shape carries lineage on the admission row and the stream entry so the lease path binds it itself; the converge arm delivers the moment with three files and no wire change. The gap: lineage stays a property of two writers rather than of the acceptance. Acceptable now; the refactor is named below.
4. **Rebuild-vs-iterate** — patch. A typed event id and entry-carried lineage are each right in the long game and each is a separate spec; folding them here would trade a three-file fix for a twenty-crate one.
5. **What we build** — a converge arm plus a returned flag; slot 916; three conflict targets; a group key; two doc lines; the tests above.
6. **What we do NOT build** — the newtype (A2); the recovery-scan work (A3, A4); any UI; a metric for a race the design absorbs; entry-carried lineage.
7. **Fit with existing features** — compounds with M202's repository-write identity, which names branches by event id; must not destabilise `RENEW_AND_METER` / `CLAIM_AND_SETTLE`, which keep their single-statement atomicity — only their conflict target changes.
8. **Surface order** — N/A — no new user surface; the CLI change is a grouping key with identical output on today's data.
9. **Dashboard restraint** — N/A — no UI change.
10. **Confused-user next step** — N/A — no user surface; an operator who sees a continuation without a "resumes" link after this ships has hit the crash window in Out of Scope, and the gate row's `event_id` is where to look.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 fixes the present-day defect at the one shared statement; §2 is the money-key migration with its writers and its proof; §3 is the two dependents that must move with the scope. Ordered E1 then A1 per Indy; §1 and §2 are independent and can land in either order.
- **Alternatives considered:** (a) carry `resumes_event_id` on the stream entry so the lease path binds it — the principled shape (`schema/910:8`–`:10`: the row is the acceptance) but touches `afd_wire::event::Entry`, the admission row, replay, the envelope decoder and the lease bind; rejected for now as the refactor behind a one-statement fix. (b) derive first-delivery from the admission stamp's `rows_affected` instead of the narrative insert — realigns authority with `core.fleet_admissions.delivered_at`, but reclassifies every non-admitted id (the benchmark's) and changes money semantics; rejected. (c) widen the ledger key without `NOT NULL` — rejected: NULLs would escape the arbiter.
- **Patch-vs-refactor verdict:** this is a **patch** because each defect has a one-site fix with a mechanical proof, and the refactors (typed id, entry-carried lineage) are named as follow-up specs rather than mud-patched here.

## Discovery (consult log)

- **Consults** — Source: `docs/v2/reviews/identity-key-fk-shard-audit-2026-09-20.md` revision 2, findings E1 and A1, with Tarzy's adversarial dispositions (#4 third writer, #25 the race). Authorisation to author: > Indy (2026-09-20): "Yes i agree - open spec from High table (first two, skip the third tarzy's round)" — context: the review's High table; E1 first, then A1. **Required human decision, GRANTED:** slot 916 drops a constraint on the money table (`docs/SCHEMA_CONVENTIONS.md`: destructive changes need an explicit owner decision per change), and the authorisation to open the spec did not carry it. Asked as decision D3 — drop and replace the arbiter, plus `fleet_id NOT NULL` — and answered by > Indy (2026-09-20): "D3 - Okay go ahead" — context: the ask named the `DROP CONSTRAINT`, the `ADD CONSTRAINT` and the `SET NOT NULL` as one migration file, and recorded that slot 915 had already dropped a constraint on this same table with his approval while `docs/SCHEMA_CONVENTIONS.md:11` requires the decision per change. Architecture consult: `docs/architecture/data_flow.md` names the ledger key as `(event_id, charge_type)` at two lines; this spec reconciles the doc in §3 rather than diverging from it silently. Pre-existing findings surfaced while authoring, not fixed here: (i) a continuation whose approval row lands before the lease is classified a redelivery and skips the receive row — behaviour preserved; (ii) the crash window between `admit()` and the approval path's insert.
- **Metrics review** — no analytics/funnel playbook update required: internal correctness change, no user-visible event.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` per Section and at the boundary; `/review`; `orly-babysit-prs` after push.
- **Removed at close** — `rustd/crates/afd_wire/tests/schema_literals.rs`, with Dimensions 2.7 and 3.2 and their Test Specification rows: > Indy (Sep 21, 2026): "I want schema_literals.rs to be removed. Its full of crap." — context: the file had grown from two `current_setting` pins into a grep-over-source home for any literal the schema spells; the ledger-key pins this spec added were the third and fourth. Consequence recorded, not fixed here: the eight `current_setting('fleet.allow_gate_purge')` literals in `schema/` are now unpinned, which RULE STS names as the case for a pin; the follow-on lifecycle milestone removes the setting itself, after which there is nothing to pin.
- **Text-scan sweep, added at close by owner direction.** > Indy (Sep 21, 2026): "can you find out such patters like is_a_use, i think this must be nuked." and "i want to clean up as part of this PR" — context: `is_a_use` in `fleet_event_writers.rs`, a `starts_with("//")` filter standing in for a parse. Eleven grep-over-source tests were audited; four files and one assertion were removed where a compiler, `cargo metadata`, or an existing structured test already carries the claim. Measured cost of the whole family before removal: ~0.8s against a 1m31s `cargo test --workspace`, so speed was not the reason — the heuristics were. Kept, with reasons: `openapi_artifact.rs` and `workspace.rs` (structured, not heuristics), `scope_catalogue.rs` (13 of 14 tests are typed set membership), `names.rs` (utoipa merges duplicate component names silently and the generator cannot report what it merged, so no other guard exists — its hand parser should become a check on the generated document).
- **Follow-on, not fixed here** — the `INSERT_FLEET_EVENT` writer roster is now unenforced. The principled guard is visibility: make the constant private to `afd_events` and expose a function, so the compiler bounds the caller set. That is a production change across three crates and belongs with the lifecycle milestone.
- **Deferrals** — none.
