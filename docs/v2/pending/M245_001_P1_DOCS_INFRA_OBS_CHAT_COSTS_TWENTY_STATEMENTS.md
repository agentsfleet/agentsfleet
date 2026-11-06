<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M245_001: A ten-second chat costs at most twenty Postgres statements

**Prototype:** v2.0.0

**Milestone:** M245

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** PENDING

**Priority:** P1, every chat's statement count sets how many fleets one Postgres carries; M241's first rig run measured 105 per chat, and renewal alone is about 4,400 statements a second at 2,000 concurrent runs (brief arithmetic)

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, the fifth push-scheduler milestone; it starts after M244 merges and is its own Pull Request

**Depends on:** M241_001 (the per-path statement bench and its baseline); M243_001 (renewals ride the stream's `RenewBatch` frame, the stream session knows each lease it holds, and reports arrive over the stream, so no new renew endpoint is built here); M244_001 (approval-gate references live in Postgres, and the per-path numbers are measured on the broker that ships)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §Milestones (M245), §Renewal, batched per host, and §Proposed tables by milestone (created by M241_001)

---

## Overview

**Goal (testable):** `make bench-lease PROFILE=rig` records at most 20 Postgres statements per simulated 10 s chat, and none on the pricing path after warm-up, while every in-process cache answers only for a version a statement in the same pass just read.

**Problem:** M241's first rig run (`push_scheduler.md` §Baselines) charged a 10 s chat 105 statements. Without the 40 activity posts M243 removes, 65 remain:
- gates 11, report 10, host 9, issue 8, meter 8, pricing 7, shared 7, claim 4.9.
- The config, the price book, the provider resolution and the platform key are re-read on every use. `installed.rs:10-17` and `runner_fleet.md:36,57,555,571` forbid caching them.
- Each renewal is about eleven statements per lease every 5 s: the lease read, two price-book reads, the funds bound and read, the config and spend reads, the host lock and census, the meter and its lines.
- The report takes the host lock only so its runtime line numbers after a racing renewal's (`sql/meter.rs:35-44`). M241 then counted ten report-path statements per chat.

**Solution summary:** Four moves, all in the daemon, with no API change.
1. Revision-keyed caches whose versions ride the claim and the statements that use them, so no cross-replica signal is needed.
2. The claim opens the delivery, and one statement reads the gates.
3. The report is one statement, numbering lines from a new `runner.leases.daemon_line_count` under the lease row lock.
4. Renewal is one fenced statement per host per tick, and every deadline pass runs on one expiry clock.

The invalidation gaps the research found close in the same change.

**Design flow:**

```
claim (1) ─ fleet row, head revision id + document, tenant, memory flags,
            catalogue_version, platform secret (id, resource_version);
            opens the event row, stamps delivery, counts it
    │ versions
    ▼
per-replica caches, keyed (id, version):  definition ◄ head_revision_id
    price book ◄ catalogue_version    platform key ◄ catalogue + secret version
    │
gates (1 + the biller's funds read) ─► issue (host lock, insert with census)
    │
RenewBatch every 5 s (M243) ─► one fenced statement per host:
    per lease: fence, budget, extend, lines; guard: priced-at catalogue_version
report ─► one statement: meter, lines, result, session, slot, obligation,
          audit row; line numbers from daemon_line_count under the row lock
deadlines ─► sweeper:expiry (daemon) and one host timer (runner)
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): a chat costs at most twenty statements
- **Intent (one sentence):** let one Postgres carry the 20,000-fleet target by cutting each chat's statements to at most 20 without serving a stale config, price or key.
- **Authoring handshake:** Indy wants far fewer Postgres calls per chat, measured, with no correctness traded for it. M245 ships the four moves above, closes the invalidation gaps, and proves the count on the rig. ASSUMPTIONS I'M MAKING:
  1. M243 ships one `renew_batch` frame per host per tick with per-lease answers, still one renewal per entry ("Batched renewal here is framing", M243_001 assumption 3). Its gateway session caches each lease it offered or rebuilt, and reports arrive over the stream. CHORE(open) re-reads M243 as merged and amends §6 and §7 before any edit if it differs.
  2. M244 moved approval-gate references to Postgres, so `select_event_gate.sql` is the only gate lookup left to merge.
  3. The bench chat is M241's: one lease, a 10 s turn, two renewals, one report. Activity costs about 0 after M243.
  4. The funds check stays behind `Arc<dyn Biller>`, because billing §11 keeps lease code unchanged when the biller moves (`billing_and_provider_keys.md:77,557`).
  5. The dev cutover may reset data (brief), but both new columns backfill anyway, because each costs one `UPDATE`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/sql/meter_lease.sql` and `sql/meter.rs` — the fenced metering statement whose guard and measurement both new statements reuse; its comment states the snapshot rule `daemon_line_count` replaces.
2. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` and `lease/event.rs` — the claim, and the delivery writes §4 folds into it.
3. `rustd/crates/afd_fleet/src/lease/commit.rs` — every write a report commits today, in order.
4. `rustd/crates/afd_credential/src/credential/broker/cache.rs` — the daemon's existing in-process `moka` cache holding `Zeroizing` secrets, to mirror for the platform key.
5. `docs/architecture/push_scheduler.md` — the target design, and the rules this milestone retires.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M245_001_P1_DOCS_INFRA_OBS_CHAT_COSTS_TWENTY_STATEMENTS.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/architecture/runner_fleet.md`, `concurrency.md`, `lease_flow.md`, `billing_and_provider_keys.md`, `postgres_schemas.md` | EDIT | §1: the config-cache and timer reversals, renewal and report steps, the funds cadence, the two columns' readers |
| `docs/architecture/push_scheduler.md` | EDIT | §1: renewal takes no host lock, M245's table row; §8: the M245 baseline |
| `schema/2950_runner_leases_daemon_line_count.sql` | CREATE | §2: the column, its backfill and `NOT NULL` |
| `schema/2960_vault_secrets_resource_version.sql` | CREATE | §2: the column, its backfill, `NOT NULL` and the update trigger |
| `rustd/crates/afd_db/src/migration.rs`, `afd_db/tests/integration_migrate/**` | EDIT | §2: register both slots; positions and grants follow (MIG) |
| `rustd/crates/afd_vault/src/sql/insert_secret_if_absent.sql` and its binder | EDIT | §2: binds the initial version constant |
| `rustd/crates/afd_admin/src/platform_key.rs` | EDIT | §2: `set` and `deactivate` bump `catalogue_version` |
| `rustd/crates/afd_db/src/test_util/**` platform-default seeds; test fixtures that insert `vault.secrets` rows | EDIT | §2: seeds bump the catalogue; raw inserts bind the version |
| `rustd/crates/afd_fleet/src/lease/cache/mod.rs`, `cache/tests.rs` | CREATE | §3: the definition and price-book caches and their versioned keys |
| `rustd/crates/afd_fleet/src/lease/installed.rs`, `coverage.rs`, `usage/mod.rs`, `deliver/window.rs` | EDIT | §3: parse once per revision; ceilings and prices from the caches |
| `rustd/crates/afd_billing/src/price/mod.rs`, `src/sql/select_price_book.sql` | EDIT | §3: a book carries the catalogue version it was read at |
| `rustd/crates/afd_credential/src/provider/cache.rs` | CREATE | §3: the platform resolution and its zeroized key |
| `rustd/crates/afd_credential/src/provider/runtime/resolve.rs`, `registered/mod.rs` | EDIT | §3: resolve through the cache for the platform reference |
| `rustd/crates/afd_memory/src/memories.rs`, `hydrate.rs`, `access.rs` | EDIT | §3: the lease window hydrates from the scope the claim read |
| `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql`, `assign/ledger.rs`, `envelope.rs`, `event.rs`, `pull/mod.rs`, `pull/refuse.rs` | EDIT | §4: the claim reads the fleet, opens the event and stamps delivery |
| `rustd/crates/afd_fleet/src/lease/sql/lease/stamp_delivery.sql`, `afd_events/src/sql/insert_leased_fleet_event.sql`, `afd_events/src/sql/mod.rs` | DELETE, EDIT | §4: folded into the claim |
| `rustd/crates/afd_fleet/src/lease/sql/select_gates.sql` | CREATE | §5: spend, recorded gate and grants in one read |
| `rustd/crates/afd_fleet/src/lease/admit/mod.rs`, `admit/gates.rs`, `afd_gate/src/gate/pass.rs`, `afd_gate/src/gate/grants.rs` | EDIT | §5: gates judge facts the read returned |
| `rustd/crates/afd_fleet/src/lease/issue.rs`, `sql/lease/insert_lease_with_event.sql`, `afd_runner/src/capacity/mod.rs`, `capacity/choice.rs` | EDIT | §5: census, ticket and choice inside the insert |
| `rustd/crates/afd_fleet/src/lease/sql/commit_report.sql` | CREATE | §6: the one report statement |
| `rustd/crates/afd_fleet/src/lease/commit.rs`, `settle.rs`, `finalize.rs`, `obligation.rs`, `report/mod.rs`, `report/steps.rs`, `report/tokens.rs`, `sql/report.rs` | EDIT | §6: one statement, its binder and its answer |
| `rustd/crates/afd_fleet/src/lease/sql/select_lease_for_report.sql`, `select_run_tokens.sql`, `select_lease_disposition.sql`, `afd_events/src/sql/update_fleet_event_result.sql` | DELETE | §6: absorbed by the report statement |
| `rustd/crates/afd_fleet/src/lease/sql/renew_batch.sql`, `renew/batch.rs` | CREATE | §7: one statement per host per tick |
| `rustd/crates/afd_fleet/src/lease/renew/mod.rs`, `sql/renew.rs`, `sql/bill_expired_slice.sql`, `billing_sweep.rs` | EDIT | §7: the batch verb; the expiry statement advances the line count |
| `rustd/crates/afd_fleet/src/lease/sql/select_lease_for_renew.sql`, `sql/meter_lease.sql`, `sql/meter.rs` | DELETE | §7: both callers of the meter replaced |
| `rustd/crates/afd_gateway/**`, `afd_fleet/src/lease/rebuild.rs`, `afd_fleet/src/lease/sql/select_runner_live_leases.sql` (all created by M243), `afd_http/src/services/leasing/**` | EDIT | §6, §7: the session's lease cache gains pricing facts and the funds tick; `renew_batch` calls the batch verb |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | §7: `FUNDS_CHECK_TICKS`, `EXPIRY_TICK` and their asserts |
| `rustd/crates/agentsfleetd/src/sweepers/clock.rs` | CREATE | §7: `sweeper:expiry` |
| `rustd/crates/agentsfleetd/src/sweepers/mod.rs`, `billing.rs`, `authoring.rs`, `afd_authoring/src/maintenance/mod.rs`, `agentsfleetd/tests/supervisor.rs` | EDIT | §7: the expiry passes move to one clock; inventory follows |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `docs/metrics.census.tsv` | EDIT | §3: `agentsfleet_lease_cache_lookups_total` |
| `rustd/crates/afd_fleet/tests/integration_lease_caches.rs`, `integration_renew_batch.rs`, `integration_report_statement.rs`, `afd_credential/tests/integration_platform_key_cache.rs` | CREATE | §3–§7 Dimensions |
| `rustd/crates/afd_fleet/tests/integration_report_renewal_lock.rs` → `integration_report_renewal_lines.rs`; `integration_lease_plan/meter.rs`; fixtures under `afd_fleet/tests/**` calling `record_received`, `load_for_report` or `claim_and_settle` | MOVE, EDIT | §4, §6, §7: the race test loses the host lock; plan tests reach the new statements |
| `rustd/crates/afd_bench/src/statements/paths.rs`, `statements/tests.rs`, `lane/lease/drain/runner.rs`, `lane/lease/seed/tests.rs` | EDIT | §8: fragments follow; the drain counts memory windows |
| `bench/baselines/lease.rig.json` | EDIT | §8: the median run |
| `~/Projects/docs` `changelog.mdx`, branch `chore/m245-chat-costs-changelog` | EDIT | §7: a tenant out of credits stops within three renewals |

## Applicable Rules

- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, SQL-COUNTER, SQL-READER, SQL-DEFAULT, STS, VLT, SQL-FILE, SQL-ROW, SQL-PREPARE, SQL-LOCKCLOCK, SQL-DEADLINE, SQL-PLAN, CNX, TXN, SQL-ROLE. SQL-TIMEOUT: the lease path's statements run unbounded today; the new statements replace unbounded ones and add no new gap.
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-RACE, RS-TESTCONST
- **`code.md`** — NDC, NLR, NLG, ORP, ECL, TIM, FLL, CODE-SEAM, OWN
- **`tests.md`** — TEST-INJECT, TEST-RESIDUAL, TEST-CONTEND, TEST-COUNTER, TEST-NEG, TEST-LANE, ITF, TFX
- **`domains/logging.md`** — LOG-EVENT, LOG-RENAME, LOG-SECRET, LOG-CODE, OBS
- **`domains/auth.md`** — AUTH-NOLOG
- **`domains/architecture.md`** — ARCH-LAND, ARCH-HOMES, ARCH-PUBLISHED
- **`docs.md`** — DOC-02, DOC-03, DOC-S4

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, six architecture pages change | sentences under 25 words, three-sentence paragraphs; scan before each commit |
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, `moka` and `zeroize` join `afd_fleet` from the workspace | workspace dependencies only; `lint-rustd` per Section |
| The statement-prepare test (SQL-PREPARE) | yes, three new statements and five rewritten | each file joins the prepared list in the commit that adds it |
| Metrics census and producer test | yes, one family added | census row, declaration and producer in one commit |
| `gitleaks` | yes, cache tests seed platform keys | keys derived at runtime from the test entropy source; no new allow entry |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `commit.rs`, `renew/mod.rs`, `event.rs` | binders in `renew/batch.rs` and `commit.rs`; answer decoding beside each statement's row type |

## Prior-Art / Reference Implementations

- **Reference:** `afd_credential/src/credential/broker/cache.rs` — `moka` entries holding `Zeroizing<String>`, with a hand-written `Debug` that prints no secret. The platform key cache mirrors it; divergence: keyed by version, with no time-based expiry.
- **Reference:** `afd_fleet/src/lease/sql/meter_lease.sql` — the fence guard, the clamped measurement and the `FROM guard` writes. Both new statements keep them verbatim per lease.
- **Reference:** `afd_billing/src/ledger/sql/insert_lines.sql` — parallel arrays through `unnest`, the binding shape for many lines and many leases in one statement.
- **Reference:** `afd_fleet/tests/integration_report_renewal_lock.rs` — a renewal held mid-statement by a row lock on the activity counter, observed through `pg_stat_activity`; the line-numbering race test keeps the hold and drops the host lock.
- **Reference:** `docs/architecture/billing_and_provider_keys.md` §11 — why the funds check stays behind `Biller`.

## Sections (implementation slices)

### §1 — The reversals are written down first

One doc-only commit lands before any code (ARCH-LAND). `runner_fleet.md` replaces "config is never cached" (`:36`, `:57`, `:555`, `:571`) with the rule: config is cached by its head revision id, which the claim reads, so the next run still obeys an edit. `concurrency.md` §"Why neither plane has a central deadline scheduler" gains one daemon expiry clock and one runner host timer; per-call timeouts stay as they are. `push_scheduler.md`'s renewal sequence loses "one host lock", and its M245 table row names the two columns. `billing_and_provider_keys.md` states the funds cadence. `lease_flow.md` and `postgres_schemas.md` follow.

- **Dimension 1.1** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the six pages.
- **Dimension 1.2** — no page keeps the old rule → Test R7's `git grep` (manual): prints nothing outside `push_scheduler.md`'s retirement table.

### §2 — Two version columns, and the writers that skipped a bump

Slot 2950 adds `runner.leases.daemon_line_count INTEGER`, the tally the meter's `daemon_lines` alias counts today, named per SQL-COUNTER. It is backfilled per lease from `billing.usage_lines` rows numbered at or above `DAEMON_SEQ_BASE`, then set `NOT NULL`. Its readers are `renew_batch.sql`, `commit_report.sql` and `bill_expired_slice.sql`, each of which reads and advances it under the lease row lock.

Slot 2960 adds `vault.secrets.resource_version BIGINT`, backfilled to 1 and set `NOT NULL`. A `BEFORE UPDATE` trigger sets it to the old value plus one. The insert binds `SECRET_FIRST_VERSION` (SQL-DEFAULT). Its readers are the claim and the platform key cache. **Implementation default:** a trigger rather than a bump in each writer, because a vault writer added later then cannot skip it. Today a same-KEK (key-encryption key) replace in `update_secret.sql` keeps the row's id and `kek_version`; only the ciphertext hash `platform.sql` computes on every read notices it.

`platform_key.rs` `set` and `deactivate` run `bump_catalogue_version.sql` in their transactions, as `model.rs` does; the `afd_db` test seeds do the same. The fleet `resource_version` gap closes by design: no cache keys on it, because the memory flags ride the claim (§3). Bumping it in `update_access.sql`, `pause_fleet.sql`, `stream_ready.sql` and `initial.sql` would move the fleet's ETag (entity tag, `afd_authoring/src/creation/patch.rs:45`), failing an editor's next save for no edit of theirs. Slot numbers are renumbered from `origin/main` at CHORE(open), because PR #803 takes `schema/2190`–`2640`.

- **Dimension 2.1** — the count backfills from logged lines → Test `daemon_line_count_backfills_from_logged_lines` (integration): a lease with runtime lines at `BASE` and `BASE + 1` reads 2 after the slot; a lease with none reads 0.
- **Dimension 2.2** — a same-KEK replace moves the version → Test `secret_version_moves_on_same_kek_replace` (integration): `update_secret` with an unchanged `kek_version` moves 1 → 2, a KEK rewrap 2 → 3. Without the trigger, both stay 1.
- **Dimension 2.3** — a platform default change moves the catalogue → Test `platform_default_change_bumps_catalogue_version` (integration): `set` then `deactivate` move it by 2; a `set` refused for an uncatalogued model moves it by 0.

### §3 — Revision-keyed caches, with versions riding the claim

Each replica holds `moka` caches whose keys are newtypes of an id and a version. A key can only be built from a statement row, so no lookup by id alone compiles.

- **Definition:** the parsed definition, keyed by `head_revision_id`. Revisions are append-only (`schema/1510` trigger), so an id never names two documents. The claim returns the document beside its id. The cache saves the parse and serves every renewal's budget ceiling, which today re-reads `select_fleet_config.sql`.
- **Price book:** keyed by provider, model and `catalogue_version`.
- **Platform resolution:** provider, model, endpoint and the decrypted key in `Zeroizing`. Keyed by `catalogue_version` and the secret's id and `resource_version`. A delete-and-recreate gets a new id.
- **Tenant-model references:** always read (open blocker 2).
- **Memory scope and payer:** no cache. The claim returns both memory flags and `fleet.fleets.tenant_id`. Workspaces are append-only (`schema/1110` trigger), so the select of the workspace's tenant goes.

The claim returns `head_revision_id`, `catalogue_version`, and the active platform default's secret id and `resource_version`. That last one is selected as `lock_platform_credential.sql` selects it. The renewal and report statements carry the `catalogue_version` their lines were priced at, and the head revision each ceiling came from. A mismatch writes nothing for that statement or that lease. The daemon reads the new value once and runs again, so a catalogue edit costs one book read and one rerun. **Implementation default:** versions ride the statements rather than an `af.invalidate.*` NATS (the core message broker M244 adds) subject. Every subject is at most once, and a lost invalidation would serve a stale price with no bound; the versions cost no statement. Capacity is a performance parameter. Definitions are weighed by document bytes, capped at 64 MiB per replica; books and keys at 1,024 entries. A miss is a read, never a fault. The platform key is cached under open blocker 1's default.

- **Dimension 3.1** — a new head revision is obeyed on every replica → Test `config_cache_follows_new_head_revision` (integration): plane A leases on revision 1 ($10 a day). Plane B, a second plane over the same database, applies revision 2 ($1 a day). Plane A's next lease and next renewal judge $1, and no signal crossed between the planes.
- **Dimension 3.2** — a memory grant change is obeyed → Test `memory_scope_follows_grant_change` (integration): with `memory_reads_workspace` false, lease 1's window holds no shared entry; after `update_access` grants it, lease 2's window holds one.
- **Dimension 3.3** — a catalogue edit reprices → Test `catalogue_cache_reprices_after_bump` (integration): tick 1 prices output at 3,000,000 nanos per million tokens. An admin edit sets 6,000,000. Tick 2's first statement writes nothing, its rerun's line charges the new rate, and the tick costs exactly two more statements: the book read and the rerun.
- **Dimension 3.4** — a same-KEK key replace is obeyed → Test `platform_key_cache_misses_on_same_kek_rotation` (integration): lease 1 carries key A; `update_secret` to key B, same `kek_version`; lease 2 carries B, and the secret was read twice in total.
- **Dimension 3.5** — tenant-model references stay uncached → Test `tenant_model_reference_reads_every_lease` (integration): two leases on a tenant model run `entry.sql` twice.
- **Dimension 3.6** — a full cache reads again → Test `definition_cache_evicts_and_reads_again` (unit): a one-document cap with fleets A, B, A parses three times and returns three correct definitions.
- **Dimension 3.7** — a cached key never renders → Test `cached_platform_key_never_renders` (unit): the entry's `Debug` output holds no byte of the key.
- **Dimension 3.8** — lookups are counted → Test `cache_lookups_counted_by_outcome` (unit): a miss then a hit on one definition key moves `{definition, miss}` and `{definition, hit}` by 1 each, and no other series.

### §4 — The claim opens the delivery

`claim_admission_head.sql` already locks the slot and the head admission (`FOR UPDATE OF a SKIP LOCKED`, `:44`), so `lock_admission.sql`'s second lock on this path goes; park keeps its own. The claim now reads the fleet in the same statement. A fleet that is not `active` claims nothing: no fence moves and no row opens. It returns:
- the head revision document, id, `policy_semantics` and `runtime_model_ref`;
- the session context, `tenant_id`, both memory flags and §3's versions.

It also inserts the `fleet.events` row. Its conflict arm answers first delivery, repeat or terminal from the stored status, with no second read. It stamps `delivered_at`.

It returns the fleet's counters as the snapshot plus its own insert, because the trigger's write is invisible to its own statement (`runner_fleet.md:529`). Clients keep the greater count, so a concurrent insert it misses is repaired by the next frame. A definition that fails to parse on a cache miss ends through `refuse_unreadable_config`, over an open row, as the money refusals already end one.

- **Dimension 4.1** — one claim opens the delivery → Test `claim_opens_event_and_stamps_delivery` (integration): the event row is `received`, `delivered_at` is set and the answer is first delivery. The claim path ran two statements, the scan and the claim.
- **Dimension 4.2** — a fleet paused after the scan takes nothing → Test `claim_on_paused_fleet_takes_nothing` (integration): `pause_fleet.sql` between scan and claim; no row returns, the slot's fence is unchanged and no event row exists.
- **Dimension 4.3** — a redelivery is classified in the claim → Test `claim_redelivery_reports_repeat_in_one_statement` (integration): a lapsed lease's admission claimed again answers repeat; a finished one answers terminal; neither runs a third statement.
- **Dimension 4.4** — the counters include the claim's own row → Test `claim_counters_include_its_own_event` (integration): a fleet with 3 events answers `event_count` 4, where a `RETURNING` read alone answers 3.

### §5 — One gates read, and an issue insert that counts the host

`select_gates.sql` returns three things: the fleet's day and month spend, as `limits/select_fleet_spend.sql` windows them; the event's recorded gate with its decision; and the approved integration services. `afd_billing::budget::covers` and `afd_gate` judge those facts; neither reads again. A fault in the read is handed to each gate's existing posture: the budget admits, and the approval gate is judged `Unreadable` as today (`gate/pass.rs`). The outcome matches each read failing alone. Funds stay behind the `Biller` (its bound and its balance read, two statements).

The issue keeps `lock_host.sql` as its own statement, so the next statement's snapshot sees the previous claimant's commit (`afd_runner/src/capacity/mod.rs:1-5`). The census, the worker limit, the ticket revision and the choice record move into `insert_lease_with_event.sql`'s guard, and a full host writes nothing. Authoring's claim and renewal keep `capacity::lock` and its census.

- **Dimension 5.1** — an ungated funded event is admitted → Test `gates_read_admits_in_one_statement` (integration): `Admit`, with the gates path at two statements.
- **Dimension 5.2** — a failed read keeps each posture → Test `gates_read_fault_keeps_each_posture` (integration, the read on a closed connection): the money gates admit, the approval gate waits as `Unreadable`, and nothing is written.
- **Dimension 5.3** — a recorded gate decides from the read → Test `recorded_gate_decides_from_gates_read` (integration): approved passes, denied refuses `Denied`, pending awaits; `select_gate_status.sql` runs zero times.
- **Dimension 5.4** — a full host is refused inside the insert → Test `issue_refuses_full_host_in_insert` (integration): `worker_count` 1 with one live lease; the second issue writes no lease and leaves `lease_version` unchanged.
- **Dimension 5.5** — racing issues never overfill a host → Test `concurrent_issues_never_overfill_a_host` (integration, 100 issuers released together): `worker_count` 4 ends with exactly 4 live leases.

### §6 — The report is one statement

`commit_report.sql` makes, in one statement, every write `commit_report` and `record_settled` make today:
1. It locks the slot and lease, fenced, and measures the slice.
2. It inserts the token lines, priced in Rust before the statement, and the runtime line. The runtime line is priced inside the statement: its milliseconds times `RUN_NANOS_PER_SEC`, bound as a parameter, at the tier the cached book quotes. It is numbered `DAEMON_SEQ_BASE + daemon_line_count`.
3. It advances the cursor and the count, flips the lease, settles the admission and moves the tally.
4. It closes the event, drops other fences' tool calls, writes the session, releases the slot and its hold, and owes the obligation from the admission's reply destination.
5. It inserts the released audit row and returns the run's token sums.

It takes no host lock. READ COMMITTED re-reads a locked row's latest version, so the lease's `daemon_line_count` is current. The snapshot count in `meter_lease.sql` was not. The closing row's cost, and the token sums, add the lines this statement inserted, because a common table expression (CTE) cannot see its own writes. The statement guards on the posture, provider, model and `catalogue_version` it was priced at. **Implementation default:** the runtime line is priced in SQL, because its milliseconds are measured inside the statement; Dimension 6.6 holds that price equal to the rater's.

When the guard fails, the trailing row says whether the lease was already settled by this runner or fenced, as `select_lease_disposition.sql` decides today. A replay writes nothing. Pricing facts come from the stream session. A replica holding none reads the lease first, so the report costs two statements there.

- **Dimension 6.1** — one statement commits every write → Test `one_statement_report_commits_every_write` (integration): the event is `processed`, the session checkpointed, the slot released, the obligation owed, the runtime and token lines and the audit row present; the report path ran one statement.
- **Dimension 6.2** — a stale fence writes nothing → Test `one_statement_report_refuses_stale_fence` (integration): a reclaimed lease's report answers `Fenced`; no line, the event row unchanged.
- **Dimension 6.3** — a replay returns the stored outcome → Test `one_statement_report_replay_returns_stored_outcome` (integration, 100 copies released together): 1 `Settled`, 99 `AlreadySettled`, one runtime line, one obligation.
- **Dimension 6.4** — a report and a renewal never share a number → Test `report_and_renewal_number_lines_distinctly` (integration): a renewal held mid-statement and a report released behind it write lines `BASE + k` and `BASE + k + 1`, both present. A count-based number drops one line to `ON CONFLICT DO NOTHING`.
- **Dimension 6.5** — the closing cost counts the report's own lines → Test `closing_cost_counts_the_reports_own_lines` (integration): 500 nanos already logged plus a 1,000-nano report line answers 1,500; the snapshot alone answers 500.
- **Dimension 6.6** — statement pricing equals the rater → Test `statement_lines_match_rater` (integration): for one slice, the report, batch and expiry statements write the `seq`, `charged_nanos` and price id that `Rater::rate` and `log_lines` write.
- **Dimension 6.7** — a wrong pricing guess writes nothing → Test `report_with_wrong_pricing_facts_writes_nothing` (integration): session facts naming another model write nothing; the read-and-retry settles once.

### §7 — One renewal statement per host per tick, on one expiry clock

M243 already sends one `renew_batch` frame per host each `RENEWAL_TICK`, naming every live lease, and runs `Plane::renew` per entry (M243_001 §4, §6). This Section replaces that loop. `renew_batch.sql` binds parallel arrays, one row per lease and one per priced line. Each lease runs the renewal guard and measurement `meter_lease.sql` applies today, with two more checks:
- the runner's status, read without a lock, and the census. Renewal adds no occupancy, so a stale census is harmless.
- the fleet's spend against the ceilings the daemon passes, compared as `has_reached` compares them, and guarded on the head revision they came from.

M243's cancel mark (`runner.leases.cancel_requested_at`) still refuses a lease with `UZ-RUN-022`. A lease that passes extends both deadlines, logs its lines and advances `daemon_line_count`. Every write joins that lease's own guard row. A lease that fails one writes nothing and changes no other lease's answer. An id that does not parse, or names another runner's lease, is answered lost alone. A statement fault answers every lease retry, and the runner keeps them until their deadlines, as a 5xx does today.

Funds go through `Biller::check` once per distinct payer: on a session's first tick, then every `FUNDS_CHECK_TICKS` (3) ticks. A short payer's leases are left out of the statement and answered `UZ-RUN-012`. **Implementation default:** every third tick, because a two-tick chat then pays two thirds of a funds check (about 1.3 statements) instead of four statements. A run overruns by at most three 5 s slices, the small overrun Indy accepted. Checking on reconnect stops a runner from dodging the check by reconnecting.

`sweeper:expiry` runs, every `EXPIRY_TICK` (2 s), the expired-lease billing pass (from `sweeper:billing`) and authoring's attempt expiry (from `maintain`). The billing relay keeps settling lines; privacy purges keep their 10 s tick.

- **Dimension 7.1** — a host batch extends live leases and refuses a fenced one → Test `host_batch_extends_live_refuses_fenced` (integration): three leases, one reclaimed, one at `MAX_RUNTIME`, one live; the answers are lost, max runtime and extended, and only the live one logs lines. One statement ran.
- **Dimension 7.2** — a foreign or malformed id fails alone → Test `host_batch_answers_foreign_lease_lost_alone` (integration): two own leases, another runner's lease and `not-a-uuid`; two lost, two extended.
- **Dimension 7.3** — an over-budget fleet stops alone → Test `host_batch_refuses_over_budget_fleet_alone` (integration): spend exactly at one fleet's daily ceiling answers `UZ-RUN-015` without extending; one nano under extends; the other fleet extends.
- **Dimension 7.4** — funds are checked on cadence → Test `funds_checked_every_third_tick` (unit, paused time, `RecordingBiller`): over six ticks with two payers, it checks on ticks 1 and 4, once per payer. A payer short at tick 4 is absent from tick 4's statement and answered no credits.
- **Dimension 7.5** — a statement fault keeps every lease → Test `host_batch_fault_answers_retry` (integration, the statement on a closed connection): every lease answers retry, none lost.
- **Dimension 7.6** — a cancel-marked lease stops alone → Test `host_batch_refuses_cancel_marked_lease` (integration): of two leases, the one with `cancel_requested_at` set answers `UZ-RUN-022` and keeps its deadline; the other extends.
- **Dimension 7.7** — one clock runs every deadline pass → Test `expiry_clock_runs_every_deadline_pass` (unit): one tick runs both passes; the supervisor inventory lists `sweeper:expiry`.
- **Dimension 7.8** — a failed expiry pass waits → Test `expiry_pass_failure_waits_for_next_tick` (unit): an unreachable database logs `expiry_sweep_failed` with its `error_code`, and the task keeps running.

### §8 — Measured: twenty statements per chat

`paths.rs` gains fragments for the new claim, gates, batch and report statements and drops the deleted ones. The drain's runner counts leases that carried a memory window, as `drain_memory_windows_per_lease`. That exposes a hydrate failing in the drain, which would undercount `issue` (M241's `other` was 0.08). Three interleaved runs follow a reset rig; the median goes to `bench/baselines/`, and `push_scheduler.md` §Baselines gains an M245 column. Projected per chat (unverified until this run): claim 2.3, gates 2.7, pricing 0, issue 3 to 4, host 1, meter 2, report 1, shared 5.7, activity 0. That is about 18 to 19 against a ceiling of 20, so the margin is thin; the lane prints every path against this projection.

