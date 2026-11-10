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

**Status:** IN_PROGRESS

**Implementation branch:** `feat/m245-chat-costs-twenty-statements`, worktree `~/Projects/agentsfleet-m245`, cut from `origin/main` `291777075` on Nov 10, 2026. M233_002 builds in parallel on `afd_fleet/src/lease/{claim.rs,pull/mod.rs,deliver/mod.rs,report/mod.rs}` and `afd_gateway/src/session/{offer.rs,dispatch/report.rs}`; whichever merges second rebases.

**Priority:** P1, every chat's statement count sets how many fleets one Postgres carries; main measures 74.2 per chat after M243 and M244 (`bench/baselines/lease.rig.json`, `6b0caea35`), and renewal alone is about 4,400 statements a second at 2,000 concurrent runs (brief arithmetic)

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, the fifth push-scheduler milestone; it starts after M244 merges and is its own Pull Request

**Depends on:** M241_001 (the per-path statement bench and its baseline); M243_001 (renewals ride the stream's `RenewBatch` frame, the stream session knows each lease it holds, and reports arrive over the stream, so no new renew endpoint is built here); M244_001 (approval-gate references live in Postgres, and the per-path numbers are measured on the broker that ships)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e; amended Nov 10, 2026 (Claude Fable 5.2) against main 51d41a9e3, see Discovery

**Canonical architecture:** `docs/architecture/push_scheduler.md` §Milestones (M245), §Renewal, batched per host, and §Proposed tables by milestone (created by M241_001)

---

## Overview

**Goal (testable):** `make bench-lease PROFILE=rig` records at most 20 Postgres statements per simulated 10 s chat, and none on the pricing path after warm-up, while every in-process cache answers only for a version a statement in the same pass just read.

**Problem:** Main charges a 10 s chat 74.165 statements and 37.43 commits (`bench/baselines/lease.rig.json`, `6b0caea35`). M243 already removed the 40 activity posts.
- By path: gates 11, host 9, other 8.1, shared 8, pricing 7, issue 7, claim 6.05, meter 6, report 6, task 6. `other` holds statements whose text M243 changed or added. `task` holds the durable-task reads #803 added.
- The config, the price book, the provider resolution and the platform key are re-read on every use. `installed.rs:10-17` and `runner_fleet.md:71,803,823` forbid caching them.
- Each renewal is about eleven statements per lease every 5 s: the lease read, two price-book reads, the funds bound and read, the config and spend reads, the host lock and census, the meter and its lines.
- The report takes the host lock only so its runtime line numbers after a racing renewal's (`sql/meter.rs:35-44`).
- The count misses round trips. Every connection checkout pings Postgres, a run asks for folded input before its first model call, and each loop loads a checkpoint that cannot exist (§8).
- The 74 statements execute in 13.2 ms in total, so a chat's wait is its serial round trips (Discovery, measured on main).

**Solution summary:** Five moves. The first four are in the daemon; the fifth also touches the runner and the lease it is offered.
1. Revision-keyed caches whose versions ride the claim and the statements that use them, so no cross-replica signal is needed.
2. The claim opens the delivery, and one statement reads the gates.
3. The report is one statement, numbering lines from a new `runner.leases.daemon_line_count` under the lease row lock.
4. Renewal is one fenced statement per host per tick, and every deadline pass runs on one expiry clock.
5. A checkout pings only an idle connection, a run skips a fold ask the claim already answered, and no loop loads a checkpoint it cannot have.

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
first model call ─► no context load; no fold ask when the lease says none waits
pool checkout ─► pings only a connection idle for 1 s or longer
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): a chat costs at most twenty statements
- **Intent (one sentence):** let one Postgres carry the 20,000-fleet target by cutting each chat's statements to at most 20 without serving a stale config, price or key.
- **Authoring handshake:** Indy wants far fewer Postgres calls per chat, measured, with no correctness traded for it. M245 ships the five moves above, closes the invalidation gaps, and proves the count on the rig. ASSUMPTIONS I'M MAKING:
  1. M243 merged as assumed (`afd_gateway/src/session/dispatch/renewal.rs`): one renewal frame per host per tick, answered per lease, running `LeaseService::renew` per entry, eight at a time. Its session cache holds each lease's target, deadline and cancel mark, and reports arrive over the stream (`session/dispatch/report.rs`).
  2. M244 moved approval-gate references to Postgres, so `select_event_gate.sql` is the only gate lookup left to merge.
  3. The bench chat is M241's: one lease, a 10 s turn, two renewals, one report. Activity costs about 0 after M243. §9 adds a run's fold asks, which the default drain does not send.
  4. The funds check stays behind `Arc<dyn Biller>`, because billing §11 keeps lease code unchanged when the biller moves (`billing_and_provider_keys.md:77,557`).
  5. The dev cutover resets data (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM), so both new columns are added `NOT NULL` with no backfill; a change that meets existing rows fails loud. This holds only until the first production deploy; §2 states the other form.
  6. Runner and daemon deploy together, so the lease field §8 adds needs no version negotiation. An absent field still reads as "ask".
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/lease/sql/meter_lease.sql` and `sql/meter.rs` — the fenced metering statement whose guard and measurement both new statements reuse; its comment states the snapshot rule `daemon_line_count` replaces.
2. `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` and `lease/event.rs` — the claim, and the delivery writes §4 folds into it.
3. `rustd/crates/daemon/afd_fleet/src/lease/commit.rs` — every write a report commits today, in order.
4. `rustd/crates/daemon/afd_credential/src/credential/broker/cache.rs` — the daemon's existing in-process `moka` cache holding `Zeroizing` secrets, to mirror for the platform key.
5. `docs/architecture/push_scheduler.md` — the target design, and the rules this milestone retires.
6. `rustd/crates/daemon/afd_db/src/pool/open.rs` — the pool options every role shares, where §8 gates the checkout ping.
7. `rustd/crates/harness/guffbolt/src/drive/mod.rs` and `rustd/crates/runner/afr_agent/src/loop/context/mod.rs` — where a run asks for folded input and loads its checkpoint.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M245_001_P1_DOCS_INFRA_OBS_CHAT_COSTS_TWENTY_STATEMENTS.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/architecture/runner_fleet.md`, `concurrency.md`, `lease_flow.md`, `billing_and_provider_keys.md`, `postgres_schemas.md`, `scaling.md`, `runner_protocol.md` | EDIT | §1: the config-cache and timer reversals, renewal and report steps, the funds cadence, the two columns' readers, the checkout ping, the lease field and the removed verb |
| `docs/architecture/push_scheduler.md` | EDIT | §1: renewal takes no host lock, M245's table row; §9: the M245 baseline |
| `schema/20261108123200_runner_leases.sql`, `schema/20261108121000_vault_secrets.sql` | EDIT | §2: each table's file gains its column, and `vault.secrets` its update trigger; after the first production deploy each is a new `<UTC>_<schema>_<table>_<what>.sql` instead |
| `rustd/crates/daemon/afd_db/src/migration.rs`, `afd_db/tests/integration_migrate/**` | EDIT | §2: column positions and grants follow; `migration.rs` changes only under the post-deploy form (MIG) |
| `rustd/crates/daemon/afd_vault/src/sql/insert_secret_if_absent.sql` and its binder (`write.rs`, `sql/mod.rs`, `lib.rs`) | EDIT | §2: binds the initial version constant, `SECRET_FIRST_VERSION` |
| `rustd/crates/daemon/afd_admin/src/platform_key.rs`, `model.rs`, `tests/integration_admin.rs` | EDIT | §2: `set` and `deactivate` bump `catalogue_version` through `model.rs`'s bump; Dimension 2.3 |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/main.rs`, `secret_version.rs` | EDIT, CREATE | §2: Dimension 2.2 |
| `rustd/crates/daemon/main/tests/integration_runner_shapes.rs` | EDIT | §2: the recorded `runner.leases` shape gains `daemon_line_count` |
| test fixtures that insert `vault.secrets` rows | EDIT | §2: raw inserts bind the version; the `afd_db` seeds do not bump the catalogue (Discovery) |
| `rustd/crates/daemon/afd_db/src/test_util/statements.rs`, `test_util/mod.rs` | CREATE, EDIT | §3: the shared statement counter every counting Dimension asserts with (PLAN, Discovery) |
| `rustd/crates/daemon/afd_fleet/src/lease/cache/mod.rs`, `cache/tests.rs` | CREATE | §3: the definition and price-book caches and their versioned keys |
| `rustd/crates/daemon/afd_fleet/Cargo.toml`, `rustd/Cargo.lock`, `src/lease/mod.rs`, `src/lease/store.rs`, `src/task/lease.rs` | EDIT | §3: `moka` joins from the workspace; the store holds one cache every clone shares; both `installed_on` callers pass it |
| `rustd/crates/protocol/af_observability/src/semconv/mod.rs`, `metrics/label/fleet.rs`, `metrics/label/tests.rs`, `producers/fleet/cache.rs` | EDIT, CREATE | §3: the `cache` label key, the `LeaseCache` and `CacheOutcome` sets, their product, and the recording function |
| `rustd/crates/daemon/afd_fleet/src/lease/installed.rs`, `coverage.rs`, `usage/mod.rs`, `deliver/window.rs`, `sql/select_fleet_config.sql` | EDIT, DELETE | §3: parse once per revision; ceilings and prices from the caches; the config read goes |
| `rustd/crates/daemon/afd_billing/src/price/mod.rs`, `src/rates.rs`, `src/sql/select_price_book.sql` | EDIT | §3: a book carries the catalogue version it was read at |
| `rustd/crates/daemon/afd_credential/src/provider/cache.rs` | CREATE | §3: the platform resolution and its zeroized key |
| `rustd/crates/daemon/afd_credential/src/provider/runtime/resolve.rs`, `registered/mod.rs` | EDIT | §3: resolve through the cache for the platform reference |
| `rustd/crates/daemon/afd_memory/src/memories.rs`, `hydrate.rs`, `access.rs` | EDIT | §3: the lease window hydrates from the scope the claim read |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql`, `assign/ledger.rs`, `envelope.rs`, `event.rs`, `pull/mod.rs`, `pull/refuse.rs`, `sql/fleet.rs`, `sql/queue.rs` | EDIT | §4: the claim reads the fleet, opens the event and stamps delivery |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/stamp_delivery.sql` | DELETE | §4: folded into the claim; `insert_leased_fleet_event.sql` stays for the task input receipt (`afd_fleet/src/task/input_receipt/answer.rs`) |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/select_gates.sql` | CREATE | §5: spend, recorded gate and grants in one read |
| `rustd/crates/daemon/afd_fleet/src/lease/admit/mod.rs`, `admit/gates.rs`, `afd_gate/src/gate/pass.rs`, `afd_gate/src/gate/grants.rs` | EDIT | §5: gates judge facts the read returned |
| `rustd/crates/daemon/afd_fleet/src/lease/issue.rs`, `sql/lease/insert_lease_with_event.sql`, `afd_runner/src/capacity/mod.rs`, `capacity/choice.rs` | EDIT | §5: census, ticket and choice inside the insert |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql` | CREATE | §6: the one report statement |
| `rustd/crates/daemon/afd_fleet/src/lease/commit.rs`, `settle.rs`, `finalize.rs`, `obligation.rs`, `report/mod.rs`, `report/steps.rs`, `report/tokens.rs`, `report/finalize.rs`, `report/assembly.rs`, `sql/report.rs` | EDIT | §6: one statement, its binder and its answer |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/select_lease_for_report.sql`, `select_run_tokens.sql`, `select_lease_disposition.sql`, `afd_events/src/sql/update_fleet_event_result.sql` | DELETE | §6: absorbed by the report statement |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/renew_batch.sql`, `renew/batch.rs` | CREATE | §7: one statement per host per tick |
| `rustd/crates/daemon/afd_fleet/src/lease/renew/mod.rs`, `renew/extend.rs`, `sql/renew.rs`, `sql/bill_expired_slice.sql`, `billing_sweep.rs` | EDIT | §7: the batch verb; the expiry statement advances the line count |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/select_lease_for_renew.sql`, `sql/meter_lease.sql`, `sql/meter.rs`, `sql/select_lease_cancelled.sql` | DELETE | §7: both callers of the meter replaced; the batch reads the cancel mark |
| `rustd/crates/daemon/afd_gateway/src/session/**` (the cache, the dispatch and `sql/select_runner_live_leases.sql`), `afd_http/src/services/leasing/**` | EDIT | §6, §7: the session's lease cache gains pricing facts and the funds tick, and the renewal dispatch calls the batch verb; §8: the leasing service loses the context load |
| `rustd/crates/foundation/af_core/src/timing.rs` | EDIT | §7: `FUNDS_CHECK_TICKS`, `EXPIRY_TICK` and their asserts |
| `rustd/crates/daemon/main/src/sweepers/clock.rs` | CREATE | §7: `sweeper:expiry` |
| `rustd/crates/daemon/main/src/sweepers/mod.rs`, `billing.rs`, `authoring.rs`, `afd_authoring/src/maintenance/mod.rs`, `main/tests/supervisor.rs` | EDIT | §7: the expiry passes move to one clock; inventory follows |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `metrics/declared/database.rs`, `producers/database.rs`, `docs/metrics.census.tsv` | EDIT | §3: `agentsfleet_lease_cache_lookups_total`; §8: `agentsfleet_db_pool_checkout_pings_total` |
| `rustd/crates/daemon/afd_fleet/tests/integration_lease_caches.rs`, `integration_renew_batch.rs`, `integration_report_statement.rs`, `afd_credential/tests/integration_platform_key_cache.rs` | CREATE | §3–§7 Dimensions |
| `rustd/crates/daemon/afd_fleet/tests/integration_report_renewal_lock.rs` → `integration_report_renewal_lines.rs`; `integration_lease_plan/meter.rs`; fixtures under `afd_fleet/tests/**` calling `record_received`, `load_for_report` or `claim_and_settle`; `afd_api/tests/runner_plane/decision_lifecycle/fixture.rs`; `afd_events/tests/integration_continuation_lineage.rs` | MOVE, EDIT | §4, §6, §7: the race test loses the host lock; plan tests reach the new statements |
| `rustd/crates/daemon/afd_db/src/pool/open.rs`, `pool/mod.rs`, `afd_db/tests/integration_pool_faults.rs` | EDIT | §8: the idle-gated checkout ping, its constant and its tests |
| `rustd/crates/protocol/af_wire/src/lease/payload.rs`, `af_wire/src/context.rs`, `af_wire/tests/redaction.rs`, `public/openapi.json` | EDIT | §8: the lease gains `inputs_waiting`; the context-load answer goes; the document is regenerated |
| `rustd/crates/daemon/afd_fleet/src/lease/answer/mod.rs`, `context.rs`, `afd_api_runner/src/handler/runner/context.rs`, `afd_http/src/route/runner.rs`, `afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `afd_api/tests/harness/stubs_runner.rs`, `main/tests/integration_runner_context.rs` | EDIT | §8: the lease carries the claim's flag; the context `GET` goes |
| `rustd/crates/runner/afr_agent/src/loop/conversation.rs`, `loop/mod.rs`, `loop/context/**`, `engine/context.rs`, `testing.rs`, `afr_agent/tests/**` | EDIT | §8: the first fold ask is conditional; no loop loads a checkpoint |
| `rustd/crates/runner/afr_supervisor/src/context.rs`, `client/context.rs`, `client/verb.rs`, `verbs/mod.rs`, `test_support/rig.rs`, `test_util/mod.rs`, `test_util/tests.rs` | EDIT | §8: the load verb goes; the inbox reads the lease's flag |
| `rustd/crates/daemon/afd_fleet/tests/integration_lease_folds/**` | EDIT | §8 Dimensions |
| `rustd/crates/daemon/afd_bench/src/statements/paths.rs`, `statements/tests.rs`, `lane/lease/drain/runner.rs`, `lane/lease/seed/tests.rs` | EDIT | §9: fragments follow; the drain counts memory windows and sends a run's fold asks |
| `bench/baselines/lease.rig.json` | EDIT | §9: the median run |
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
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, eight architecture pages change | sentences under 25 words, three-sentence paragraphs; scan before each commit |
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, `moka` joins `afd_fleet` from the workspace | workspace dependencies only; `lint-rustd` per Section |
| The statement-prepare test (SQL-PREPARE) | yes, three new statements and five rewritten | each file joins the prepared list in the commit that adds it |
| Metrics census and producer test | yes, two families added | census row, declaration and producer in one commit |
| `gitleaks` | yes, cache tests seed platform keys | keys derived at runtime from the test entropy source; no new allow entry |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `commit.rs`, `renew/mod.rs`, `event.rs` | binders in `renew/batch.rs` and `commit.rs`; answer decoding beside each statement's row type |
| OpenAPI document | yes, the runner's context `GET` goes | regenerate `public/openapi.json` from `rustd/` with `--no-banner` in the same commit |

## Prior-Art / Reference Implementations

- **Reference:** `afd_credential/src/credential/broker/cache.rs` — `moka` entries holding `Zeroizing<String>`, with a hand-written `Debug` that prints no secret. The platform key cache mirrors it; divergence: keyed by version, with no time-based expiry.
- **Reference:** `afd_fleet/src/lease/sql/meter_lease.sql` — the fence guard, the clamped measurement and the `FROM guard` writes. Both new statements keep them verbatim per lease.
- **Reference:** `afd_billing/src/ledger/sql/insert_lines.sql` — parallel arrays through `unnest`, the binding shape for many lines and many leases in one statement.
- **Reference:** `afd_fleet/tests/integration_report_renewal_lock.rs` — a renewal held mid-statement by a row lock on the activity counter, observed through `pg_stat_activity`; the line-numbering race test keeps the hold and drops the host lock.
- **Reference:** `docs/architecture/billing_and_provider_keys.md` §11 — why the funds check stays behind `Biller`.

## Sections (implementation slices)

### §1 — The reversals are written down first

One doc-only commit lands before any code (ARCH-LAND). `runner_fleet.md` replaces "config is never cached" (`:71`, `:803`, `:823`) with the rule: config is cached by its head revision id, which the claim reads, so the next run still obeys an edit. `concurrency.md` §"Why neither plane has a central deadline scheduler" gains one daemon expiry clock and one runner host timer; per-call timeouts stay as they are. `push_scheduler.md`'s renewal sequence loses "one host lock", and its M245 table row names the two columns. `billing_and_provider_keys.md` states the funds cadence. `lease_flow.md` and `postgres_schemas.md` follow. `scaling.md` states the checkout ping, and `runner_protocol.md` the lease field and the removed verb.

- **Dimension 1.1** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the eight pages.
- **Dimension 1.2** — DONE — no page keeps the old rule → Test R7's `git grep` (manual): prints nothing outside `push_scheduler.md`'s retirement table.

### §2 — Two version columns, and the writers that skipped a bump

`runner.leases` gains `daemon_line_count INTEGER`, the tally the meter's `daemon_lines` alias counts today, named per SQL-COUNTER. It is added `NOT NULL` with no backfill, because the dev cutover resets data. Its readers are `renew_batch.sql`, `commit_report.sql` and `bill_expired_slice.sql`, each of which reads and advances it under the lease row lock.

`vault.secrets` gains `resource_version BIGINT`, added `NOT NULL` with no backfill. A `BEFORE UPDATE` trigger sets it to the old value plus one. No trigger does this yet; `schema/20261108122600_fleet_approval_gates.sql` holds the nearest shape. The insert binds `SECRET_FIRST_VERSION` (SQL-DEFAULT). Its readers are the claim and the platform key cache. **Implementation default:** a trigger rather than a bump in each writer, because a vault writer added later then cannot skip it. Today a same-KEK (key-encryption key) replace in `update_secret.sql` keeps the row's id and `kek_version`; only the ciphertext hash `platform.sql` computes on every read notices it.

`platform_key.rs` `set` and `deactivate` run `bump_catalogue_version.sql` in their transactions, as `model.rs` does. The `afd_db` test seeds do not bump (Discovery, CHORE(open) and PLAN). The fleet `resource_version` gap closes by design: no cache keys on it, because the memory flags ride the claim (§3). Bumping it in `update_access.sql`, `pause_fleet.sql`, `stream_ready.sql` and `initial.sql` would move the fleet's ETag (entity tag, `afd_authoring/src/creation/patch.rs:45`), failing an editor's next save for no edit of theirs.

**File form:** before the first production deploy, each column is an edit to its table's file (`.orly/LOCAL.md`; Indy, Nov 08, 2026). After it, each is a new `<UTC>_<schema>_<table>_<what>.sql` registered in `migration.rs`, with a backfill, and Dimension 2.1 reopens. CHORE(open) records which form holds.

- **Dimension 2.1** — CUT — no backfill (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Dimension 2.2** — DONE — a same-KEK replace moves the version → Test `secret_version_moves_on_same_kek_replace` (integration): `update_secret` with an unchanged `kek_version` moves 1 → 2, a KEK rewrap (`update_secret_envelope.sql`) 2 → 3. Without the trigger, both stay 1.
- **Dimension 2.3** — DONE — a platform default change moves the catalogue → Test `platform_default_change_bumps_catalogue_version` (integration): `set` then `deactivate` move it by 2; a `set` refused for an uncatalogued model moves it by 0.

### §3 — Revision-keyed caches, with versions riding the claim

Each replica holds `moka` caches whose keys are newtypes of an id and a version. A key can only be built from a statement row, so no lookup by id alone compiles.

- **Definition:** the parsed definition, keyed by `head_revision_id`. Revisions are append-only (`schema/20261108122000_fleet_revisions.sql` trigger), so an id never names two documents. The claim returns the document beside its id. The cache saves the parse and serves every renewal's budget ceiling, which today re-reads `select_fleet_config.sql`.
- **Price book:** keyed by provider, model and `catalogue_version`.
- **Platform resolution:** provider, model, endpoint and the decrypted key in `Zeroizing`. Keyed by `catalogue_version` and the secret's id and `resource_version`. A delete-and-recreate gets a new id.
- **Tenant-model references:** always read (blocker 2).
- **Memory scope and payer:** no cache. The claim returns both memory flags and `fleet.fleets.tenant_id`. Workspaces are append-only (`schema/20261108120300_identity_workspaces.sql` trigger), so the select of the workspace's tenant goes.

The claim returns `head_revision_id`, `catalogue_version`, and the active platform default's secret id and `resource_version`. That last one is selected as `lock_platform_credential.sql` selects it. The renewal and report statements carry the `catalogue_version` their lines were priced at, and the head revision each ceiling came from. A mismatch writes nothing for that statement or that lease. The daemon reads the new value once and runs again, so a catalogue edit costs one book read and one rerun. **Implementation default:** versions ride the statements rather than an `af.invalidate.*` NATS (the core message broker M244 adds) subject. Every subject is at most once, and a lost invalidation would serve a stale price with no bound; the versions cost no statement. Capacity is a performance parameter. Definitions are weighed by document bytes, capped at 64 MiB per replica; books and keys at 1,024 entries. A miss is a read, never a fault. The platform key is cached under blocker 1's default.

- **Dimension 3.1** — a new head revision is obeyed on every replica → Test `config_cache_follows_new_head_revision` (integration): plane A leases on revision 1 ($10 a day). Plane B, a second plane over the same database, applies revision 2 ($1 a day). Plane A's next lease and next renewal judge $1, and no signal crossed between the planes.
- **Dimension 3.2** — a memory grant change is obeyed → Test `memory_scope_follows_grant_change` (integration): with `memory_reads_workspace` false, lease 1's window holds no shared entry; after `update_access` grants it, lease 2's window holds one.
- **Dimension 3.3** — a catalogue edit reprices → Test `catalogue_cache_reprices_after_bump` (integration): tick 1 prices output at 3,000,000 nanos per million tokens. An admin edit sets 6,000,000. Tick 2's first statement writes nothing, its rerun's line charges the new rate, and the tick costs exactly two more statements: the book read and the rerun.
- **Dimension 3.4** — a same-KEK key replace is obeyed → Test `platform_key_cache_misses_on_same_kek_rotation` (integration): lease 1 carries key A; `update_secret` to key B, same `kek_version`; lease 2 carries B, and the secret was read twice in total.
- **Dimension 3.5** — tenant-model references stay uncached → Test `tenant_model_reference_reads_every_lease` (integration): two leases on a tenant model run `entry.sql` twice.
- **Dimension 3.6** — DONE — a full cache reads again → Test `definition_cache_evicts_and_reads_again` (unit): a one-document cap with fleets A, B, A parses three times and returns three correct definitions.
- **Dimension 3.7** — a cached key never renders → Test `cached_platform_key_never_renders` (unit): the entry's `Debug` output holds no byte of the key.
- **Dimension 3.8** — DONE — lookups are counted → Test `cache_lookups_counted_by_outcome` (unit): a miss then a hit on one definition key moves `{definition, miss}` and `{definition, hit}` by 1 each, and no other series.

### §4 — The claim opens the delivery

`claim_admission_head.sql` already locks the slot and the head admission (`FOR UPDATE OF a SKIP LOCKED`, `:57`), so `lock_admission.sql`'s second lock on this path goes; park keeps its own. The claim now reads the fleet in the same statement. A fleet that is not `active` claims nothing: no fence moves and no row opens. It returns:
- the head revision document, id, `policy_semantics` and `runtime_model_ref`;
- the session context, `tenant_id`, both memory flags, the event's `opened_at` and §3's versions.

It also inserts the `fleet.events` row. Its conflict arm answers first delivery, repeat or terminal from the stored status, with no second read. It stamps `delivered_at`, and judges a redelivery from the prior value (`RETURNING WITH (OLD AS prior)`, as the claim's slot arm does). Read after the stamp, every claim would look like one.

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

It takes no host lock. READ COMMITTED re-reads a locked row's latest version, so the lease's `daemon_line_count` is current. The snapshot count in `meter_lease.sql` was not. The closing row's cost, its `budget_used_nanos` and the token sums add the lines this statement inserted. A common table expression (CTE) cannot see its own writes, nor the budget trigger's (`trg_usage_lines_bump_budget`). The statement guards on the posture, provider, model and `catalogue_version` it was priced at. **Implementation default:** the runtime line is priced in SQL, because its milliseconds are measured inside the statement; Dimension 6.6 holds that price equal to the rater's.

When the guard fails, the trailing row says whether the lease was already settled by this runner or fenced, as `select_lease_disposition.sql` decides today. A replay writes nothing. A fenced report still logs the runner's known request lines (blocker 4). Pricing facts come from the stream session, and blocker 6 names what else the report reads. A replica holding none reads the lease first, so the report costs two statements there.

- **Dimension 6.1** — one statement commits every write → Test `one_statement_report_commits_every_write` (integration): the event is `processed`, the session checkpointed, the slot released, the obligation owed, the runtime and token lines and the audit row present; the report path ran one statement.
- **Dimension 6.2** — a stale fence writes nothing → Test `one_statement_report_refuses_stale_fence` (integration): a reclaimed lease's report answers `Fenced`; no runtime line, the event row unchanged.
- **Dimension 6.3** — a replay returns the stored outcome → Test `one_statement_report_replay_returns_stored_outcome` (integration, 100 copies released together): 1 `Settled`, 99 `AlreadySettled`, one runtime line, one obligation.
- **Dimension 6.4** — a report and a renewal never share a number → Test `report_and_renewal_number_lines_distinctly` (integration): a renewal held mid-statement and a report released behind it write lines `BASE + k` and `BASE + k + 1`, both present. A count-based number drops one line to `ON CONFLICT DO NOTHING`.
- **Dimension 6.5** — the closing cost counts the report's own lines → Test `closing_cost_counts_the_reports_own_lines` (integration): 500 nanos already logged plus a 1,000-nano report line answers 1,500; the snapshot alone answers 500. `budget_used_nanos` moves by the same 1,000.
- **Dimension 6.6** — statement pricing equals the rater → Test `statement_lines_match_rater` (integration): for one slice, the report, batch and expiry statements write the `seq`, `charged_nanos` and price id that `Rater::rate` and `log_lines` write.
- **Dimension 6.7** — a wrong pricing guess writes nothing → Test `report_with_wrong_pricing_facts_writes_nothing` (integration): session facts naming another model write nothing; the read-and-retry settles once.

### §7 — One renewal statement per host per tick, on one expiry clock

M243 sends one renewal frame per host each `RENEWAL_TICK`, naming every live lease, and runs `LeaseService::renew` per entry, eight at a time (`afd_gateway/src/session/dispatch/renewal.rs`). This Section replaces that loop. `renew_batch.sql` binds parallel arrays, one row per lease and one per priced line. Each lease runs the renewal guard and measurement `meter_lease.sql` applies today, with two more checks:
- the runner's status, read without a lock, and the census. Renewal adds no occupancy, so a stale census is harmless.
- the fleet's spend against the ceilings the daemon passes, compared as `has_reached` compares them, and guarded on the head revision they came from.

M243's cancel mark (`runner.leases.cancel_requested_at`) still refuses a lease with `UZ-RUN-022`. Durable-task authority, which `renew/extend.rs` checks at commit today, follows blocker 3. A lease that passes extends both deadlines, logs its lines and advances `daemon_line_count`. Every write joins that lease's own guard row. A lease that fails one extends nothing and changes no other lease's answer; its known request lines follow blocker 4. An id that names another runner's lease is answered `UZ-RUN-006` alone, as today, and one that does not parse keeps the gateway's refusal. The statement returns its own instant, which tells max runtime from lost (`RenewAnswer::verdict`). A statement fault answers every lease retry, and the runner keeps them until their deadlines, as a 5xx does today.

Funds go through `Biller::check` once per distinct payer: on a session's first tick, then every `FUNDS_CHECK_TICKS` (3) ticks. A short payer's leases are left out of the statement and answered `UZ-RUN-012`. **Implementation default:** every third tick, because a two-tick chat then pays two thirds of a funds check (about 1.3 statements) instead of four statements. A run overruns by at most three 5 s slices, the small overrun Indy accepted. Checking on reconnect stops a runner from dodging the check by reconnecting.

`sweeper:expiry` runs, every `EXPIRY_TICK` (2 s), the expired-lease billing pass (from `sweeper:billing`) and authoring's attempt expiry (from `maintain`). The billing relay keeps settling lines; privacy purges keep their 10 s tick.

- **Dimension 7.1** — a host batch extends live leases and refuses a fenced one → Test `host_batch_extends_live_refuses_fenced` (integration): three leases, one reclaimed, one at `MAX_RUNTIME`, one live; the answers are lost, max runtime and extended, and only the live one logs lines. One statement ran.
- **Dimension 7.2** — a foreign or malformed id fails alone → Test `host_batch_answers_foreign_lease_lost_alone` (integration): two own leases, another runner's lease and `not-a-uuid`; the foreign one answers `UZ-RUN-006`, `not-a-uuid` the gateway's refusal, and two extend.
- **Dimension 7.3** — an over-budget fleet stops alone → Test `host_batch_refuses_over_budget_fleet_alone` (integration): spend exactly at one fleet's daily ceiling answers `UZ-RUN-015` without extending; one nano under extends; the other fleet extends.
- **Dimension 7.4** — funds are checked on cadence → Test `funds_checked_every_third_tick` (unit, paused time, `RecordingBiller`): over six ticks with two payers, it checks on ticks 1 and 4, once per payer. A payer short at tick 4 is absent from tick 4's statement and answered no credits.
- **Dimension 7.5** — a statement fault keeps every lease → Test `host_batch_fault_answers_retry` (integration, the statement on a closed connection): every lease answers retry, none lost.
- **Dimension 7.6** — a cancel-marked lease stops alone → Test `host_batch_refuses_cancel_marked_lease` (integration): of two leases, the one with `cancel_requested_at` set answers `UZ-RUN-022` and keeps its deadline; the other extends.
- **Dimension 7.7** — one clock runs every deadline pass → Test `expiry_clock_runs_every_deadline_pass` (unit): one tick runs both passes; the supervisor inventory lists `sweeper:expiry`.
- **Dimension 7.8** — a failed expiry pass waits → Test `expiry_pass_failure_waits_for_next_tick` (unit): an unreachable database logs `expiry_sweep_failed` with its `error_code`, and the task keeps running.

### §8 — Round trips the statement count does not see

The bench counts statements, and three round trips of a chat never reach its count. None needs a new table.

**Checkout ping.** `afd_db/src/pool/open.rs` builds every pool with sqlx's default `test_before_acquire`. Each checkout of an idle connection therefore sends a Sync and waits for its answer, at most 37 times a chat. The pools turn the default off and ping in `before_acquire` only when the connection sat idle for `PING_AFTER_IDLE` (1 s) or longer. A connection used inside that second is handed out untested. A backend lost inside it fails one statement; sqlx discards the connection, and the next checkout dials. **Implementation default:** 1 s, because one chat's statements are milliseconds apart, while a failover idles a pool for longer. Each ping moves `agentsfleet_db_pool_checkout_pings_total`.

**First fold ask.** A run asks the daemon for folded input before every model call and once after its answer (`guffbolt/src/drive/mod.rs`). Each ask is one `claim_inputs.sql`. The first follows the claim by one offer, so the claim (§4) answers it. The claim returns `inputs_waiting`, true when its head's conversation holds a later unsettled admission, foldable or not. It reads that through `idx_admissions_pending_conversation`. `LeasePayload` carries the flag, and a run whose lease says false skips the ask before its first model call. A message that lands after the claim's snapshot is claimed by the ask after the answer, which starts another model call. Indy accepted that change (Discovery). Today such a message joins the first call, across a window of gates, issue and offer. An absent flag reads as true, and durable tasks keep their closed inbox.

**Context load.** A loop loads its context checkpoint before its first model call: a `GET`, and a daemon transaction of two statements. The branch it asks for is one it just minted, the lease id for the root loop and a fresh identifier for a child (`afr_agent/src/loop/context/mod.rs`). The daemon scopes a checkpoint by lease and branch (`afd_fleet/src/lease/context.rs`), and only this run writes under either. So the load can only answer nothing, and it goes: the runner port's `load`, the client verb, the daemon's `GET` handler and `Leases::context_load`. The runner's `af_context` store answers a load with nothing, and `af_context`'s own `load` stays for the daemon. The checkpoint `PUT` stays. M247 §4 carries a reclaimed turn's checkpoint in the delivery, so it needs no runner load.

- **Dimension 8.1** — a backend lost inside the window fails one statement → Test `backend_lost_inside_the_window_fails_one_statement` (integration): a pool of one, its backend terminated, then a checkout at once. The first statement errors and the second succeeds on a new connection. Under sqlx's default the first succeeds, so a pool that still pings every checkout fails this test.
- **Dimension 8.2** — an idle connection is tested → Test `idle_connection_is_pinged_and_replaced` (integration): the same pool, terminated, then idle for 1.5 s. The next statement succeeds and the counter moves by 1; 100 back-to-back checkouts move it by 0.
- **Dimension 8.3** — the claim reports waiting input → Test `claim_reports_waiting_input` (integration): the claim of the first of two person messages in one conversation answers true. A lone message answers false, and so does one whose only later message sits in another conversation.
- **Dimension 8.4** — the first model call skips an answered ask → Test `first_model_call_skips_the_answered_fold_ask` (unit, a recording inbox): a lease saying false and one answering call record one claim, after the answer. A lease saying true, or carrying no flag, records two.
- **Dimension 8.5** — a message after the claim still folds → Test `message_after_claim_folds_after_the_answer` (integration): a lease issued with nothing waiting, then a person's message admitted before the report. The ask after the answer returns it, and a second model call reads it. A run that skipped every ask would settle with the message unread.
- **Dimension 8.6** — a loop starts without a load → Test `first_checkpoint_commits_on_a_branch_never_loaded` (integration): a lease's first `PUT` for its root branch is accepted with no `GET` before it, and the route refuses a `GET`.

### §9 — Measured: twenty statements per chat

`paths.rs` gains fragments for the new claim, gates, batch and report statements and drops the deleted ones. It restores the fragments M243's edits moved into `other`, and adds a `fold` path for `claim_inputs.sql`. The drain's runner counts leases that carried a memory window, as `drain_memory_windows_per_lease`. That exposes a hydrate failing in the drain, which would undercount `issue` (M241's `other` was 0.08). The runner also sends what a run sends around its one model call: the fold ask before it unless the lease says none waits, and the ask after it. Three interleaved runs follow a reset rig; the median goes to `bench/baselines/`, and `push_scheduler.md` §Baselines gains an M245 column. Projected per chat (unverified until this run): claim 2.3, gates 2.7, pricing 0, issue 3 to 4, host 1, meter 2, report 1, fold 1, shared 5.7, activity 0, and task 0 under blocker 3's default. That is about 19 to 20 against a ceiling of 20, so there is no margin; the lane prints every path against this projection.

- **Dimension 9.1** — a chat costs at most 20 → Test `make bench-lease PROFILE=rig` (integration): `drain_statements_per_lease` ≤ 20, `_pricing` < 0.05, `_other` < 0.1, `_fold` ≤ 1.05, `drain_memory_windows_per_lease` 1.0.
- **Dimension 9.2** — each new statement lands in its path → Test `new_statements_match_their_paths` (unit): each new statement's normalized text classifies as claim, gates, meter, report or fold, never `other`.
- **Dimension 9.3** — the drain counts windows → Test `drain_counts_leases_that_carry_a_window` (unit): three leases, two with windows, report 0.667.
- **Dimension 9.4** — the baseline is a median run → Test `baselines_are_median_runs` (manual): §Baselines lists three totals, and the committed file holds the middle one.
- **Dimension 9.5** — the chat got no slower → Test `make bench-lease PROFILE=rig` (integration): `drain_lease_p95_ms` ≤ 64.2 and `drain_report_p95_ms` ≤ 29.8, the committed baseline's values. A build that cut statements and added waits fails.

## Interfaces

- **Wire:** the renewal frame and its per-lease answers keep M243's shapes. Refusals keep `UZ-RUN-006`, `UZ-RUN-011`, `UZ-RUN-012`, `UZ-RUN-015`, M243's `UZ-RUN-022` and the max-runtime code. `LeasePayload` gains `inputs_waiting`, a boolean whose absence reads as true. The runner's context-load `GET` is removed, and the checkpoint `PUT` stays. No route or error code is added.
- **Schema:** `runner.leases.daemon_line_count INTEGER NOT NULL`; `vault.secrets.resource_version BIGINT NOT NULL`, moved by trigger on every update.
- **Timing (`af_core::timing`):** `FUNDS_CHECK_TICKS` = 3, asserted at least 1 and `FUNDS_CHECK_TICKS × RENEWAL_TICK < LEASE_TTL`; `EXPIRY_TICK` = 2 s, asserted below `LEASE_TTL` (TIM). `afd_db`'s pool gains `PING_AFTER_IDLE` = 1 s, asserted below its `IDLE_TIMEOUT`.
- **Internal:** `Acquired` carries the claim's fleet facts and versions. `PriceBook` carries its `catalogue_version`. `LeaseService::renew_batch(runner, frame)` answers one verdict per lease. `Memories::hydrate_scoped` takes the claim's scope. Cache keys build only from statement rows. The runner's `ContextCheckpoints` port keeps `commit` only.
- **Metric:** `agentsfleet_lease_cache_lookups_total{cache, outcome}`, with `cache` ∈ `definition`, `price_book`, `platform_key` and `outcome` ∈ `hit`, `miss`. Also `agentsfleet_db_pool_checkout_pings_total{role}`.

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
| Foreign or malformed id (malformed input) | failure | runner defect | that entry refused alone | `host_batch_answers_foreign_lease_lost_alone` |
| Fleet killed mid-run | failure | M243's cancel mark | `UZ-RUN-022` for that lease only | `host_batch_refuses_cancel_marked_lease` |
| Fleet over budget (exceeded quota) | capacity | spend reached a ceiling | `UZ-RUN-015` for that lease only | `host_batch_refuses_over_budget_fleet_alone` |
| Tenant out of funds (exceeded quota) | capacity | balance short | `UZ-RUN-012` within three ticks | `funds_checked_every_third_tick` |
| Batch statement fails (timeout, network blip) | failure | Postgres error or timeout | every lease retries; none lost | `host_batch_fault_answers_retry` |
| Expiry pass fails (dependency unavailable) | failure | database down | logged; the next tick retries | `expiry_pass_failure_waits_for_next_tick` |
| Backend lost inside the ping window (network blip) | failure | failover or restart within 1 s of a connection's last use | one statement fails with the connection's error; the connection is discarded and the next checkout dials | `backend_lost_inside_the_window_fails_one_statement` |
| Idle connection dead (dependency unavailable) | failure | backend closed while the connection sat idle | the checkout pings, discards and dials; the caller sees nothing | `idle_connection_is_pinged_and_replaced` |
| Message lands after the claim's snapshot (race) | failure | a person sends between claim and first model call | the ask after the answer claims it; one more model call | `message_after_claim_folds_after_the_answer` |
| Lease carries no flag | failure | an older daemon | the run asks before its first call, as today | `first_model_call_skips_the_answered_fold_ask` |

## Invariants

1. **A cache answers only for a version a statement in the same pass read.** Cache keys are newtypes whose only constructors take a statement's row. No lookup by bare id compiles. A wrong version misses, never serves. `config_cache_follows_new_head_revision` is the shortest counterexample.
2. **A statement priced from a cached book writes nothing once the catalogue moved.** `renew_batch.sql` and `commit_report.sql` compare `model.catalogue_version` with the bound priced-at version in their guards. The race is the one today's pre-transaction book read already has. Test: `catalogue_cache_reprices_after_bump`.
3. **A lease's runtime lines carry distinct numbers.** `daemon_line_count` is read and advanced under the lease's `FOR UPDATE` lock by every writer of daemon-range lines. `uq_usage_lines_work_type_work_id_seq` backs it. Test: `report_and_renewal_number_lines_distinctly`.
4. **A lease that fails its guard in a batch extends nothing and moves no other answer.** Every write in `renew_batch.sql` joins its lease's guard row (`FROM guard`), the pattern `meter_lease.sql` uses today. Test: `host_batch_extends_live_refuses_fenced`.
5. **`vault.secrets.resource_version` moves on every update.** The table's `BEFORE UPDATE` trigger enforces it, whichever writer runs. Test: `secret_version_moves_on_same_kek_replace`.
6. **A cached platform key is never rendered and is zeroed when dropped.** The value is `Zeroizing<String>`, and the entry's `Debug` is written by hand. Test: `cached_platform_key_never_renders`.
7. **A run never settles while claimable input waits unasked.** The ask after every answer is unconditional in `guffbolt`'s drive loop. Only the ask before the first model call reads the lease's `inputs_waiting`. Test: `message_after_claim_folds_after_the_answer`.
8. **A connection idle for `PING_AFTER_IDLE` or longer is tested before it is handed out.** The pool's `before_acquire` hook reads sqlx's idle time and pings; a failed ping discards the connection. Test: `idle_connection_is_pinged_and_replaced`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_lease_cache_lookups_total` | operator | each cache lookup | `cache`, `outcome` | no fleet, tenant or key value in labels | `cache_lookups_counted_by_outcome` |
| `renew_batch_repriced` (log) | operator | a guard forced a rerun | `runner_id`, `leases`, `reason` | no prices, no key material | `catalogue_cache_reprices_after_bump` |
| `expiry_sweep_completed`, `expiry_sweep_failed` (log) | operator | each expiry tick | counts, `error_code` | identifiers only | `expiry_pass_failure_waits_for_next_tick` |
| `renew_no_credits`, `renew_budget_breach` (log, unchanged spellings) | operator | a batch refuses a lease | `fleet_id`, `lease_id` | identifiers only | `funds_checked_every_third_tick`, `host_batch_refuses_over_budget_fleet_alone` |
| `agentsfleet_db_pool_checkout_pings_total` | operator | a checkout pings an idle connection | `role` | role names only | `idle_connection_is_pinged_and_replaced` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A chat costs at most 20 statements (§9) | `jq -e '.measurements.drain_statements_per_lease <= 20' bench/baselines/lease.rig.json` | `true`, exit 0 | P0 | |
| R2 | Pricing sends nothing after warm-up (§3, §9) | `jq -e '.measurements.drain_statements_per_lease_pricing < 0.05' bench/baselines/lease.rig.json` | `true`, exit 0 | P0 | |
| R3 | Every cache follows its version (§2, §3) | `make test-integration-rustd` | exit 0; `config_cache_follows_new_head_revision`, `memory_scope_follows_grant_change`, `catalogue_cache_reprices_after_bump`, `platform_key_cache_misses_on_same_kek_rotation` print `ok` | P0 | |
| R4 | A host batch renews live leases and refuses a fenced one (§7) | `make test-integration-rustd` | exit 0; `host_batch_extends_live_refuses_fenced ... ok` | P0 | |
| R5 | The one-statement report is fenced and replay-safe (§6) | `make test-integration-rustd` | exit 0; `one_statement_report_refuses_stale_fence` and `one_statement_report_replay_returns_stored_outcome` print `ok` | P0 | |
| R6 | A report and a renewal never share a line number (§6) | `make test-integration-rustd` | exit 0; `report_and_renewal_number_lines_distinctly ... ok` | P0 | |
| R7 | No page or comment keeps "config is never cached" (§1) | `git grep -n -i -e "config is never cached" -e "no in-memory config cache" -e "re-read per lease and never cached" -- docs/architecture rustd/crates/daemon/afd_fleet/src ':!docs/architecture/push_scheduler.md'` | no output | P1 | |
| R8 | Only the issue locks the host in `afd_fleet` (§6, §7) | `git grep -n -e "capacity::lock(" -e "lock_host_capacity(" -- rustd/crates/daemon/afd_fleet/src` | four lines: three in `store.rs`, one in `issue.rs` | P1 | |
| R9 | The replaced statements have no caller (§4, §6, §7) | `git grep -n -w -e SELECT_LEASE_FOR_RENEW -e SELECT_LEASE_FOR_REPORT -e SELECT_RUN_TOKENS -e SELECT_LEASE_DISPOSITION -e SELECT_FLEET_CONFIG -e STAMP_DELIVERY -e METER_LEASE -e UPDATE_FLEET_EVENT_RESULT -- rustd` | no output | P1 | |
| R10 | The diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R11 | A chat's first model call pays no uncounted round trip (§8) | `make test-integration-rustd` | exit 0; `backend_lost_inside_the_window_fails_one_statement`, `claim_reports_waiting_input` and `message_after_claim_folds_after_the_answer` print `ok` | P0 | |
| R12 | The chat got no slower (§9) | `jq -e '.measurements.drain_lease_p95_ms <= 64.2 and .measurements.drain_report_p95_ms <= 29.8' bench/baselines/lease.rig.json` | `true`, exit 0 | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `select_lease_for_renew.sql`, `load_for_renew`; `select_lease_cancelled.sql` | `git grep -n -w -e SELECT_LEASE_FOR_RENEW -e load_for_renew -e LOCK_CANCELLED -- rustd` | no output |
| `select_lease_for_report.sql`, `load_for_report` | `git grep -n -w -e SELECT_LEASE_FOR_REPORT -e load_for_report -- rustd` | no output |
| `select_run_tokens.sql`, `run_tokens`; `select_lease_disposition.sql` | `git grep -n -w -e SELECT_RUN_TOKENS -e run_tokens -e SELECT_LEASE_DISPOSITION -- rustd` | no output |
| `select_fleet_config.sql`, `stored_config` | `git grep -n -w -e SELECT_FLEET_CONFIG -e stored_config -- rustd` | no output |
| `stamp_delivery.sql`, `record_received` | `git grep -n -w -e STAMP_DELIVERY -e record_received -- rustd` | no output |
| `meter_lease.sql`, `meter.rs`; `update_fleet_event_result.sql` | `git grep -n -w -e METER_LEASE -e UPDATE_FLEET_EVENT_RESULT -- rustd` | no output |
| the runner's context load: `context_load`, `ContextLoad`, `ContextLoaded` | `git grep -n -w -e context_load -e ContextLoad -e ContextLoaded -- rustd public` | no output |

`select_fleet_with_session.sql`, `select_access.sql`, `select_tenant_for_workspace.sql`, `lock_admission.sql`, `release_fleet_slot.sql`, `delete_other_fences.sql`, `select_reply_destination.sql`, `census.sql` and `record_choice.sql` keep callers off this path: the message path, the runner's memory verbs, the wallet, park, affinity, tool detail and authoring. CHORE(open) re-runs every row against `origin/main`, because M243 and #803 moved files on this path.

Schema this milestone leaves with no runtime reader or writer is dropped in this Pull Request, in the file form §2 states, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- Activity posts: M243 removes the per-post target read.
- Memory capture at report: a separate role (`assume_memory_role.sql`) keeps its own statements.
- Authoring attempts in the host batch: they renew over the stream on their own verb (`afd_gateway/src/session/dispatch/authoring.rs`), outside the lease batch.
- The funds read inside the gates and batch statements: billing §11 forbids it today, and reversing §11 is Indy's call. It would save about three statements per chat: the issue's bound and balance, and the renewals' share.
- One deadline column per lease: the slot's deadline is the claim's reclaim test, and both columns are written from one value in one statement.
- A NATS invalidation subject: versions ride the statements (§3).
- Caching tenant-model references: blocker 2's "yes" path, which adds a slot.
- The liveness sweep: M247 rebuilds host death around the stream.
- The fold ask before later model calls and after the answer: each is the backstop for a lost command (`push_scheduler.md` §When a signal is lost).
- The Send hop through the app's route, and provider prompt-cache reuse across turns: both wait on M233_002's stage timing.

---

## Product Clarity (authoring record)

N/A: no user surface. No page, command or error code changes. The runner protocol loses one private verb and its lease gains one field (§8). The one billing-visible effect, a credit stop within three renewals, is a changelog entry (§7).

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** reversals on paper first, then the schema the caches and line numbers stand on, then each verb collapsed in the order a chat runs: claim, gates and issue, report, renewal, then the round trips the count misses. The measurement closes it, so each Section's statements are counted before the next lands.
- **Alternatives considered:**
  - An `af.invalidate.<kind>.<id>` NATS subject: rejected, because it is at most once and a lost message serves stale config with no bound. The versions it would carry already ride statements we run.
  - Time-to-live caches: rejected, because an invisible staleness window is why `runner_fleet.md` banned caching.
  - The funds read inside the statements: rejected by default under billing §11.
  - A per-lease renewal loop: rejected, because one statement per lease per tick is the cost being removed.
  - Keeping the host lock for numbering: rejected, because it serialises every renewal and report on a host.
  - A checkpoint answer carried in the lease: rejected, because a run can never find one under a branch it just minted, so there is nothing to carry.
  - Dropping the checkout ping outright: rejected, because a pool idle through a failover would hand out every dead connection.
- **Patch-vs-refactor verdict:** a **refactor**, because the statement structure of four verbs changes while every user-facing shape stays.

## Discovery (consult log)

- **Consults** — source review on `main` `14539a73e` and the M241 branch, Nov 06, 2026:
  - Renewal and report take the host lock only so `daemon_lines` counts the other's committed line (`sql/meter.rs:35-44`, `commit.rs:145`, `renew/extend.rs:62`). A duplicate number loses a runtime line to `ON CONFLICT DO NOTHING` (`insert_lines.sql`; `integration_report_renewal_lock.rs` header). `bill_expired_slice.sql:23` counts the same way.
  - The census runs after the host lock as its own statement for snapshot freshness (`afd_runner/src/capacity/mod.rs:1-5`).
  - The claim already locks the head admission (`claim_admission_head.sql:57`); `record_received` locks it again (`event.rs`, `lock_admission.sql`).
  - Revisions and workspaces are append-only by trigger (`schema/20261108122000_fleet_revisions.sql:30-38`, `schema/20261108120300_identity_workspaces.sql:25-33`).
  - `platform_key.rs` `set` and `deactivate` change platform defaults without `bump_catalogue_version.sql`. Only `model.rs:268` bumps.
  - `vault.secrets` has no version column (`schema/20261108121000_vault_secrets.sql`), and `platform.sql` hashes the ciphertext on every read.
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
- **Owner decisions** —
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
  > Indy (Nov 10, 2026): "1. yes amend 245 2. amend the runner change in the 245 spec" — context: asked which of the checkout ping and the run's first-call round trips to take; both land here as §8.
  > Indy (Nov 10, 2026): chose "Keep one spec" — context: asked whether §8 should move to an M245_002, with this spec at its size ceiling; it stays here.
  > Indy (Nov 10, 2026): chose "Accept all six (Recommended)" — context: asked whether to accept the implementation defaults of blockers 1 to 6 below.
  > Indy (Nov 10, 2026): chose "Accept (Recommended)" — context: asked whether a message landing between the claim and the first model call may get a second model call (§8).
  > Indy (Nov 10, 2026): chose "Rewrite §1 for the gateway now" — context: M233_002 §1 opens spans in `afd_gateway/src/session/**` and marks stages in `lease/claim.rs`, `deliver/**` and `report/mod.rs` while this spec rewrites them. Whichever merges second rebases onto the other's shape.
- **Model reviews** — the programme brief's two EGM passes are recorded in M241_001's Discovery. EGM implementation-readiness and codebase-details reviews ran from a Claude Fable 5.2 session on Nov 10, 2026, against main `51d41a9e3`: 18 findings (10 and 8, with overlap). The stale cites, paths and grep rows are fixed in place. The design questions are blockers 3 to 6 and §8's fold window, which Indy answered the same day. Open for PLAN: the helper the statement-count Dimensions count with, since no integration test has one today.
- **Measured in M244 (Nov 10, 2026)** — two paths outside this spec's chat path, counted with `pg_stat_statements` over one `make bench-<lane> PROFILE=rig` run each.
  - Outbound, 200 replies on one lane: 4 statements per reply, `lock_lanes.sql`, `claim_heads.sql`, `count_attempt.sql` and `stamp_delivered.sql` (`afd_outbound/src/obligation/sql/`), 200 calls each and 228 ms of execution in total. A reply takes about 33 ms against about 20 ms on the previous stream reader, so execution explains about 1 ms of the 13 ms added; the rest is about 6 serial round trips. One statement for the lock, claim and attempt count would leave 2.
  - Steer, 20,227 steers: 2 statements per steer, the intake and `afd_admission/src/sql/run_facts.sql` (4,828 ms in total), which `323764add` added on Nov 08 for the steer's disposition (`afd_events/src/steer/mod.rs:231`). The 1.003 baseline predates it. Its facts could ride the intake's `RETURNING`.
  > Indy (Nov 10, 2026): chose "Note in M245 spec" — context: both findings land here as Discovery, not Sections.
- **Measured on main (Nov 10, 2026)** — the rig's `pg_stat_statements` after the three drains at `6b0caea35`, 600 chats (`scaling.md` §Core NATS baselines):
  - The 74 statements execute in 13.2 ms per chat in total. The candidate scan is 3.2 ms of it, the meter 2.0 ms and the usage-line insert 1.6 ms.
  - `other` (8.1) is the renewal's lease read and cancel lock (2 each), the report's lease read, the ticket read, one event read, the other-fences delete and a savepoint pair.
  - sqlx 0.9.0 pings on every checkout by default (`sqlx-core` `pool/options.rs`, `pool/inner.rs`), and `afd_db/src/pool/open.rs` does not override it. A chat makes at most 37 checkouts, one per commit.
  - A run's fold ask is one statement, and its context load a transaction of two (`afd_fleet/src/lease/context.rs`). The default drain sends neither.
  - Dev Postgres is in `aws-us-east-2` and the daemon in Fly `iad` (`scaling.md` §The infra reality first), so each round trip there crosses a region.
- **Blockers, answered** — Indy accepted every implementation default below (Owner decisions, Nov 10, 2026):
  1. **May decrypted platform keys be cached in daemon memory?** This changes the security posture: a key lives past its lease. **Implementation default:** cache the resolution with the key in `Zeroizing`, re-decrypt only when the secret's id or `resource_version` moves, and zero it on eviction. Precedent: the broker caches connector tokens the same way, and a platform host's lease already carries the key to the runner. M246 keeps the key inside the daemon for tenant runners and re-resolves it on every proxied model call (M246_001 §4), which this cache serves. "No" keeps the resolution cache and re-reads the secret per lease, one statement per chat.
  2. **Is `model.tenant_models` cached or always read?** It has no version column (`schema/20261108121500_model_tenant_models.sql`). **Implementation default:** always read. A fleet on a tenant model then pays four statements per issue (`lock_catalogue.sql`, `lock_entry_credential.sql`, `entry.sql`, `select_secret.sql`), which the platform-default bench does not see. "Cache" adds a `resource_version` column bumped by the four entries writers, and the claim returns it.
  3. **Do durable-task reads and settlement, and fold settlement, join the new statements?** #803's reads cost 6 statements a chat (the `task` path), and §9's projection counts them as 0. The report also settles a task (`task::settle_report`, a state transition with a staged admission) and its folds (`settle_folds.sql` and two event statements), which §6's five steps omit. **Implementation default:** the claim, the batch and the report carry the task guard each verb checks today, and a stopped task answers as `Renewed::Stopped` does now. Task and fold settlement stay separate statements in the report's transaction, so only a plain chat's report is one statement. "No" keeps the reads and puts a chat at about 25, over the ceiling.
  4. **Do refused passes keep logging the runner's known request lines?** A fenced report (`commit.rs:165-186`) and a refused renewal (`renew/mod.rs:234-240`, `renew/extend.rs:97-119`) log them today, under the host lock (`usage/original.rs`). **Implementation default:** yes, so billing is unchanged: each refusing arm logs them under the lease row lock. "No" is a billing change.
  5. **What proves the stream at renewal, and what does a payer's funds check ask with?** The host lock proves the opening credential and the connection owner today (`afd_runner/src/capacity/mod.rs:136-153`). `Ask` carries one lease's work key and model floor (`afd_billing/src/biller/mod.rs:90-96`). **Implementation default:** each lease's guard joins the runner row and applies the same authority predicates without the lock, and a payer asks once with its dearest lease in the frame. The fault postures of `coverage.rs:104-147` carry over unchanged.
  6. **What does a report read when its session holds no facts?** `select_lease_for_report.sql` goes, yet memory capture, the stored trace and task preparation read the lease before the commit (`report/assembly.rs`, `report/memory.rs`). **Implementation default:** the session record gains the `Reported` row's fields at offer and rebuild, and the fallback read keeps that statement's shape under a new name.
- **CHORE(open) and PLAN, Nov 10, 2026** — Indy's start instruction: "Start M245. Work only in ~/Projects/agentsfleet-m245, on branch feat/m245-chat-costs-twenty-statements. Use the orly-build skill." Every path the spec cites exists on `291777075`.
  - **File form (§2):** no production deploy has happened. The last `release.yml` run is Apr 07, 2026, under the old organisation, before the `schema/20261108*` files existed (`gh run list -w release.yml`). Every later deploy is a `deploy-dev-*` workflow. So each column edits its table's file, and `migration.rs` does not change.
  - **Statement counter (Dimensions 3.3, 3.4, 3.5, 4.1, 4.3, 5.1, 5.3, 6.1, 7.1):** four tests hand-roll a `pg_stat_statements` read today (`afd_events/tests/integration_intake_cost.rs:22`, `queued/snapshot.rs:7`, `main/tests/integration_runner_stream/measurements/support.rs:77,101`). §3 adds one shared `afd_db::test_util::statements` behind the `test-util` feature, built first. It runs on a private database from `TestDatabase::create`, because the extension counts per database and the shared lane's parallel suites would land in the count. `Window::open` reads calls per query id, and `close` returns the calls that ran since. The read keeps top-level statements only, so a trigger's statements are not round trips. It drops transaction control, as the bench does (`afd_bench/src/statements/queries.rs:22`), and the owner role's calls, so seeds and the counter's own reads stay out. A test asserts the window's total and each statement's calls, matched by its source constant's text. The four existing copies keep their reads; moving them adds work this spec does not need.
  - **`daemon_line_count` default:** the column is `NOT NULL DEFAULT 0 CHECK (daemon_line_count >= 0)`, as `fleet.events.task_activity_sequence` is (`schema/20261108122400_fleet_events.sql:33`). A tally starts at 0, and SQL-DEFAULT allows a numeric default. The default leaves the eleven test fixtures that insert lease rows unchanged. `vault.secrets.resource_version` has no default, as §2 states, and every insert binds `SECRET_FIRST_VERSION`.
  - **The re-seal writer:** Dimension 2.2's rewrap is `afd_credential/src/vault/sql/update_secret_envelope.sql`, reached through `Vault::rotate_refresh_token`. It rewrites `kek_version` with the rest of the envelope. A `deactivate` that matches an already inactive default still reports a change and bumps; one extra cache miss is harmless, so the statement is unchanged.
  - **Test seeds do not bump the catalogue (deviation from §2):** a bump in `afd_db`'s platform-default seeds failed two `afd_api` `decision_lifecycle` tests with `503 UZ-DT-006`. `platform.sql` folds the global `catalogue_version` into each lease's `resolution_version`. On the shared lane, a parallel test's seed then moved every other test's resolution mid-run. The platform key cache keys on the default's secret id and `resource_version` too, so a seeded default misses without the bump. The seeds stay as they were; `platform_key.rs` `set` and `deactivate` bump.
  - **§3 lands in steps (Invariant 1):** a cache answers only for a version a statement in the same pass read. The claim read already returns the revision id beside the document, so the definition cache, its key type and the lookup metric land first, wired into `installed_on` (Dimensions 3.6, 3.8, and 1.2 with the rewritten `installed.rs` comment). The price-book and platform key caches land with the statements that carry their versions: §4's claim returns the catalogue version and the platform secret's version (Dimensions 3.2, 3.4, 3.5, 3.7), and §7's renewal statement guards on them (Dimensions 3.1's renewal half and 3.3). Built earlier, those two caches would be dead code, and wired earlier they would have nothing to catch a stale price.
  - **Lane observations, Nov 10, 2026 (shared 8-core, 16 GB machine):** four full `make test-integration-rustd` runs went red on four different tests, each in a suite this Section does not touch. Two were mine and are fixed: a stray `)` in the platform-default seed, and the seed bumping the catalogue (above). The others were timing: `55P03` lock timeouts under the API login's 2 s `lock_timeout`, a 10 s `DROP DATABASE … WITH (FORCE)` bound in `integration_tasks/fixture.rs:248`, and the shared-sweep `closed_retention_waits_for_a_live_authoring_lease_then_purges_at_expiry`. The load average read 9 to 23 with the M233 agent compiling beside the lane and 6.7 GB in the memory compressor. At `--test-threads=3` on a freshly reset database, `afd_fleet`'s `fleet_suite` passed 375 of 375, `afd_vault` 22 of 22 and `agentsfleetd`'s `daemon_suite` 150 of 150. The gate run (`orly gate pr`) needs the machine to itself.
  - **R12 margin:** `drain_lease_p95_ms` is 64.191 against a bar of 64.2. The last three re-baselines moved it 44.095, 57.855, 64.191 (`git log -- bench/baselines/lease.rig.json`), so 0.009 ms is inside run-to-run noise. §9 runs main and this branch back to back on a reset rig, three each, with no other lane running, and records both medians and spreads. R12 stays graded against 64.2 as written. A miss inside main's own spread is reported with both sets of numbers, and the bar is Indy's call then.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
