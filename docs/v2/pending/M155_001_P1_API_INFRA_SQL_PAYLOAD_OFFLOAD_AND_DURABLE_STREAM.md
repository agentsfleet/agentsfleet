<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M155_001: Postgres stops storing bulk — bodies to object storage, per-slice detail to a durable stream

**Prototype:** v2.0.0
**Milestone:** M155
**Workstream:** 001
**Date:** Aug 03, 2026
**Status:** PENDING
**Priority:** P1 — event bodies grow without bound in the primary datastore, and the per-slice charge history M154 removed has no durable home
**Categories:** API, INFRA, SQL
**Batch:** B1 — §1 and §2 share the content-hash and the writer; §3 is gated on a measurement and may not build at all
**Depends on:** M154_001 — the read split (§7) that stopped the list selecting bodies is the precondition for moving them; the ledger shape (§3, §4) is what §3 here measures
**Provenance:** LLM-drafted (Claude Opus 5, Aug 03, 2026), from the three deferrals M154_001 recorded in Out of Scope and Discovery
**Canonical architecture:** `docs/architecture/observability.md` · `docs/architecture/data_flow.md` §The list read and the detail read are different reads · `docs/architecture/billing_and_provider_keys.md`

---

## Overview

**Goal (testable):** A fleet event's request body and agent answer are retrievable byte-for-byte after their Postgres row holds only a content hash, and a completed run's per-slice charge history is reconstructable from the durable stream without any Postgres table storing one row per slice.

**Problem:** Two unbounded stores and one missing one. Event bodies — a trigger payload and a full agent answer — accumulate in Postgres for the lifetime of every event, and nothing prunes them; M154 stopped the *list* reading them, which removed the read cost but not the storage growth. Separately, M154 deleted `fleet.metering_periods`, the per-slice charge detail, because it wrote a row roughly every twenty seconds of every run. The budget drain was reworked to apportion from the ledger instead, so enforcement is intact — but the slice-by-slice audit trail that answered "how did this run's debit accrue?" now exists nowhere. An operator reconciling a disputed charge has the total and the window, and no breakdown.

**Solution summary:** Move the two body columns behind a content hash: the row keeps a hash and a size, the bytes live in object storage, and the single-event detail read resolves them. Because the bodies are already off the list read, only the detail path changes shape. Emit the per-slice charge detail to the durable event stream at the moment the drain computes it, so the audit trail is append-only, retained on the stream's policy, and never a Postgres table again. The tenant usage rollup stays unbuilt until a measurement says the bounded ledger is too slow to aggregate — M154 established it holds two rows per event, not one per slice, which removed the growth that made a rollup necessary.

## PR Intent & comprehension handshake