- **Dimension 8.1** — a chat costs at most 20 → Test `make bench-lease PROFILE=rig` (integration): `drain_statements_per_lease` ≤ 20, `_pricing` < 0.05, `_other` < 0.1, `drain_memory_windows_per_lease` 1.0.
- **Dimension 8.2** — each new statement lands in its path → Test `new_statements_match_their_paths` (unit): each new statement's normalized text classifies as claim, gates, meter or report, never `other`.
- **Dimension 8.3** — the drain counts windows → Test `drain_counts_leases_that_carry_a_window` (unit): three leases, two with windows, report 0.667.
- **Dimension 8.4** — the baseline is a median run → Test `baselines_are_median_runs` (manual): §Baselines lists three totals, and the committed file holds the middle one.

## Interfaces

- **Wire:** none. `renew_batch` and its per-lease answers keep M243's shapes. Refusals keep `UZ-RUN-011`, `UZ-RUN-012`, `UZ-RUN-015`, M243's `UZ-RUN-022` and the max-runtime code. No route, field or error code is added.
- **Schema:** `runner.leases.daemon_line_count INTEGER NOT NULL`; `vault.secrets.resource_version BIGINT NOT NULL`, moved by trigger on every update.
- **Timing (`afd_core::timing`):** `FUNDS_CHECK_TICKS` = 3, asserted at least 1 and `FUNDS_CHECK_TICKS × RENEWAL_TICK < LEASE_TTL`; `EXPIRY_TICK` = 2 s, asserted below `LEASE_TTL` (TIM).
- **Internal:** `Acquired` carries the claim's fleet facts and versions. `PriceBook` carries its `catalogue_version`. `Plane::renew_batch(runner, frame)` answers one verdict per lease. `Memories::hydrate_scoped` takes the claim's scope. Cache keys build only from statement rows.
- **Metric:** `agentsfleet_lease_cache_lookups_total{cache, outcome}`, with `cache` ∈ `definition`, `price_book`, `platform_key` and `outcome` ∈ `hit`, `miss`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Stale definition | failure | Apply on another replica | the claim's new revision id misses; the run judges the new config | `config_cache_follows_new_head_revision` |
| Stale memory scope | failure | `update_access` between leases | the claim returns the flags; the window follows | `memory_scope_follows_grant_change` |
| Catalogue edited mid-run | failure | admin price edit | the statement writes nothing; one reread and rerun | `catalogue_cache_reprices_after_bump` |
| Key replaced, same KEK | security | vault update | the version moves; the next lease carries the new key; the old entry drops zeroized | `platform_key_cache_misses_on_same_kek_rotation` |
| Default switched, no bump | failure | `platform_key.rs` `set` | the bump commits with it | `platform_default_change_bumps_catalogue_version` |
| Cache over capacity | capacity | more fleets than the cap | evicted entries read again | `definition_cache_evicts_and_reads_again` |
| Key in a log | security | `Debug` of an entry | no key bytes render | `cached_platform_key_never_renders` |
| Paused between scan and claim (race) | failure | operator pause | nothing claimed | `claim_on_paused_fleet_takes_nothing` |
| Redelivered admission (replay) | failure | lapsed lease | repeat or terminal from one statement | `claim_redelivery_reports_repeat_in_one_statement` |
| Own write unseen | failure | trigger or CTE invisible to its statement | the statement adds its own row or lines | `claim_counters_include_its_own_event`, `closing_cost_counts_the_reports_own_lines` |
| Gates read fails (dependency unavailable) | failure | Postgres refuses the read | each gate applies today's posture; nothing written | `gates_read_fault_keeps_each_posture` |
| Host full | capacity | `worker_count` reached | the insert writes nothing | `issue_refuses_full_host_in_insert`, `concurrent_issues_never_overfill_a_host` |
| Stale report (race) | failure | reclaim | `Fenced`, nothing written | `one_statement_report_refuses_stale_fence` |
| Report replayed (replay) | failure | lost response | `AlreadySettled`, nothing written | `one_statement_report_replay_returns_stored_outcome` |
| Report beside renewal (race) | failure | concurrent statements | distinct line numbers | `report_and_renewal_number_lines_distinctly` |
| Wrong pricing facts | failure | a replica guessed the lease's model | nothing written; read and retry | `report_with_wrong_pricing_facts_writes_nothing` |
| Fenced lease in a batch (race) | failure | reclaim | that lease lost; the rest extend | `host_batch_extends_live_refuses_fenced` |
| Foreign or malformed id (malformed input) | failure | runner defect | that entry lost alone | `host_batch_answers_foreign_lease_lost_alone` |
| Fleet killed mid-run | failure | M243's cancel mark | `UZ-RUN-022` for that lease only | `host_batch_refuses_cancel_marked_lease` |
| Fleet over budget (exceeded quota) | capacity | spend reached a ceiling | `UZ-RUN-015` for that lease only | `host_batch_refuses_over_budget_fleet_alone` |
| Tenant out of funds (exceeded quota) | capacity | balance short | `UZ-RUN-012` within three ticks | `funds_checked_every_third_tick` |
| Batch statement fails (timeout, network blip) | failure | Postgres error or timeout | every lease retries; none lost | `host_batch_fault_answers_retry` |
| Expiry pass fails (dependency unavailable) | failure | database down | logged; the next tick retries | `expiry_pass_failure_waits_for_next_tick` |

