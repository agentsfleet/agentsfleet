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

# M206_001: An answer is owed only to the thread its question came from, and an answer nobody can take is abandoned

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 001
**Date:** Sep 23, 2026
**Status:** DONE
**Priority:** P0 — no connector answer can reach its thread, and every non-empty answer from every producer is owed to a model provider and re-offered to the queue without end.
**Categories:** API, SQL
**Batch:** B1 — first on the critical path, beside M206_003 §1 and §3 and M206_004 §1–§2; M206_002 adds the only producer that records a destination, and an approval continuation inherits one wherever a fleet's own gate rules park an event.
**Branch:** feat/m206-slack-incident-responder
**Baseline revision:** f3edd3c17062087a6db7f7e271606bf0c3901259
**Test Baseline:** `unit=2662 integration=572` at the comparison revision, measured on an isolated checkout of it (Rust unit 2662 passed / 0 failed / 592 ignored; integration 571 + 1 exclusive, 0 failed; TypeScript app 2976 · website 142 · cli 1755 · design-system 634).
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M206_001-f3edd3c17.md`
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4` and the retired Zig daemon at `1ad07eb2` in `~/Projects/oss/zig/agentsfleet_zig`
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §6; `docs/architecture/data_flow.md` §"C. EXECUTE"

---

## Overview

**Goal (testable):** a settled report owes a delivery only when its event, or the event an approval continuation resumes, carries a producer-recorded reply destination; the obligation names that destination's connector and address and never the lease's model provider; an obligation its destination permanently refuses is abandoned and never re-offered.

**Problem:** a person who asks a fleet something from a connector thread never gets the answer there, and the delivery queue never drains. The report path owes every non-empty answer, from a cron sweep, a steer or a GitHub review alike, to `lease.provider` (`rustd/crates/afd_fleet/src/lease/commit.rs:198-210`, `report/steps.rs:114-120`), which is the model provider resolved at billing (`schema/610_runner_leases.sql:35-39,62`). The worker cannot parse `anthropic` as a connector and drops the job as permanent (`rustd/crates/afd_outbound/src/poster.rs:77-92`). Only a delivered verdict stamps the row (`afd_outbound/src/lanes.rs:223-228`), so the undelivered scan re-appends it every 300 seconds (`obligation/sql.rs:111-116`, `producer.rs:59-66`) for the life of the fleet, and a genuinely lost answer queues behind them. Every outbound suite seeds `provider = "slack"` by hand (`afd_outbound/tests/integration_obligations.rs:66`) and no suite drives a report into the ledger, which is how it shipped. The retired Zig daemon read the provider from the fleet's channel binding and owed nothing for an unbound fleet (`src/agentsfleetd/fleet/service_report_outbound.zig:25-46` at `1ad07eb2`).