- **PR title (eventual):** Move event bodies to object storage and per-slice charge detail to the durable stream
- **Intent (one sentence):** An operator can still read what an agent was asked and what it answered, and can still see how a run's charge accrued slice by slice, without Postgres carrying either as it grows.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/lib/s3/r2.zig` — the object-storage client already in the tree; §1 consumes it rather than introducing a second one.
2. `src/agentsfleetd/state/fleet_event_detail_store.zig` — the single-event read M154 §7 created. It is the only production reader of the body columns, which is what makes §1 a bounded change.
3. `src/agentsfleetd/fleet/sql_budget_drain.zig` — the drain that apportions the accumulated ledger total across `[created_at, last_charged_at]`. §2 emits its per-slice arithmetic; the drain's own correctness is M154's and must not change.
4. `docs/architecture/observability.md` — the canonical shape for what already ships off-box, and therefore where a durable outbox belongs rather than beside it.
5. `src/agentsfleetd/observability/otel_logs.zig` — the existing structured-log export path; §2 mirrors its transport decisions instead of inventing one.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/8*.sql` | CREATE | A history-layer slot swapping the two body columns for a content hash and a byte size |
| `src/agentsfleetd/state/fleet_event_detail_store.zig` | EDIT | The detail read resolves bytes from object storage instead of selecting columns |
| `src/agentsfleetd/state/fleet_events_store.zig` | EDIT | The write path stores bytes and records the hash; the list read is already payload-free |
| `src/lib/s3/r2.zig` | EDIT | Whatever the body put/get path needs that the client does not already expose |
| `src/agentsfleetd/fleet/sql_budget_drain.zig` | EDIT | Emits the per-slice detail it already computes |
| `src/agentsfleetd/observability/*.zig` | EDIT | The durable emit path for the slice records |
| `public/openapi/paths/fleets.yaml`, `public/openapi.json` | EDIT | The detail response is unchanged in shape; documented if latency or error surface moves |
| `docs/architecture/data_flow.md` | EDIT | The three-durable-stores section stops being true when bodies leave Postgres |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Records where per-slice charge history lives once it is not a table |
| `~/Projects/docs/changelog.mdx` | EDIT | User-visible if body retrieval latency or availability changes |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (hash algorithm name, bucket prefix, and size caps become named constants shared with any sibling runtime), **NDC** (no speculative rollup code until §3's measurement authorises it), **ORP** (the body columns and every reader retire together), **FLL** (the detail store gains a resolve path — split before the cap), **NLR** (the drain and detail store carry M154-era comments that stop being true).
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — the new slot follows the renumbered layer scheme M154 §1 established (`8xx` history); no `ALTER`, no patch-only slot.
- **`dispatch/write_zig.md`** — the object-storage path allocates per request and returns owned bytes; errdefer ladder, drain discipline on the detail query, cross-compile both linux targets.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — the detail endpoint keeps its contract; a new failure class (object unavailable) needs a registered error code, not a bare 500.
- **`~/Projects/dotfiles/docs/LOGGING_STANDARD.md`** — the slice records are an emit surface; scope, event naming, and error-code embedding apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — Zig stores, drain, object client | errdefer ladder on the put/get path; `PgQuery` drain on the detail read; cross-compile `x86_64-linux` and `aarch64-linux` |
| PUB / Struct-Shape | yes — the resolve path is a new surface | shape verdict per new `pub`; no `pub` without an in-tree consumer |
| File & Function Length (≤350/≤50/≤70) | yes — the detail store grows a resolve path | extract the object-resolution concern to a sibling before the cap |
| UFS (repeated/semantic literals) | yes — hash name, bucket prefix, size caps | named constants; identifier shared verbatim with any sibling runtime |
| UI Substitution / DESIGN TOKEN | no — no new user interface surface | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes — new emit surface, new error class, new slot | registered `UZ-` code for object-unavailable; slot follows the layer scheme; emit follows the logging standard |

## Prior-Art / Reference Implementations

- **Reference:** `src/lib/s3/r2.zig` — the object-storage client already in the tree. §1 consumes it; a second client would be the divergence to justify, not the default.
- **Reference:** `src/agentsfleetd/observability/otel_logs.zig` — the existing structured export path. §2 mirrors its transport and failure posture rather than inventing a parallel one.
- **Reference:** `schema/710_usage_ledger.sql` — the accumulate-in-place shape (`ON CONFLICT (event_id, charge_type) DO UPDATE … +=`) that bounds the ledger at two rows per event. §3 measures against this shape, and any rollup mirrors its charge-type vocabulary.

## Sections (implementation slices)

### §1 — Event bodies move behind a content hash

The two body columns are the only unbounded per-event storage left in Postgres, and M154 already removed every reader except one — the single-event detail read. That makes the move bounded: the write path stores bytes and records a hash, the detail read resolves them, and nothing else in the tree selects those columns. Content addressing rather than a per-event key means an identical payload replayed across events stores once, and a body is immutable by construction, so no invalidation path is needed. **Implementation default:** the row keeps hash and byte size, not a bucket path — the path is derived from the hash, so a bucket reorganisation never rewrites rows.

- **Dimension 1.1** — a stored body round-trips byte-for-byte through the detail read after its columns are gone from the row → Test `test_body_round_trips_through_object_storage`
- **Dimension 1.2** — two events carrying an identical payload store one object and both resolve it → Test `test_identical_payloads_store_once`
- **Dimension 1.3** — the events list read touches neither object storage nor the hash columns, so the read M154 made payload-free stays free → Test `test_list_read_makes_no_object_request`
- **Dimension 1.4** — a body whose object is unavailable answers a registered error code with the row's other fields intact, never a bare failure → Test `test_missing_object_degrades_to_registered_error`

### §2 — Per-slice charge detail lands on the durable stream

M154 deleted `fleet.metering_periods` because it wrote a row roughly every twenty seconds of every run, and reworked the budget drain to apportion from the ledger instead. Enforcement survived; the audit trail did not. An operator reconciling a disputed charge can see the total and the window and cannot see how it accrued. The drain already computes the per-slice arithmetic — this emits it where an append-only, retention-governed store can hold it, instead of putting the row back in Postgres. **Implementation default:** emit at the point the drain computes the slice, not on a separate pass, so the record cannot disagree with the charge it describes.

- **Dimension 2.1** — a run with renewals emits one slice record per drain computation, and their charges sum to the ledger's accumulated total for that event → Test `test_slice_records_sum_to_ledger_total`
- **Dimension 2.2** — a slice record carries the event, the window it covers, and the charge, so the accrual is reconstructable without joining Postgres → Test `test_slice_record_is_self_describing`
- **Dimension 2.3** — an emit failure never fails or delays the drain, and is itself observable → Test `test_emit_failure_does_not_block_the_drain`
- **Dimension 2.4** — no Postgres table gains a per-slice row; the catalogue holds nothing keyed one-per-slice → Test `test_no_per_slice_table_returns`

### §3 — The usage rollup is built only if a measurement demands it

M154 deferred `billing.usage_rollup` on the grounds that the ledger is bounded at two rows per event — `charge_type` has exactly two values — so the growth that made a rollup necessary is not there. That reasoning is sound and unmeasured. This slice measures before it builds, and is allowed to conclude that nothing should be built. **Implementation default:** measure tenant-scoped aggregation latency at a realistic ledger size against the existing indexes; build the rollup only if that number misses the budget the measurement itself declares. RULE NDC forbids writing the rollup speculatively.

- **Dimension 3.1** — tenant usage aggregation is measured at a stated ledger size and the number is recorded with the query plan that produced it → Test `test_usage_aggregation_latency_is_measured`
- **Dimension 3.2** — if and only if the measurement misses its budget, the rollup exists and its totals equal the ledger's for the same window → Test `test_rollup_totals_match_ledger`

## Interfaces

```
UNCHANGED  GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}
           Response shape is identical to M154 §7.2. The bodies arrive from
           object storage rather than from the row; callers observe no
           difference except a new failure class when an object is unavailable.

UNCHANGED  GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events
           Already payload-free after M154 §7.1. This milestone must not
           reintroduce a body field or an object request on this path.

NEW ERROR  Object-unavailable on the detail read — a registered UZ- code with
           the row's non-body fields still returned, never a bare 500 and never
           a silent empty body.

RECORD     One per-slice charge record per drain computation, carrying at
           minimum the event identifier, the window covered, and the charge.
           Append-only; no update path.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Object unavailable | Bucket outage, deleted object, credential expiry | Detail read returns the row's other fields plus a registered error code for the body; the list read is unaffected |
| Object write fails at event write | Bucket outage during ingest | The event write fails loudly rather than storing a row whose hash resolves to nothing |
| Hash collision | Two distinct payloads hashing equal | Refused at write; a collision must not silently serve the wrong body |
| Oversized body | A payload beyond the declared cap | Refused at a named limit at the boundary, not truncated silently |
| Emit unavailable | Stream or exporter down during a drain | The drain completes and charges correctly; the emit failure is observable and the slice record is lost rather than the charge |
| Replayed drain | The same slice computed twice | The stream carries a duplicate that reconciliation can identify, never a double charge |
| Aggregation slower than budget | Ledger larger than measured | §3's measurement is what authorises the rollup; an unmeasured slowdown authorises nothing |

## Invariants

1. **The list read makes no object request.** Enforced by a test asserting zero object-storage calls across a full page render, not by review.
2. **A row's body hash always resolves or reports.** Enforced by the write ordering — the object is durable before the row references it — and by the negative test for the unavailable case.
3. **No table holds one row per slice.** Enforced by a catalogue assertion, mirroring the shape M154 used for its own absence claims.
4. **A slice record cannot disagree with the charge it describes.** Enforced by emitting inside the drain's computation rather than on a second pass over stored state.
5. **No rollup code exists without the measurement.** Enforced by RULE NDC at write time and by §3's Dimension ordering.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Body resolve outcome | agentsfleetd | Detail read resolves or fails to resolve a body | event id, outcome, byte size | No body bytes, no payload fragments in any field | `test_missing_object_degrades_to_registered_error` |
| Per-slice charge record | agentsfleetd | The drain computes a slice | event id, window bounds, charge | Charge and identifiers only; no prompt or answer text | `test_slice_records_sum_to_ledger_total` |
| Emit failure | agentsfleetd | A slice record cannot be delivered | reason, error code | No record contents in the failure log | `test_emit_failure_does_not_block_the_drain` |

Operator-facing only. No product analytics event changes: the dashboard surfaces the same fields it did after M154 §7, from the same endpoints.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_body_round_trips_through_object_storage` | A stored body read back through the detail endpoint is byte-identical after the row carries only a hash |
| 1.2 | integration | `test_identical_payloads_store_once` | Two events with the same payload produce one object; both details resolve it |
| 1.3 | integration | `test_list_read_makes_no_object_request` | A page of events renders with zero object-storage calls |
| 1.4 | integration | `test_missing_object_degrades_to_registered_error` | A row whose object is absent returns the registered code and the row's other fields, never a bare 500 |
| 2.1 | integration | `test_slice_records_sum_to_ledger_total` | A run with renewals emits slice records whose charges sum exactly to the ledger's accumulated total |
| 2.2 | unit | `test_slice_record_is_self_describing` | A record carries event, window bounds and charge with no Postgres join required |
| 2.3 | integration | `test_emit_failure_does_not_block_the_drain` | With the exporter refusing, the drain still charges correctly and the failure is observable |
| 2.4 | integration | `test_no_per_slice_table_returns` | The catalogue holds no table keyed one row per slice |
| 3.1 | integration | `test_usage_aggregation_latency_is_measured` | Aggregation at the stated ledger size records a latency and the plan that produced it |
| 3.2 | integration | `test_rollup_totals_match_ledger` | Present only if 3.1 missed its budget; totals equal the ledger's for the same window |
| regression | integration | `test_event_detail_scoping_unchanged` | The workspace scoping M154 §7.2 established still refuses a sibling workspace's event identically |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Body columns are gone from the row | `grep -rnE 'request_json\|response_text' schema/` | only the hash/size columns, no body columns | P0 | |
| R2 | The list read makes no object request | `test_list_read_makes_no_object_request` | test passes | P0 | |
| R3 | No per-slice table returned | `test_no_per_slice_table_returns` | test passes | P0 | |
| R4 | No speculative rollup code | `grep -rn 'usage_rollup' src/ schema/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--)'` | no output unless §3.1 authorised it | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned references — zero remaining live uses.** Each grep drops comment lines, so prose recording a retirement cannot fail the criterion asserting it.

| Deleted symbol/column | Grep | Expected |
|-----------------------|------|----------|
| body columns on the event row | `grep -rnE -w 'request_json\|response_text' src/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--\|\*)'` | only the write path and the detail resolve |
| `metering_periods` | `grep -rn -w 'metering_periods' src/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--\|\*)'` | 0 matches |

## Out of Scope

- **Retention policy for the durable stream** — this milestone puts records there; how long they live is the stream's policy and Indy's call, not a code change here.
- **Approval-gate retention** — unchanged and deliberately unset; the table is a compliance record.
- **Row-Level Security** — its own milestone, with M154_001 as prerequisite.
- **Partitioning machinery** — M154 carried the stable key; the machinery still waits on a measurement, and §3 here measures aggregation, not partitioning.
- **Horizontal sharding** — M154_001 reworded this from a rejection to "measure, then decide"; that measurement is not this milestone's.
- **Migrating historical bodies** — this milestone changes the shape going forward. Backfilling existing rows into object storage is a separate operation with its own failure surface.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens a disputed charge and sees, slice by slice, how it accrued; and opens any event and reads what was asked and answered, however old it is.
2. **Preserved user behaviour** — expanding an event still shows its body, at the same endpoint with the same response shape. Nothing the dashboard does today changes.
3. **Optimal-way check** — the optimal shape is that the primary datastore holds what it must serve transactionally and nothing else. Bodies are read rarely and never joined; slice detail is append-only and never updated. Both belong outside Postgres.
4. **Rebuild-vs-iterate** — iterate. M154 already isolated the single reader of the bodies and reworked the drain; this is the follow-through, not a redesign.
5. **What we build** — a content-hash indirection for bodies, an emit path for per-slice charge detail, and a measurement that decides whether a rollup is warranted.
6. **What we do NOT build** — a retention policy, a historical backfill, partitioning machinery, sharding, or a rollup without the measurement.
7. **Fit with existing features** — consumes the object client and the export path already in the tree; adds no new external dependency the product does not already carry.
8. **Surface order** — no surface reordering; the detail dialog and events table keep their current positions and contents.
9. **Dashboard restraint** — no new dashboard surface. The slice detail is operator-facing through the stream, not a new page.
10. **Confused-user next step** — a body that cannot be resolved says so with a registered code and leaves every other field on screen, so the operator sees the event and knows precisely which part is unavailable.

## Decomposition & alternatives (patch vs refactor)

- **Chosen — hash indirection with the bytes in object storage.** Bounded because M154 left exactly one reader; content addressing deduplicates replays and makes bodies immutable by construction.
- **Rejected — keep bodies in Postgres and add retention.** Pruning makes an audit surface lossy on a timer, and the growth returns the moment the window widens. It trades an unbounded store for a lossy one.
- **Rejected — a per-event object key rather than a content hash.** Requires an invalidation path, stores replays twice, and rewrites rows on any bucket reorganisation.
- **Rejected — restore `fleet.metering_periods` under a retention policy.** This is the table M154 deleted for writing a row every twenty seconds of every run. Reinstating it with a sweeper trades one growth problem for a sweeper that must never touch live work.
- **Deferred — the rollup.** RULE NDC: no speculative code. §3 measures first and is permitted to build nothing.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

- **Why this spec exists (Aug 03, 2026).** M154_001 deferred three items to "M155" in its Out of Scope — payload offload, the durable outbox, and `billing.usage_rollup` — and no M155 spec existed. Three deferrals pointed at a document that was never written, so once M154 closed, that work would have had no home beyond a line in a `done/` spec. Indy's instruction on being shown this: write it.

- **Open decision — which durable store backs the stream.** M154_001 wrote "Elastic or Loki" and never settled it. This spec does not settle it either: §2 specifies the record's content and the emit's failure posture, which are the parts that constrain the code, and leaves the destination to the transport decision `docs/architecture/observability.md` and `observability/otel_logs.zig` already encode. **Blocks promotion out of `pending/`** — the golden-path walk cannot be `[?]`-free until the destination is named.

- **Open decision — the body size cap and the hash algorithm.** Both become named constants under RULE UFS, and both are boundary-defining rather than incidental. Neither is settled here.

- **Correction carried from M154_001 (Aug 03, 2026).** M154's Discovery twice recorded the ledger as "bounded at three rows per event". The code has exactly two `charge_type` values, `receive` and `stage` (`state/fleet_telemetry_store.zig`), so the bound is two. Corrected in M154_001 and stated correctly here, because §3's entire premise is that this bound removed the rollup's forcing reason — a premise that is stronger at two than at three.