## Invariants

1. **A cache answers only for a version a statement in the same pass read.** Cache keys are newtypes whose only constructors take a statement's row. No lookup by bare id compiles. A wrong version misses, never serves. `config_cache_follows_new_head_revision` is the shortest counterexample.
2. **A statement priced from a cached book writes nothing once the catalogue moved.** `renew_batch.sql` and `commit_report.sql` compare `model.catalogue_version` with the bound priced-at version in their guards. The race is the one today's pre-transaction book read already has. Test: `catalogue_cache_reprices_after_bump`.
3. **A lease's runtime lines carry distinct numbers.** `daemon_line_count` is read and advanced under the lease's `FOR UPDATE` lock by every writer of daemon-range lines. `uq_usage_lines_work_type_work_id_seq` backs it. Test: `report_and_renewal_number_lines_distinctly`.
4. **A lease that fails its guard in a batch writes nothing and moves no other answer.** Every write in `renew_batch.sql` joins its lease's guard row (`FROM guard`), the pattern `meter_lease.sql` uses today. Test: `host_batch_extends_live_refuses_fenced`.
5. **`vault.secrets.resource_version` moves on every update.** The slot's `BEFORE UPDATE` trigger enforces it, whichever writer runs. Test: `secret_version_moves_on_same_kek_replace`.
6. **A cached platform key is never rendered and is zeroed when dropped.** The value is `Zeroizing<String>`, and the entry's `Debug` is written by hand. Test: `cached_platform_key_never_renders`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_lease_cache_lookups_total` | operator | each cache lookup | `cache`, `outcome` | no fleet, tenant or key value in labels | `cache_lookups_counted_by_outcome` |
| `renew_batch_repriced` (log) | operator | a guard forced a rerun | `runner_id`, `leases`, `reason` | no prices, no key material | `catalogue_cache_reprices_after_bump` |
| `expiry_sweep_completed`, `expiry_sweep_failed` (log) | operator | each expiry tick | counts, `error_code` | identifiers only | `expiry_pass_failure_waits_for_next_tick` |
| `renew_no_credits`, `renew_budget_breach` (log, unchanged spellings) | operator | a batch refuses a lease | `fleet_id`, `lease_id` | identifiers only | `funds_checked_every_third_tick`, `host_batch_refuses_over_budget_fleet_alone` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A chat costs at most 20 statements (§8) | `jq -e '.measurements.drain_statements_per_lease <= 20' bench/baselines/lease.rig.json` | `true`, exit 0 | P0 | |
| R2 | Pricing sends nothing after warm-up (§3, §8) | `jq -e '.measurements.drain_statements_per_lease_pricing < 0.05' bench/baselines/lease.rig.json` | `true`, exit 0 | P0 | |
| R3 | Every cache follows its version (§2, §3) | `make test-integration-rustd` | exit 0; `config_cache_follows_new_head_revision`, `memory_scope_follows_grant_change`, `catalogue_cache_reprices_after_bump`, `platform_key_cache_misses_on_same_kek_rotation` print `ok` | P0 | |
| R4 | A host batch renews live leases and refuses a fenced one (§7) | `make test-integration-rustd` | exit 0; `host_batch_extends_live_refuses_fenced ... ok` | P0 | |
| R5 | The one-statement report is fenced and replay-safe (§6) | `make test-integration-rustd` | exit 0; `one_statement_report_refuses_stale_fence` and `one_statement_report_replay_returns_stored_outcome` print `ok` | P0 | |
| R6 | A report and a renewal never share a line number (§6) | `make test-integration-rustd` | exit 0; `report_and_renewal_number_lines_distinctly ... ok` | P0 | |
| R7 | No page or comment keeps "config is never cached" (§1) | `git grep -n -i -e "config is never cached" -e "no in-memory config cache" -e "re-read per lease and never cached" -- docs/architecture rustd/crates/afd_fleet/src ':!docs/architecture/push_scheduler.md'` | no output | P1 | |
| R8 | Only the issue takes the host lock in `afd_fleet` (§6, §7) | `git grep -n "capacity::lock(" -- rustd/crates/afd_fleet/src` | one line, in `issue.rs` | P1 | |
| R9 | The replaced statements have no caller (§4, §6, §7) | `git grep -n -w -e SELECT_LEASE_FOR_RENEW -e SELECT_LEASE_FOR_REPORT -e SELECT_RUN_TOKENS -e SELECT_LEASE_DISPOSITION -e SELECT_FLEET_CONFIG -e STAMP_DELIVERY -e METER_LEASE -e INSERT_LEASED_FLEET_EVENT -e UPDATE_FLEET_EVENT_RESULT -- rustd` | no output | P1 | |
| R10 | The diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `select_lease_for_renew.sql`, `load_for_renew` | `git grep -n -w -e SELECT_LEASE_FOR_RENEW -e load_for_renew -- rustd` | no output |
| `select_lease_for_report.sql`, `load_for_report` | `git grep -n -w -e SELECT_LEASE_FOR_REPORT -e load_for_report -- rustd` | no output |
| `select_run_tokens.sql`, `run_tokens`; `select_lease_disposition.sql` | `git grep -n -w -e SELECT_RUN_TOKENS -e run_tokens -e SELECT_LEASE_DISPOSITION -- rustd` | no output |
| `select_fleet_config.sql`, `stored_config` | `git grep -n -w -e SELECT_FLEET_CONFIG -e stored_config -- rustd` | no output |
| `stamp_delivery.sql`, `record_received`; `insert_leased_fleet_event.sql` | `git grep -n -w -e STAMP_DELIVERY -e record_received -e INSERT_LEASED_FLEET_EVENT -- rustd` | no output |
| `meter_lease.sql`, `meter.rs`; `update_fleet_event_result.sql` | `git grep -n -w -e METER_LEASE -e UPDATE_FLEET_EVENT_RESULT -- rustd` | no output |