**Solution summary:** the admission row gains a nullable reply destination, a connector id plus an opaque address, written only by a producer that owns a reply surface; the six existing producers state none. An approval continuation copies the destination of the event it resumes. The report transaction reads the destination of the event it settles and owes a delivery only when one exists, with the provider typed as `afd_connector::Provider`, so the model provider cannot be passed. The obligation keeps the address and the Slack poster reads it from the job rather than re-reading `core.fleet_events`. A permanent verdict, or a delivery that exhausts its cycle budget, marks the obligation abandoned; both recovery scans skip abandoned and destination-less rows, which stops the rows already in the ledger from cycling without a data migration.

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(outbound): an answer is owed to its reply destination, never to the model provider`
- **Intent (one sentence):** an answer goes back to the thread that asked for it, and an answer with nowhere to go stops costing the queue anything.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/commit.rs` — the report transaction; the owe is its fifth write and the destination read joins the same transaction.
2. `rustd/crates/afd_outbound/src/obligation.rs` — the single owner of `core.fleet_obligations` and why the statements live there; `obligation/sql.rs` holds both scans.
3. `rustd/crates/afd_admission/src/lib.rs` — `Admission`, `Producer`, the payload digest; a new field here is stated by every producer at compile time.
4. `rustd/crates/afd_approval/src/inbox/resolve.rs` — `continue_from` admits a continuation with body `{}`; it must carry the resumed event's destination.
5. `docs/architecture/scenarios/slack-incident-responder.md` — §6 is the design this implements; §10 names what is broken today.
6. `docs/RUST_ERROR_STANDARD.md` — every fallible signature touched here follows it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/918_fleet_admissions_reply_destination.sql` | CREATE | Nullable `reply_provider` and `reply_address` on `core.fleet_admissions`, both-or-neither check, and a lookup index partial on the destination. |
| `schema/919_fleet_obligations_destination.sql` | CREATE | `destination` on `core.fleet_obligations`. |
| `schema/920_fleet_obligations_abandonment.sql` | CREATE | `abandoned_at`, `abandon_reason`; both scan indexes rebuilt to exclude abandoned and destination-less rows. One concern per slot, so the destination (§2) and abandonment (§4) are two. |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | Registers 918, 919 and 920. |
| `rustd/crates/afd_admission/src/{lib.rs,admit.rs,sql.rs,tests.rs}` | EDIT | `Admission` carries a `Reply` — none, stated, or inherited; the insert writes or copies it; the digest covers it; `SELECT_REPLY_DESTINATION` returns it. |
| `rustd/crates/afd_events/src/steer.rs` · `afd_cron/src/fire.rs` · `afd_ingress/src/deliver.rs` · `afd_runner/src/sweep/repair.rs` | EDIT | Each existing producer states `Reply::None`. |
| `rustd/crates/afd_approval/tests/integration_inbox_tail_continuation.rs` · `afd_events/tests/integration_budgets.rs` · `afd_fleet/tests/{integration_cluster_rebuild.rs,integration_recovery_outage.rs,integration_admission_recovery.rs,support/fleet_recovery_seed.rs}` · `agentsfleetd/tests/support/e2e_event.rs` | EDIT | Test admissions state `Reply::None`. |
| `rustd/crates/afd_fleet/tests/integration_reply_destination.rs` · `afd_fleet/tests/fleet_suite.rs` | CREATE · EDIT | §1's proofs against the live ledger. |
| `rustd/crates/afd_approval/src/inbox/resolve.rs` | EDIT | The continuation states `Reply::Inherit` with the resumed event's id. |
| `rustd/crates/afd_fleet/src/lease/{commit.rs,obligation.rs,mod.rs,report.rs,report/steps.rs}` · `afd_fleet/Cargo.toml` | EDIT | The report reads the destination in its transaction and owes only with one; `Committed::Settled` carries an `Owing`; `Reported.provider` feeds metering only; `afd_connector` becomes a direct dependency. |
| `rustd/crates/afd_fleet/tests/integration_report_commit.rs` | EDIT | A charged report over an event with no destination owes nothing. |
| `rustd/crates/afd_outbound/src/{obligation.rs,obligation/sql.rs,producer.rs,lanes.rs,poster.rs,slack.rs,worker.rs}` | EDIT | Typed provider and destination on `Delivery`/`Owed`; scans skip abandoned and destination-less rows; abandon on a permanent or exhausted verdict; the poster reads the address from the job. |
| `rustd/crates/afd_outbound/src/abandon.rs` · `src/lib.rs` | CREATE · EDIT | The abandon stamp and its event, shared by the lanes (a queued job) and the producer (a scanned row), keeping `lanes.rs` under the cap; the event's fields are a type with no field for the answer or the address. |
| `rustd/crates/afd_dragonfly/src/{outbound.rs,outbound/reader.rs}` | EDIT | The queue entry carries the destination; an entry without one is dropped as undecodable. |
| `rustd/crates/agentsfleetd/src/outbound.rs` | EDIT | The poster is built without a pool: it reads no event row. |
| `rustd/crates/afd_bench/src/lane/outbound.rs` · `outbound/poster.rs` · `outbound/tests.rs` · `outbound/poster/tests.rs` | EDIT | Bench jobs carry a destination. |
| `rustd/crates/afd_outbound/tests/{integration_obligations.rs,integration_attempt_count.rs,integration_worker.rs,integration_producer_outage.rs,integration_slack_poster.rs,delivery.rs,lanes.rs,lane_sharing.rs,ledger_faults.rs,read_backoff.rs,support/gated_poster.rs,support/obligation_seed.rs}` | EDIT | Seed a destination; add the abandonment cases. |
| `rustd/crates/afd_fleet/tests/integration_report_owes_destination.rs` | CREATE | Report to ledger, end to end, for three producers. |
| `docs/architecture/data_flow.md` · `docs/architecture/scenarios/slack-incident-responder.md` | EDIT | Drop the "today" paragraph; mark §10's delivery row shipped. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — STS (both-or-neither is a NULL-test check with no literal; no provider spelled in SQL), NSQ, SGR (918/919 carry grants unchanged), UFS (reasons and events are named constants), ECL (permanent vs retryable vs abandoned stay distinct), IDMP (a replayed report owes once), ORP (the removed event-table read), TST-NAM, LOG, ERR-RS, FLL.
- `docs/RUST_ERROR_STANDARD.md` — new fallible signatures in `afd_admission`, `afd_outbound`, `afd_fleet`.
- `dispatch/write_rust.md` RULE FN-RS with `M-STRONG-TYPES` and `M-STRONG-TYPES-GUARD` — the destination is one `Option<ReplyDestination>`, never two nullable strings; the provider is `afd_connector::Provider` from the ledger read to the poster, and at admission a `&'static str` that only a constant such as `Provider::id()` supplies, so a model provider fails to compile at either end while `afd_admission` stays free of `afd_vault` (`cargo tree`: `afd_events`, `afd_cron`, `afd_runner` and `afd_approval` do not link it); the abandon reason and the delivery outcome are enums. RULE PSR — a workspace helper or a `[workspace.dependencies]` crate wins over a new function; a new helper says in its PR why neither fits.
- `docs/SCHEMA_CONVENTIONS.md` — forward, additive, single-concern slots of at most 100 lines.
- `docs/LOGGING_STANDARD.md` — `outbound_delivery_abandoned` carries reason and count, never the answer or the address.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — two `ALTER TABLE` slots | `SCHEMA GUARD: VERSION=0.49.0 (migrate)`; additive columns and index rebuilds only, no data rewrite. |
| LOGGING / UFS | yes | Events and reasons are module constants; no answer text or address in any field. |
| MILESTONE-ID | yes | No milestone identifiers in source or test names. |
| File & Function Length (≤350/≤50/≤70) | yes — `lanes.rs` is 320 lines | The abandon step lands in `lanes/` beside `retire.rs` rather than growing `lanes.rs`. |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/fleet/service_report_outbound.zig` at `1ad07eb2` in `~/Projects/oss/zig/agentsfleet_zig` — owed only for a bound fleet, with the connector's provider. Divergence: the destination comes from the event's producer, not a fleet-level table, so a fleet bound to a channel can still run a steer without owing Slack anything.
- **Reference:** `rustd/crates/afd_admission` — the ledger-first pattern (row, then receipt) this extends; M198's attempt counting in `afd_outbound/src/obligation/sql.rs` stays as written.

## Sections (implementation slices)

### §1 — A producer records where an answer goes

The admission gains a nullable pair: `reply_provider`, a connector id, and `reply_address`, an address only that connector's poster reads. Only a producer owning a reply surface writes it; in this milestone there is none, and M206_002's `slack_mention` is the first. The existing producers state `Reply::None` in the struct literal, so a producer added later cannot forget the question. The payload digest covers the pair. **Implementation default:** keep the destination on `core.fleet_admissions` only, because the report finds that row by the logical event id the delivery stamp already uses; a copy on `core.fleet_events` would be a second row that could disagree. `continue_from` states `Reply::Inherit` with the resumed event's id, and the insert copies that row's pair in the same statement, so no read races the write. The lookup rides a new index partial on `reply_provider IS NOT NULL`: the settled event was delivered, so neither index partial on `delivered_at IS NULL` covers it.

- **Dimension 1.1** DONE — the steer, webhook, App webhook, schedule and repair-verification producers admit rows with no destination → Test `existing_producers_record_no_reply_destination`
- **Dimension 1.2** DONE — a row with both halves round-trips; a row with one half is refused by the check → Test `reply_destination_is_both_or_neither`
- **Dimension 1.3** DONE — an approval continuation carries its resumed event's destination; resuming an event with none yields none → Test `continuation_inherits_the_resumed_destination`
- **Dimension 1.4** DONE — a retried producer key whose destination differs keeps the first admission and logs drift → Test `retried_key_with_a_new_destination_keeps_the_first`

### §2 — The report owes a delivery only to a recorded destination

Inside the report transaction, after the settle and before commit, the owe step reads the settled event's destination. None, or an event id that is not a ledger id: nothing is owed, whatever the answer. Present: the stored provider is parsed with `Provider::parse`; an unknown id owes nothing and logs `report_reply_provider_unknown`. `Delivery.provider` becomes `afd_connector::Provider`, so `lease.provider` has no type-compatible path into the ledger.

- **Dimension 2.1** DONE — non-empty answers from a steer, an App webhook and a cron fire owe nothing → Test `answers_without_a_destination_owe_nothing`
- **Dimension 2.2** DONE — an event whose admission names `slack` and an address owes one row with provider `slack` and that exact address → Test `answer_is_owed_to_the_recorded_destination`
- **Dimension 2.3** DONE — with the lease resolved to provider `anthropic`, no ledger row names it → Test `the_model_provider_never_reaches_the_ledger`
- **Dimension 2.4** DONE — a replayed report owes one row; a report whose transaction rolls back owes none → Test `a_replayed_or_rolled_back_report_owes_at_most_once`

### §3 — The poster posts from the obligation's address

`OutboundJob` and `OutboundDelivery` carry the destination, and the producer's requeue passes the stored one. The Slack poster parses `{team_id, channel_id, thread_ts}` from the job and no longer queries `core.fleet_events`; an address missing a field is a permanent verdict before any request is built.

- **Dimension 3.1** DONE — the poster posts to the job's channel and thread → Test `poster_posts_to_the_jobs_address`
- **Dimension 3.2** DONE — an address missing `channel_id` or `thread_ts` is permanent with zero HTTP calls → Test `unreadable_address_is_permanent_without_a_request`
- **Dimension 3.3** DONE — an unreceipted and an undelivered row are re-appended carrying their stored destination → Test `requeued_obligation_keeps_its_destination`

### §4 — An answer nobody can take is abandoned

A permanent verdict stamps `abandoned_at` and a named reason before the acknowledgement. A retryable verdict stays re-offerable until `attempt_count` reaches `MAX_DELIVERY_CYCLES`, then the same stamp applies. **Implementation default:** `MAX_DELIVERY_CYCLES = 12`, a compile-time-asserted constant beside `LOST_AFTER`. Both scans require a destination and no abandonment, so rows written before this change are never re-offered and leave with their fleet by cascade.

- **Dimension 4.1** DONE — a permanent verdict abandons the row and a pass past `LOST_AFTER` does not re-offer it → Test `permanent_refusal_abandons_the_obligation`
- **Dimension 4.2** DONE — a destination failing retryably is re-offered until the cycle cap, then abandoned → Test `exhausted_cycles_abandon_the_obligation`
- **Dimension 4.3** DONE — destination-less rows are skipped by the unreceipted and the undelivered scan → Test `legacy_rows_are_never_reoffered`
- **Dimension 4.4** DONE — abandonment emits `outbound_delivery_abandoned` once, with reason and count and no answer or address → Test `abandonment_is_logged_once_without_content`

## Interfaces

```
core.fleet_admissions  + reply_provider TEXT NULL, reply_address TEXT NULL
                         CHECK ((reply_provider IS NULL) = (reply_address IS NULL))