`select_fleet_with_session.sql`, `select_access.sql`, `select_tenant_for_workspace.sql`, `lock_admission.sql`, `release_fleet_slot.sql`, `delete_other_fences.sql`, `select_reply_destination.sql`, `census.sql` and `record_choice.sql` keep callers off this path: the message path, the runner's memory verbs, the wallet, park, affinity, tool detail and authoring. CHORE(open) re-runs every row against `origin/main`, because M243 and #803 may move these files first.

## Out of Scope

- Activity posts: M243 removes the per-post target read.
- Memory capture at report: a separate role (`assume_memory_role.sql`) keeps its own statements.
- Authoring attempts in the host batch: they renew through `afd_authoring/src/lease/renew.rs`; whether authoring joins the stream is M243's question.
- The funds read inside the gates and batch statements: billing §11 forbids it today, and reversing §11 is Indy's call. It would save about three statements per chat: the issue's bound and balance, and the renewals' share.
- One deadline column per lease: the slot's deadline is the claim's reclaim test, and both columns are written from one value in one statement.
- A NATS invalidation subject: versions ride the statements (§3).
- Caching tenant-model references: open blocker 2's "yes" path, which adds a slot.
- The liveness sweep: M247 rebuilds host death around the stream.

---

## Product Clarity (authoring record)

N/A: no user surface. No route, frame, command, page or error code changes; the one billing-visible effect, a credit stop within three renewals, is a changelog entry (§7).

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** reversals on paper first, then the schema the caches and line numbers stand on, then each verb collapsed in the order a chat runs: claim, gates and issue, report, renewal. The measurement closes it, so each Section's statements are counted before the next lands.
- **Alternatives considered:**
  - An `af.invalidate.<kind>.<id>` NATS subject: rejected, because it is at most once and a lost message serves stale config with no bound. The versions it would carry already ride statements we run.
  - Time-to-live caches: rejected, because an invisible staleness window is why `runner_fleet.md` banned caching.
  - The funds read inside the statements: rejected by default under billing §11.
  - A per-lease renewal loop: rejected, because one statement per lease per tick is the cost being removed.
  - Keeping the host lock for numbering: rejected, because it serialises every renewal and report on a host.
- **Patch-vs-refactor verdict:** a **refactor**, because the statement structure of four verbs changes while every external shape stays.

## Discovery (consult log)

- **Consults** — source review on `main` `14539a73e` and the M241 branch, Nov 06, 2026:
  - Renewal and report take the host lock only so `daemon_lines` counts the other's committed line (`sql/meter.rs:35-44`, `commit.rs:127`, `renew/mod.rs:130`). A duplicate number loses a runtime line to `ON CONFLICT DO NOTHING` (`insert_lines.sql`; `integration_report_renewal_lock.rs` header). `bill_expired_slice.sql:23` counts the same way.
  - The census runs after the host lock as its own statement for snapshot freshness (`afd_runner/src/capacity/mod.rs:1-5`).
  - The claim already locks the head admission (`claim_admission_head.sql:44`); `record_received` locks it again (`event.rs`, `lock_admission.sql`).
  - Revisions and workspaces are append-only by trigger (`schema/1510`, `schema/1110:26-32`).
  - `platform_key.rs` `set` and `deactivate` change platform defaults without `bump_catalogue_version.sql`. Only `model.rs:176,225,292` bump.
  - `vault.secrets` has no version column (`schema/1200`), and `platform.sql` hashes the ciphertext on every read.
  - Four fleet writers skip the `resource_version` bump: `update_access.sql`, `pause_fleet.sql`, `stream_ready.sql`, `initial.sql`. The column is the authoring ETag (`creation/patch.rs:45`).
  - `update_fleet_event_result.sql` sums `billing.usage_lines` and reads the counters, both blind to writes in its own statement.
  - Only billing bounds its transactions with `set_config`; lease statements run unbounded (SQL-TIMEOUT, unchanged here).
  - `Biller` is the one funds path by decision (`billing_and_provider_keys.md:77,557`).
  - The broker already holds minted connector tokens in daemon memory (`credential/broker/cache.rs:28`).
  - The bench excludes `BEGIN` and `COMMIT` from statement counts (`afd_bench/src/statements/queries.rs:25`).