core.fleet_obligations + destination TEXT NULL (919), abandoned_at BIGINT NULL, abandon_reason TEXT NULL (920)
  owed set  = receipt IS NULL     AND destination IS NOT NULL AND abandoned_at IS NULL
  lost set  = receipt IS NOT NULL AND delivered_at IS NULL AND destination IS NOT NULL AND abandoned_at IS NULL

afd_admission::Admission { …, reply: Reply<'a> }
  Reply::None | Reply::To { connector: &'static str, address: &'a str } | Reply::Inherit { event_id: &'a str }
idx_fleet_admissions_reply_lookup ON core.fleet_admissions (fleet_id, created_at, seq) WHERE reply_provider IS NOT NULL
afd_outbound::obligation::Delivery { fleet_id, workspace_id, provider: afd_connector::Provider,
                                     destination: &str, event_id, answer }
afd_dragonfly::{OutboundJob, OutboundDelivery} gain `destination`

Slack address (opaque outside the Slack poster; channel_id and thread_ts required, other keys ignored):
  {"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}
OutboundJob → From<Delivery> (one conversion for the report's append and the producer's re-append)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Crash between commit and append | process death | Row unreceipted; the producer scan re-appends it with its destination; the thread receives the answer late. |
| Unreadable address | producer bug or a hand-edited row | Permanent before any request; abandoned with reason `refused`; the poster logs `slack_post_address_unreadable`. |
| Channel archived or bot removed | Slack answers `ok:false` | Permanent; abandoned with reason `refused`; never re-offered. |
| Vendor outage across cycles | 429 or 5xx | Retryable; re-offered after `LOST_AFTER`; abandoned at the cycle cap with reason `cycles_exhausted`. |
| Unknown stored provider | tampering or a removed connector | Report owes nothing and logs `report_reply_provider_unknown`; the run's result and charge stand. |
| Continuation of an unknown event | lineage race | No destination; nothing owed. |
| Abandon stamp fails | datastore | Reported as `outbound_obligation_abandon_failed`; the job is still acknowledged; the row stays in the lost set and is re-offered after the window (the at-least-once direction). |
| Stored connector names no connector | a connector removed from the catalogue, or an out-of-band edit | The producer abandons the row with reason `unaddressable` rather than skip it, so a batch of such rows cannot fill `BATCH_LIMIT` and starve the answers behind it. |
| Acknowledgement lost after delivery | at-least-once | The thread may show the answer twice; recorded, not prevented. |

## Invariants

1. A ledger `provider` is a connector id — `Delivery.provider: afd_connector::Provider`, written through `Provider::id()`; `Reported.provider` has no path into `owe` (test 2.3).
2. No destination, no obligation — the owe call takes a destination value, not an `Option`; the report branches once on the read (test 2.1).
3. An abandoned or destination-less row is never re-offered — both scan predicates and both partial indexes carry the two NULL tests (tests 4.1, 4.3).
4. A reply pair is both-or-neither — the named check in slot 918 (test 1.2).
5. A continuation's destination equals its resumed event's — copied inside the continuation's own insert (test 1.3).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `outbound_delivery_abandoned` | ops | an obligation is abandoned | provider, fleet_id, reason, attempt_count | no answer text, no address | `abandonment_is_logged_once_without_content` |
| `report_reply_provider_unknown` | ops | a stored provider does not parse | fleet_id, agentsfleet_event_id | no address | `answer_is_owed_to_the_recorded_destination` |
| `outbound_unknown_provider` (existing) | ops | no longer fires for a report-owed job | unchanged | unchanged | `the_model_provider_never_reaches_the_ledger` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `existing_producers_record_no_reply_destination` | Each of the five producers without a reply surface, admitted with `Reply::None`, has no destination under the shipped lookup; every call site states its `Reply` because the field has no default. |
| 1.2 | integration | `reply_destination_is_both_or_neither` | `(slack, address)` round-trips; `(slack, NULL)` fails the named check. |
| 1.3 | integration | `continuation_inherits_the_resumed_destination` | Continuations inheriting from an event admitted with `(slack, A)`, one admitted with none, and an id the ledger never minted carry `(slack, A)`, none, and none. |
| 1.4 | integration | `retried_key_with_a_new_destination_keeps_the_first` | Re-admitting a key with address B returns the first event and keeps A; the stored digest differs from the retry's, the condition the drift warning fires on. |
| 2.1 | integration | `answers_without_a_destination_owe_nothing` | Reports carrying `"done"` for a steer, an App webhook and a cron fire leave zero rows in `core.fleet_obligations`. |
| 2.2 | integration | `answer_is_owed_to_the_recorded_destination` | A report for an event admitted with `(slack, A)` writes one row: provider `slack`, destination `A`. |
| 2.3 | integration | `the_model_provider_never_reaches_the_ledger` | With lease provider `anthropic`, `SELECT count(*) … WHERE provider = 'anthropic'` is 0 after reports from three producers. |
| 2.4 | integration | `a_replayed_or_rolled_back_report_owes_at_most_once` | A report the datastore refuses leaves no row; its retry leaves one; a repeat of the retry leaves one. |
| 3.1 | integration | `poster_posts_to_the_jobs_address` | A loopback Slack receives `{channel:"C0123456789", thread_ts:"1700000000.000100", text}` from the job alone, with no `core.fleet_events` row present. |
| 3.2 | unit | `unreadable_address_is_permanent_without_a_request` | Addresses lacking either field, empty strings, and non-JSON give `Permanent`; the fake records zero requests. |
| 3.3 | integration | `requeued_obligation_keeps_its_destination` | After the queue loses an entry, the re-appended job's destination equals the row's. |
| 4.1 | integration | `permanent_refusal_abandons_the_obligation` | `ok:false` stamps `abandoned_at` and reason; a scan with the cutoff past `LOST_AFTER` returns nothing. |
| 4.2 | integration | `exhausted_cycles_abandon_the_obligation` | A 503-only destination is re-offered on successive passes until the count reaches the cap, then abandoned with reason `cycles_exhausted`. |
| 4.3 | integration | `legacy_rows_are_never_reoffered` | Rows seeded with NULL destination, receipted and not, are absent from both scans at any cutoff. |
| 4.4 | unit | `abandonment_is_logged_once_without_content` | One structured event with provider, fleet, reason and count; the captured record contains neither the answer nor the address. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The report path cannot address a delivery with the lease's provider (§2) | `grep -rn 'provider: &lease.provider' rustd/crates/afd_fleet/src/lease \| wc -l` | `0` | P0 | |
| R2 | Both recovery scans skip abandoned and destination-less rows (§4) | `grep -c 'destination IS NOT NULL AND abandoned_at IS NULL' rustd/crates/afd_outbound/src/obligation/sql.rs` | `2` | P0 | |
| R3 | The Slack poster reads no event row (§3) | `grep -c 'core.fleet_events' rustd/crates/afd_outbound/src/slack.rs` | `0` | P0 | |
| R4 | The three slots are registered (§1, §2, §4) | `grep -cE '9(18\|19\|20)_fleet_' rustd/crates/afd_db/src/migration.rs` | `3` | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R6 | Patch coverage meets the repository bar | `gh pr checks --json name,state --jq '.[] \| select(.name\|startswith("codecov/patch")) \| .state'` | every line `SUCCESS` — `rust-afd` at 99% of added lines (`codecov.yml`, threshold 0%) | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery; a P0 may be **MOVED** only under `docs/TEMPLATE.md`'s three conditions.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `SELECT_EVENT_REQUEST` | `grep -rn "SELECT_EVENT_REQUEST" rustd/crates/afd_outbound/src \| head` | 0 matches |
| `REASON_EVENT_LOAD_FAILED` | `grep -rn "REASON_EVENT_LOAD_FAILED" rustd/crates/afd_outbound/src \| head` | 0 matches |

## Out of Scope

- The Slack mention producer and every routing decision — M206_002.
- GitHub evidence and the Slack write reach — M206_003.
- Deleting the destination-less rows already in the ledger: they stop cycling here and leave with their fleet by cascade.
- A dashboard surface for abandoned answers; the structured event is the operator signal until someone asks for more.

---

## Product Clarity (authoring record)

1. **Successful user moment** — someone asks in a Slack thread and the answer lands in that thread; on a deployment where nobody asked anything, `outbound_obligations_requeued` stays silent.
2. **Preserved user behaviour** — a steer's answer still streams to the command-line interface (CLI) and dashboard tail; cron and webhook runs settle and bill exactly as today; the GitHub reviewer still posts its own review; Slack delivery stays at-least-once.
3. **Optimal-way check** — the most direct shape names the destination where the question arrives and owes nothing without one. The gap to unconstrained-optimal is a registry of reply surfaces for Jira or Linear comments, which nothing needs until one of them has an ingress.
4. **Rebuild-vs-iterate** — iterate. M198's ledger shape is right; its addressing input was wrong.
5. **What we build** — two nullable admission columns, three obligation columns, typed provider and destination on the job, abandonment, and the scan predicates.
6. **What we do NOT build** — a delete-the-junk migration (unnecessary once scans skip it), per-connector retry tuning, a dashboard view of abandoned answers.
7. **Fit with existing features** — compounds with M198's attempt counting and M203's continuation lineage; must not disturb the settle, which stays one fenced statement.
8. **Surface order** — N/A — no user surface; the change is ledger addressing and an operator event.
9. **Dashboard restraint** — N/A — nothing new is shown.
10. **Confused-user next step** — an operator reading `outbound_delivery_abandoned` gets the reason and attempt count; `data_flow.md` §C names what each reason means.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream owning the ledger end to end, from the producer's field through the report, the job and the poster to recovery, because each half without the other either strands answers or keeps cycling them.
- **Alternatives considered:** (a) port the Zig lookup, reading the provider from `core.connector_channels` by fleet — rejected: it owes for every run of a bound fleet, steers included, and an approval continuation would still lose its thread. (b) delete misaddressed rows in the migration — rejected: a data-deleting migration buys nothing once the scans skip them.
- **Patch-vs-refactor verdict:** this is a **patch** because the ledger, the queue and the poster keep their shapes; only the address a delivery carries, and when one is owed, change.

## Discovery (consult log)

- **Consults** — `ARCH: grounded in data_flow.md §C "Slack-resident answer round-trip" | proposal: the producer records the destination; the report owes only to it | status: conflicts — the page described the Zig binding lookup, which the Rust port did not keep | landing: a` (doc-only commit beside this spec corrects the page to today's code and names this design). Source findings: `commit.rs:198-210` passes the lease provider; `settle.rs:69-70` documents it as the provider resolved at issue; `poster.rs:77-92` drops an unparseable provider as permanent; `lanes.rs:223-228` stamps only on delivered; `obligation/sql.rs:111-116` re-offers after `LOST_AFTER`. Agent choice: destination on admissions only (§1 default).
- **Metrics review** — two operator events added, one existing event expected to fall silent for report-owed jobs; no analytics or funnel playbook update required, because nothing user-facing is counted.
- **Skill-chain outcomes** — `/orly-write-unit-test` (Sep 24, 2026): diff ledger 16/16 resolved; patch coverage 223/238 → 267/267 added lines; mutation not run (live-datastore proofs), carried to the PR. `/orly-write-integration-test`, `/review` and `orly-babysit-prs` run at the milestone Pull Request.
- **Deferrals** — none at authoring.