- **Programme decisions this spec relies on:**
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: the programme; no compatibility path for per-lease renewal.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: version guards instead of a broker channel; no tenant-model version slot by default.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: every new statement keeps the Postgres fence.
  > Indy (Nov 06, 2026: 01:18 PM): "Yes i agree on NATS." — context: M244's broker carries no cache state.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
  - The reviewed brief records, as Indy's decisions of Nov 06, 2026, "one expiry clock" and that "a small budget overrun is accepted". Its M2 finding asks to "check money every Nth tick". §7 builds on both; neither is a verbatim quote of Indy.
- **Model reviews** — the programme brief's two EGM passes are recorded in M241_001's Discovery. This spec has had no model review of its own yet.
- **Open blockers** (the spec stays in `pending/` until Indy answers both):
  1. **May decrypted platform keys be cached in daemon memory?** This changes the security posture: a key lives past its lease. **Implementation default:** cache the resolution with the key in `Zeroizing`, re-decrypt only when the secret's id or `resource_version` moves, and zero it on eviction. Precedent: the broker caches connector tokens the same way, and a platform host's lease already carries the key to the runner. M246 keeps the key inside the daemon for tenant runners and re-resolves it on every proxied model call (M246_001 §4), which this cache serves. "No" keeps the resolution cache and re-reads the secret per lease, one statement per chat.
  2. **Is `model.tenant_models` cached or always read?** It has no version column (`schema/1330`). **Implementation default:** always read. A fleet on a tenant model then pays four statements per issue (`lock_catalogue.sql`, `lock_entry_credential.sql`, `entry.sql`, `select_secret.sql`), which the platform-default bench does not see. "Cache" adds a `resource_version` slot (2970) bumped by the four entries writers, and the claim returns it.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
