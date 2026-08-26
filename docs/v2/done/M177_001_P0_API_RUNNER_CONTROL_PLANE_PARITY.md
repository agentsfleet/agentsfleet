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

# M177_001: Runner control plane parity — a Zig runner completes leases against the Rust daemon

**Prototype:** v2.0.0
**Milestone:** M177
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** DONE
**Priority:** P0 — the demoable core of the port; cutover is impossible without it
**Categories:** API
**Batch:** B3 — serial after M176; M178/M179 fan out after it
**Branch:** `feat/m177-runner-control-plane-parity`
**Test Baseline:** `unit=1072 integration=57` — `make test-unit-all` (cargo 560 passed / 57 ignored, plus 512 TypeScript coverage-gate tests across 55 files) and `make test-integration-rustd` (57 passed), both exit 0 on `165d96201`
**Depends on:** M176_001 (substrate crates, boot, auth primitives); M175_001 (afd_wire fixtures)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` + `docs/architecture/data_flow.md` §C. EXECUTE

---

## Overview

**Goal (testable):** an unmodified production Zig `agentsfleet-runner` completes register → heartbeat → lease → activity → report against `agentsfleetd-rs`, with fencing, billing debits, memory writes, and lease bookkeeping row-equivalent to the Zig daemon on the same inputs.
**Problem:** the runner plane is the daemon's hardest-invariant surface — fencing tokens, at-most-one-lease-per-fleet, money gates, secret placement — and the cutover claim is empty until a real runner drives real leases through the Rust daemon.
**Solution summary:** port the `/v1/runners` verb set (register, me, heartbeat, lease, renew, report, activity, credentials/mint, memory, bundles) plus the `afd_fleet` service behind it (assignment, billing gates, the 12 hot-path writes as verbatim SQL), the fleet-config resolution layer (`afd_fleet_runtime`), the four runner-plane background sweepers, and the runner metric families — proven by the existing integration suite running against the Rust binary.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): runner control plane with lease/report parity
- **Intent (one sentence):** the execution plane cannot tell the daemons apart — a stock Zig runner leases, executes, and reports through `agentsfleetd-rs` with identical rows, codes, and money movements.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/data_flow.md` §Steer flow end-to-end + §C. EXECUTE — the 12 hot-path writes in the worker's order (`lease` does 1–6, `report` does 7–12) and the row-equivalence cutover invariant this milestone re-proves.
2. `docs/architecture/runner_fleet.md` — lease lifecycle, `LEASE_TTL_MS`/`MAX_RUNTIME_MS`, money gates, reclaim sweep bounds, runner state model.
3. `docs/AUTH.md` §Runner token (`agt_r`) — separate `runnerBearer` middleware, no verdict memoization (read-per-request is the revocation channel), Postgres outage → 503 as transport class.
4. `src/agentsfleetd/fleet/` — `service.zig`, `assign.zig`, `service_billing.zig` and the domain `sql.zig` files whose statements port verbatim.
5. `src/agentsfleetd/fleet_runtime/config_parser.zig` + `credentials/broker.zig` — fleet-config resolution and the cached mint broker this milestone ports.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_fleet/**` | CREATE | lease assignment, billing/budget gates, report fencing, sweepers, memory store |
| `rustd/crates/afd_fleet_runtime/**` | CREATE | fleet config parsing (YAML frontmatter + markdown), ExecutionPolicy build, metering |
| `rustd/crates/afd_credentials/**` | CREATE | on-demand credential broker: cached short-lived integration-token minting from vault handles |
| `rustd/crates/afd_api/**` | EDIT | Route variants + handlers for the `/v1/runners` verb set; `runnerBearer` layer wiring |
| `rustd/crates/afd_auth/**` | EDIT | `runnerBearer` validator (`agt_r` prefix, timing-safe hash lookup, no memoization); `Digest::of_minted` so the minter and the verifier hash through ONE function |
| `rustd/crates/agentsfleetd/**` | EDIT | sweeper tasks join the supervisor; runner metric families registered |
| `rustd/crates/afd_identity/**` | EDIT | `ring` → `aws-lc-rs` (4 lines in `jwks/verifier.rs`, plus the signing test) so the binary carries ONE crypto provider once `object_store` forces `aws-lc-rs` |
| `rustd/crates/afd_core/**` | EDIT | `Uuid7::encode` — version-7 minting beside the canonical-spelling parser that already lives there; the Zig `id_format.zig` has no Rust counterpart and an encoder anywhere else would re-derive the dash and nibble offsets |
| `rustd/crates/afd_wire/**` | EDIT | registered in `workspace.dependencies`; no wire shape is touched (M175 owns those) |
| `rustd/crates/afd_redis/**` | EDIT | `EventId::of` — the report acknowledges a stream entry long after the poll that read it, from a different request, so the id has to be reconstructible from the `fleet.runner_leases.event_id` text that survives in between. Deliberately a named constructor rather than a `From<&str>`, which would let any string in the program become a stream id (§3) |
| `rustd/crates/afd_db/**` | EDIT | `Db::unreachable` behind `test-util` — a pool over a datastore that answers nothing, so §1's transport-class refusal (RULE ECL) is provable through the real router with no container to stop |
| `rustd/Cargo.toml` + `rustd/Cargo.lock` | EDIT | new members; `uuid` (v7 minting) and `hex` registered — both already resolved in the lock, so neither is a new crate |
| `make/test-integration-rustd.mk` | EDIT | end-to-end lane driving a stock Zig runner against `agentsfleetd-rs` (§7.1); no daemon-under-test selector — only one daemon remains |
| `src/build/**` | EDIT | test-build wiring so the stock Zig runner binary is available to the §7.1 lane |
| `tests/**` | EDIT | seeded deterministic scenario set backing `test_seeded_row_shapes` |
| `PROMPT_M182_SUBSTRATE_SPEC.md` | CREATE, then DELETE at CHORE(close) | paste-ready authoring prompt for the substrate-abstraction spec; carried on the branch so the spec-authoring agent read it here rather than from a sibling worktree. It did its job — the spec it produced is `docs/v2/pending/M182_001_P1_API_DOCS_UI_EXECUTION_SUBSTRATE_ABSTRACTION.md` — so the prompt is removed rather than left behind as a second, staler account of the same milestone (Indy, this stream) |
| `docs/v2/pending/M181_001_P0_API_DOCS_INFRA_RUST_CUTOVER_SOAK.md` | EDIT | §5 added: the Rust daemon exports no telemetry — M176 §6 shipped the machinery and deferred the transport to boot, where it never landed. Found here, owned there, because it blocks M181's own Dimension 4.3 rather than anything this milestone ships (Indy, this stream) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NSQ (schema-qualified verbatim SQL), CTM (timing-safe token compares), ECL (transport loss ≠ auth rejection — the 503/401 split is load-bearing for the runner's backoff), KYS (keyset cursor on `(updated_at, id)` in the reclaim sweep), OWN, FLS, UFS, NDC, TST-NAM, MSID, ERR (UZ-RUN-* codes referenced, never re-declared), FLL.
- `dispatch/write_rust.md` — deterministic contention tests for every atomic claim (fencing, reclaim, affinity); preserved error variants.
- `docs/AUTH.md` — auth-flow rule: read before the `runnerBearer` and mint work.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | one handler module per verb; sweepers split per concern |
| LOGGING | yes | scoped events with UZ-RUN-* codes; lease payloads and secrets never logged |
| MILESTONE-ID | yes | none in source or tests |
| UFS | yes | TTL/backoff/batch constants named, single-sourced (mirroring `src/lib/common/constants.zig` values) |
| SCHEMA GUARD | no | no schema change — row shapes are the parity target |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/fleet/` + `src/agentsfleetd/http/handlers/runner/` (Zig daemon) — behaviour and SQL source of truth; statements port verbatim, wrapped in sqlx.
- **Reference:** `~/Projects/oss/exonum` — testkit pattern (real HTTP over the production router, no timers) shapes the verb-level tests; its `after_commit`-hook periodic work is NOT copied — sweepers stay explicit supervised tasks.
- **Reference:** `docs/architecture/runner_fleet.md` §The control protocol — the five-verb shape is frozen; afd_wire (M175) already carries the types.

## Sections (implementation slices)

### §1 — runnerBearer and the verb shell

The dedicated runner middleware: `Bearer agt_r` prefix required (no JWKS fall-through), timing-safe SHA-256 hash lookup in `fleet.runners` on every call (no memoized verdict — the lookup IS the revocation channel), `admin_state` mapping (miss → 401 UZ-RUN-001; non-active → 401 UZ-RUN-009; Postgres outage → 503 UZ-AUTH-004 so the runner backs off instead of counting an auth reject). Register (`POST /v1/runners`) gated by `runner:enroll` on the tenant plane; `me`, heartbeat (unconditionally `.ok`, revocation rides the reply), and the verb routes wired into the Route enum.

- **Dimension 1.1** — `agt_r` validation matrix: valid/unknown/revoked/cordoned → documented code each → Test `test_runner_bearer_state_matrix` — **DONE**
- **Dimension 1.2** — Postgres down during validation → 503 transport class, not 401 → Test `test_runner_auth_pg_outage_503` — **DONE**
- **Dimension 1.3** — register mints once, stores only the hash, rejects missing `runner:enroll` with 403 UZ-AUTH-022 → Test `test_register_enroll_gate` — **DONE**
- **Dimension 1.4** — revocation takes effect on the next request on every replica (no memo) → Test `test_revocation_immediate` — **DONE**

### §2 — Lease issue: assignment, money gates, policy

The lease verb performs hot-path writes 1–6 in the worker's order: atomic `runner_affinity` claim (UNIQUE per fleet) with monotonic `fencing_seq`, config resolved fresh per lease (no cache), the three gates in order (tenant balance → fleet budget → approval, the first two failing OPEN on a datastore fault so a metering outage cannot halt the platform), the two debit points (flat receive, charged on FIRST DELIVERY only because the balance debit is not replay-guarded; plus the floor-token run estimate), coverage refusal on empty wallet, `ExecutionPolicy` built with inline `secrets_map`, the per-lease resolved provider key (never persisted into `secrets_map`; zeroed after serialization), and the provider-neutral lease network rules ported from `src/agentsfleetd/git/repository_http_policy.zig`, bundle manifest attachment, lease TTL/`MAX_RUNTIME_MS` arithmetic.

**No version negotiation (Indy, Aug 23, 2026 — M175 addendum A1).** The Rust lease handler implements no `leaseWireVersion` equivalent, no `fromCurrent` downgrade, and no `requiresLeaseWireV2` refusal path. The version-two policy fields (`execution_policy.zig` `NetworkPolicy.read_only`, `http_origin_policies`, the richer `repository_binding`) apply unconditionally. **Implementation default:** Rust IGNORES the request's `wire_version` entirely and always serves the current shape — deliberately no explicit "unsupported version" rejection, because that needs a new error code, the ERROR REGISTRY gate fires on new codes, and the registry is single-sourced in Zig which this family does not touch. If the implementing agent judges loud rejection necessary, that is a judgment flag to Indy (📟🔦📈💥☠️), not a unilateral call. The divergence is registered in M181 §4.

**The issue-time run debit diverges from the Zig daemon (Indy, this stream).** The
Zig lease path debits ONCE — `fleet/service_billing.zig` closes its gate pass
with `// No issue-time stage debit: run fee + tokens meter on /renew + settle at
report`, and `fleet_runtime/metering.zig` exports `debitReceive` as its only
debit. The spec's two-debit shape and `data_flow.md` §C agree with each other
and not with that code. The decision is to implement the DOCUMENTED behaviour in
Rust and leave the Zig daemon alone, so the two daemons charge differently at
lease for the duration of the cutover. Consequences the implementing agent owns:
the run estimate is an ESTIMATE debited up front and reconciled by §3's
`billing.usage_ledger` settle — never a second charge for the same run — and
Dimension 2.1's row-parity oracle therefore compares against the shapes recorded
from the ported SQL, not against a live Zig daemon (which would disagree here by
design). Registered for M181 §4's divergence register.

- **Dimension 2.1** — the six lease writes land in the worker's order, with row shapes equal to the shapes recorded from the ported SQL for identical input → Test `test_lease_writes_row_parity` — **DONE** as `test_seeded_row_shapes`, which records the lease row's populated-column set from the ported `INSERT` and diffs it against `information_schema`; the write ORDER is held by the statement being one `WITH`, and `test_select_assigns_a_ready_fleets_event` proves it end to end
- **Dimension 2.2** — two runners race one fleet → exactly one lease; loser gets the no-work reply → Test `test_lease_affinity_race` — **DONE**
- **Dimension 2.3** — empty wallet → lease refused with the coverage code; no partial writes → Test `test_lease_money_gate_refusal` — **DONE**
- **Dimension 2.4** — provider key rides `ExecutionPolicy` only and is zeroed post-serialization → Test `test_provider_key_placement` — **DONE** as `a_mintable_credentials_handle_never_reaches_the_map`, `the_two_channels_stay_disjoint_across_a_mixed_set` and `a_declaration_never_renders_its_stored_values` (`afd_fleet::secrets`)

### §3 — Report: fence, flip, dedup

`claimReport()` semantics in one atomic statement: fencing-token verification, lease flip, telemetry dedup via the UNIQUE `(event_id, charge_type)` ledger rows — writes 7–12. Stale writers rejected with UZ-RUN-005. Renew extends to `min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)` and re-checks coverage (UZ-RUN-012 reachable).

- **Dimension 3.1** — report writes 7–12 row-parity on identical input → Test `test_report_writes_row_parity` — **DONE**
- **Dimension 3.2** — stale fencing token → UZ-RUN-005; no row mutated → Test `test_report_stale_fence_rejected` — **DONE**
- **Dimension 3.3** — duplicate report for one event → ledger stays at two rows per event; idempotent reply → Test `test_report_dedup_idempotent` — **DONE**
- **Dimension 3.4** — renew clamps to max runtime and refuses on empty wallet → Tests
  `test_renew_clamps_to_the_hard_ceiling`, `test_renew_after_reclaim_is_lost`,
  `test_renew_coverage_refuses_an_empty_wallet` (three tests rather than the one
  originally named: the clamp is store-level SQL and the coverage refusal needs the
  composed `Plane`, so fusing them would make a wallet fixture a precondition of
  every deadline assertion. The `Lost` verdict joined them because it is the arm
  the clamp must NOT be confused with — a cap says the result is still wanted, a
  loss says it will be refused) — **DONE**

### §4 — Activity, memory, bundles, mint

Activity frames validated and published to `fleet:{id}:activity`; memory GET hydrates the category-pinned byte window deterministically (every `core` entry newest-first, then newest non-core) and POST enforces the single-live-holder capture fencing; bundle GET streams by content hash from Cloudflare R2 (S3-compatible store); `credentials/mint` ports the broker — cached, singleton-per-key, minting short-lived integration tokens from vault handles via the config-driven registry (GitHub App + OAuth refresh).

- **Dimension 4.1** — activity frame → one `PUBLISH` on the fleet channel; malformed frame → typed 4xx → Test `test_activity_publish` — **DONE**, with both drop branches covered separately: `test_activity_drops_a_frame_it_cannot_render` (the bridge refuses the frame) and `test_activity_publish_redis_down` (the queue refuses the publish, over a `Redis::unreachable` handle so the lane's shared server is untouched)
- **Dimension 4.2** — memory hydration byte-window ordering matches the documented rule on a crafted corpus → Test `test_memory_hydration_window` — **DONE** as `test_core_entries_outrank_newer_non_core_ones`, `test_one_oversized_entry_still_hydrates`, `test_a_rejected_entry_ends_its_tier_rather_than_being_skipped`, `test_an_empty_set_hydrates_nothing` and `test_the_two_halves_partition_the_input` (`afd_fleet::memory::window`)
- **Dimension 4.3** — memory write with a stale capture fence rejected → Test `test_memory_capture_fencing` — **DONE**
- **Dimension 4.4** — bundle fetch streams the exact stored bytes; unknown hash → 404 → Test `test_bundle_fetch_by_hash` — **DONE**
- **Dimension 4.5** — mint: cache hit returns the live token; expiry re-mints once under concurrent callers → Test `test_mint_broker_single_flight` — **DONE** as `concurrent_cold_callers_cost_exactly_one_upstream_mint` and `a_second_ask_is_served_from_the_cache` (`afd_fleet::credential::broker`)

### §5 — Fleet config resolution (afd_fleet_runtime)

STORED fleet-config resolution — `config_json` → typed policy — with validation parity against `fleet_runtime/config_parser.zig`, feeding `ExecutionPolicy` (approval-gate policy included; rendering stays with M178). This is the half the runner plane calls: `fleet_session.zig:129` resolves it per lease and `credentials_mint_scope.zig:66` re-resolves it per mint, so §2's gates and §4.5's broker are both built on it.

**The install-time half is M178's, and the seam is the call site (Indy, this stream).** `parseTriggerMarkdownWithJson` — YAML frontmatter + markdown body → `config_json`, i.e. `config_markdown.zig` (338) + `yaml_frontmatter.zig` (272) — has FOUR non-test callers and not one of them is a route this milestone owns: `fleets/create.zig:123`, `fleets/patch_txn.zig:114` and `connectors/slack/channel_fleet.zig` are M178's tenant surface, and `fleet_library/importer.zig:165` is M179's bundle import. Porting it here would add a YAML crate and ~610 lines with zero callers in this PR, which the Dead Code Sweep and the crate-wide `unused_crate_dependencies` deny both refuse. `fleet_runtime/` is a LIBRARY straddling a boundary this family draws on route ownership — the section was authored whole because the Zig module family is whole, and the dimension is split to match where the callers actually are. The `serde_norway` implementation default travels with that half; no YAML crate is chosen here.

- **Dimension 5.1** — the committed STORED fleet-config corpus (seeded from `tests/fixtures/fleetbundle/` and the `config_json` shapes `fleet_session.zig` resolves) parses to the same accept/reject verdicts and field values as `parseStoredFleetConfig` → Test `test_fleet_config_corpus_parity` — **DONE for the half this milestone owns; the corpus half is DEFERRED to M178 (Indy, this stream).** The row has two inputs and they belong to different milestones, which the §5 note above already decided: `parseTriggerMarkdownWithJson` — `config_markdown.zig` (338) + `yaml_frontmatter.zig` (272) — has four non-test callers, none of them a route this PR owns, so porting it here would add a YAML crate and ~610 lines with zero callers, which `unused_crate_dependencies` and the Dead Code Sweep both refuse. `serde_norway` is fixed there as the implementation default travelling with that half. The `tests/fixtures/fleetbundle/` documents are YAML frontmatter and are read by `frontmatter_fixtures_test.zig`; reading them from Rust on this branch would mean asserting against a conversion invented for the test rather than the one production will run. **What is DONE here is the `config_json` half — the shapes `fleet_session.zig` actually resolves** — held by `afd_fleet_runtime::config`'s twenty-eight accept/reject-and-field-value cases and, end to end, by `test_runner_suite_vs_rust_daemon`, whose fleet carries a stored document the pull path parses per lease. The `parseStoredFleetConfig` DIFF is separately unfalsifiable: M175 §6 deleted the Zig lanes, the same trade §7 records for row parity. **M178 inherits `test_fleet_config_corpus_parity` over the frontmatter corpus, alongside the reader it ports.**

- **Dimension 5.2** — malformed stored config (wrong types, unknown runtime keys, a runtime key at the top level) → same error classes as Zig → Test `test_fleet_config_rejects_malformed` — **DONE** as the `afd_fleet_runtime::config` refusal cases, which cover the three classes this row names: a runtime key at the top level (`RuntimeKeyOutsideBlock`), an unknown key inside the block (`UnknownRuntimeKey`), and wrong-typed or half-declared fields (`a_gate_rule_without_a_tool_is_a_missing_field_not_a_shape_failure`, `an_access_level_without_a_list_is_refused`, `a_list_without_an_access_level_is_refused`)

### §6 — Sweepers and runner metrics

The four runner-plane sweepers as supervised tasks: liveness (derived three-category state; fresh-mint `last_seen_at = 0` reads `registered`, never a fake `online`), reclaim (expired leases re-leased with a higher fencing token; keyset cursor on `(updated_at, id)`, batch limit 100), retention, repair-verification dispatcher (including the repair-branch naming and trusted-context logic from `src/agentsfleetd/git/` — that module's only consumers live in this milestone). Runner metric families in the fixed 4096-slot table with bounded overflow routing and zero Postgres on the scrape-free OTLP path.

**The overflow SPELLING belongs to M181 §5, not here (Indy, this stream).** This section owns the runner families and the property that a capped table cannot grow without bound — `runner_id` is caller-supplied, so an unbounded label set is a memory fault a hostile or misconfigured fleet could reach. It does NOT own what the overflow series is called. M181 §5 configures the metrics pipeline on `opentelemetry_sdk` rather than porting the Zig aggregator, and the OpenTelemetry specification marks overflow with the attribute `otel.metric.overflow=true` where the Zig uses an `_other` label value. Pinning `_other` in a P0 dimension HERE would either force the SDK decision by the back door or leave this milestone's rubric contradicting the one that owns continuity. So Dimension 6.4 asserts the bound and the constant memory; M181's Dimension 5.5 asserts the spelling, against the dashboards that actually read it.

- **Dimension 6.1** — expired lease reclaimed with a strictly higher fencing token; original writer then fenced out → Test `test_reclaim_bumps_fence` — **DONE**
- **Dimension 6.2** — reclaim pages by keyset cursor honoring the batch limit → Test `test_reclaim_keyset_pagination` — **DONE** as `a_fresh_cursor_starts_below_every_real_row`, `a_cursor_resumes_after_the_row_it_recorded` and `a_rewound_cursor_is_a_fresh_one` (`afd_fleet::sweep::reclaim`), with the batch-limit pacing held by `a_full_batch_comes_straight_back`
- **Dimension 6.3** — never-connected runner reports `registered` → Test `test_liveness_fresh_mint_sentinel` — **DONE** as `a_runner_that_has_never_connected_is_not_stale` (`afd_fleet::sweep::liveness`)
- **Dimension 6.4** — past the family table's capacity, overflow routes to ONE series and metric memory stays constant; the overflow SPELLING is deliberately not asserted here (see the §6 note) → Test `test_metric_families_overflow` — **DONE** as `a_runner_gets_its_own_series_until_the_table_is_full`, `past_the_capacity_everything_lands_in_one_series` and `an_overflowed_runner_is_still_counted_and_still_carries_its_reason` (`afd_observability::runner`)

### §7 — The parity harness

The Rust daemon is proven against the committed wire fixtures (M175 §3), the ported SQL read side by side in REVIEW, and its own integration suite against real Postgres and Redis. There is no cross-implementation row differ: M175 §6 deleted `make test-integration` with the rest of the Zig lanes, so no live Zig daemon exists to diff against. What that costs is row-level cross-implementation equivalence; what it buys is not maintaining two daemons in order to compare them, which is the trade Indy took on Aug 23, 2026 on the fact that no production user reaches either.

The existing runner-side suites (lease hardening, lease transport, credential-mint end to end) run against `agentsfleetd-rs` directly — the demoable capability: a stock Zig runner completing work end to end against the Rust daemon.

- **Dimension 7.1** — a stock Zig runner completes a lease end to end against the Rust daemon, against real Postgres and Redis → Test `test_runner_suite_vs_rust_daemon` (the demoable capability; unchanged by the override, since it needs the runner, not the Zig daemon) — **DONE**
- **Dimension 7.2** — the seeded scenario set produces the expected row shapes in leases, events, ledger and memory, asserted against shapes recorded from the ported SQL rather than against a live second daemon → Test `test_seeded_row_shapes` — **DONE**

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 (serial) | §1 + §2 + §3 | Claude Code · Opus 5 · max | fencing, money, and secret placement are the family's highest-judgment code; one mind holds the atomic-statement set |
| B2 | §4 | Codex · GPT 5.6 tera · high | four separable verbs with crisp oracles |
| B2 | §5 | Claude Code · Opus 5 · high | parser parity against a committed corpus |
| B2 | §6 | Claude Code · Opus 5 · high | sweepers are well-documented ports with contention tests |
| B3 (serial) | §7 | Claude Code · Opus 5 · xhigh | harness wiring + failure triage across both daemons |

Indy decides how many agents actually spin per batch.

## Interfaces

```
Routes (afd_wire path constants; wire version 2 — frozen):
  POST /v1/runners                      (tenant plane, runner:enroll)
  GET  /v1/runners/me
  POST /v1/runners/me/heartbeats | /leases | /reports
  POST /v1/runners/me/credentials/mint
  POST /v1/runners/me/leases/{lease_id}/activity | /renew
  GET|POST /v1/runners/me/memory/{fleet_id}
  GET  /v1/runners/me/bundles/{content_hash}
Error codes: existing UZ-RUN-* / UZ-AUTH-* registry — referenced, never re-declared.
Row shapes: fleet.runner_leases, fleet.runner_events, billing.usage_ledger,
            fleet_sessions/fleet_events joins — parity targets, not designs.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale writer | reclaimed lease's original runner reports | atomic fence check → 4xx UZ-RUN-005, zero row mutation |
| Empty wallet at renew | tenant exhausted mid-run | UZ-RUN-012; lease expires by TTL; work redelivered per policy |
| Double lease attempt | two runners, one fleet | UNIQUE affinity + fencing_seq → one winner; loser polls on |
| Postgres outage | store down mid-verb | 503 transport class; runner backs off; no auth-reject counting |
| Redis publish failure | activity fan-out down | verb still succeeds where the Zig daemon does; drop accounted, never a 500 caused by telemetry |
| R2 outage on bundle fetch | object store down | 5xx with retryable class; lease remains redeliverable |
| Mint upstream failure | GitHub/OAuth down | typed upstream error; cache never stores a failure; no partial token |
| Reclaim double-claim race | two replicas sweep at once | atomic claim; loser observes zero rows; fence strictly increases |
| Config parse failure at lease | broken fleet config | lease refused with the parse error class; no partial writes |

## Invariants

1. At most one active lease per fleet — UNIQUE `runner_affinity` + time gate; `test_lease_affinity_race`.
2. `fencing_seq` strictly increases and every report verifies it inside the same atomic statement — `test_report_stale_fence_rejected`, `test_reclaim_bumps_fence`.
3. The provider `api_key` never enters `secrets_map` and is zeroed after lease serialization — `test_provider_key_placement` + zeroize newtypes.
4. Runner auth reads the row every time — no memoized verdict exists to invalidate; enforced by afd_auth exposing no cache seam; `test_revocation_immediate`.
5. Hot-path SQL is ported verbatim from the Zig domain `sql.zig` files (row-equivalence). The dual-run differ that would have enforced this mechanically is gone with the Zig lanes (Indy override, Aug 23, 2026), so enforcement is REVIEW's side-by-side read plus `test_seeded_row_shapes` — weaker, and named as weaker rather than left implied.
6. The heartbeat reply is unconditionally ok — rejection is auth's job; `test_runner_bearer_state_matrix` covers the split.
7. The lease handler carries exactly one lease shape — the current one — and never branches on `wire_version`; enforced by REVIEW reading the handler for a version branch and by §7's scenario set containing no version-one request.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| runner metric families (4 existing, fixed-slot) | ops | verb traffic / lease grants / releases | runner_id (capped table, bounded overflow — spelling is M181 §5's), verb, outcome | no tenant payloads | `test_metric_families_overflow` |
| credit drained at the receive debit | ops | a first delivery passes the gates | posture, provider, model — never workspace or tenant payloads | amount only; no event body | §2 returns the drained amount as a value; M181 §5 attaches the instrument |
| `fleet.runner_events` rows (existing append-only) | ops | state transitions | state, runner id | no secrets | `test_runner_bearer_state_matrix` |
| product analytics | not applicable | — | no product-event changes; parity port | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration (negative-heavy) | `test_runner_bearer_state_matrix` | valid→200 path; unknown→401 UZ-RUN-001; revoked/cordoned→401 UZ-RUN-009 |
| 1.2 | integration (negative) | `test_runner_auth_pg_outage_503` | stopped Postgres → 503 UZ-AUTH-004, not 401 |
| 1.3 | integration (negative) | `test_register_enroll_gate` | tenant JWT without runner:enroll and any agt_t key → 403 UZ-AUTH-022; with scope → 201, hash-only storage |
| 1.4 | integration | `test_revocation_immediate` | revoke between two requests → second request 401 on every replica |
| 2.1 | integration | `test_lease_writes_row_parity` | six writes land in order; each row's shape equals the recorded fixture shape |
| 2.2 | integration (race) | `test_lease_affinity_race` | concurrent leases, one fleet → one lease row, one no-work reply |
| 2.3 | integration (negative) | `test_lease_money_gate_refusal` | zero-balance tenant → refusal code, zero writes |
| 2.4 | unit | `test_provider_key_placement` | key present on policy, absent from secrets_map, buffer zeroed after serialize |
| 3.1 | integration | `test_report_writes_row_parity` | the six report writes land in one statement — lease flipped to `reported` with its cursor advanced, affinity cursor advanced, wallet drawn down by exactly the reported charge, `stage` ledger row carrying that amount and its span, lifetime tally on the succeeded column only |
| 3.2 | integration (negative) | `test_report_stale_fence_rejected` | fence n after reclaim to n+1 → UZ-RUN-005, no mutation |
| 3.3 | integration (replay) | `test_report_dedup_idempotent` | same report twice → two ledger rows total, same reply |
| 3.4 | integration (negative) | `test_renew_clamps_to_the_hard_ceiling` · `test_renew_after_reclaim_is_lost` · `test_renew_coverage_refuses_an_empty_wallet` | renew near max-runtime → clamped to `created_at + MAX_RUNTIME_MS`, past it → UZ-RUN-010, both rows advanced together; after a reclaim → UZ-RUN-011; empty wallet → UZ-RUN-012 with nothing advanced |
| 4.1 | integration | `test_activity_publish` | frame → one publish on `fleet:{id}:activity`; malformed → 4xx, no publish |
| 4.2 | unit | `test_memory_hydration_window` | crafted corpus → documented core-first newest-first ordering, byte cap honored |
| 4.3 | integration (negative) | `test_memory_capture_fencing` | stale capture fence → rejected, store unchanged |
| 4.4 | integration | `test_bundle_fetch_by_hash` | stored bytes round-trip; unknown hash → 404 |
| 4.5 | integration (race) | `test_mint_broker_single_flight` | concurrent expired-cache mints → one upstream call, one token |
| 5.1 | unit | `test_fleet_config_corpus_parity` | STORED corpus verdicts + field values equal `parseStoredFleetConfig` output |
| 5.2 | unit (negative) | `test_fleet_config_rejects_malformed` | malformed stored-config classes → matching error classes |
| 6.1 | integration (race) | `test_reclaim_bumps_fence` | expired lease → re-lease with fence+1; old writer fenced |
| 6.2 | integration | `test_reclaim_keyset_pagination` | >batch-limit expired leases → paged sweep, none skipped/duplicated |
| 6.3 | unit | `test_liveness_fresh_mint_sentinel` | `last_seen_at = 0` → `registered` |
| 6.4 | unit | `test_metric_families_overflow` | slot 4097 routes to a single overflow series; table size constant past the cap (spelling asserted by M181 5.5) |
| 7.1 | e2e | `test_runner_suite_vs_rust_daemon` | existing runner integration subset green against `agentsfleetd-rs` |
| 4.1 (FM) | integration (negative) | `test_activity_publish_redis_down` | Redis publish failure → verb outcome parity with Zig; drop accounted, never a telemetry-caused 500 |
| 4.4 (FM) | integration (negative) | `test_bundle_fetch_r2_outage` | object store down → 5xx retryable class; lease stays redeliverable |
| 4.5 (FM) | integration (negative) | `test_mint_upstream_failure` | upstream mint failure → typed error; cache never stores a failure |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Stock Zig runner completes a lease cycle on the Rust daemon (§7) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Row-shape parity on lease + report writes (§2, §3, §7 — against shapes recorded from the ported SQL; there is no live Zig daemon to diff against, per §7 and Invariant 5) | `cd rustd && cargo test row_parity` | exit 0 | P0 | |
| R3 | Fencing and races hold (§2–§4, §6) | `cd rustd && cargo test fence` + `cargo test race` | exit 0 | P0 | |
| R4 | Config corpus parity (§5) | `cd rustd && cargo test test_fleet_config_corpus_parity` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

**Credential gate (in scope):** this milestone adds the Cloudflare R2 read keys and the GitHub App private key (mint) to the family enumeration — same fetch location (`~/.config/agentsfleet/` via `provision-env-1password`); the boot preflight extends to name them.

**Identity-provider knobs — enumerated here because no milestone had them.** M176's credential gate named `OIDC_ISSUER`/`OIDC_AUDIENCE` as required and `OIDC_JWKS_URL` as optional-derived, but shipped without wiring any of them into the Rust preflight; M178 explicitly defers to "the M176/M177 enumerations"; and `CLERK_API_BASE`/`CLERK_SECRET_KEY` appeared in no milestone at all, only in the Zig `cmd/serve.zig` hint. This milestone closes that gap in the preflight rather than carrying it into M181's divergence register (Indy, this stream).

| Knob | Boot | Why |
|------|------|-----|
| `OIDC_ISSUER` | required | the issuer the key-set endpoint is derived from; `runtime_validate.zig` exits without it |
| `OIDC_AUDIENCE` | required | checked strictly, so a token minted for a sibling service is refused |
| `OIDC_JWKS_URL` | optional | derived from the issuer unless overridden, so the two can never name different providers |
| `CLERK_API_BASE` | required | the base a subject's capability claim is read from |
| `CLERK_SECRET_KEY` | required | authorises that read; `clerk_scope_resolver.zig` treats an absent secret as a fetch failure, so an unset one is a permanent tenant-plane outage rather than a degraded write-back |

Same fetch location as the rest of the family. The daemon no longer boots with the tenant plane unconfigured: `Identity::Absent` is gone, and a missing knob is named in the same one-shot refusal as a missing datastore URL.

## Out of Scope

- Tenant/workspace routes (M178), admin/operator surface incl. bundle *import* (M179 — this milestone only *serves* stored bundles), signed ingress + cron (M180).
- Approval-gate Slack rendering and the tenant approvals surface (M178) — only the policy fields on `ExecutionPolicy` land here.
- Any change to the runner binary or the wire version — the runner stays stock Zig; wire stays version two. Reproducing the Zig daemon's version-one downgrade path is explicitly NOT owed (M175 addendum A1; registered as a declared divergence in M181 §4).


---

## Product Clarity (authoring record)

1. **Successful user moment** — on staging, an operator watches a stock runner heartbeat into `agentsfleetd-rs`, take a lease, stream activity, and report — and the billing ledger and lease rows are indistinguishable from the Zig daemon's.
2. **Preserved user behaviour** — runner operators change nothing: same token, same verbs, same codes, same backoff behaviour; tenants see identical billing rows.
3. **Optimal-way check** — runner plane before tenant surface is the direct path: it is the best-specified core (frozen wire, documented invariants) and the strongest de-risking signal for everything after.
4. **Rebuild-vs-iterate** — port with verbatim SQL, not a redesign; the one deliberate rebuild (sweepers as supervised async tasks) preserves observable behaviour.
5. **What we build** — three crates + verb handlers + sweepers + the dual-daemon parity harness.
6. **What we do NOT build** — scheduler/trust-class placement changes, wire v3, runner-side changes, new metrics families.
7. **Fit with existing features** — compounds with M176 substrate; the Zig daemon's own integration lanes went with M175 §6, so there is no second lane for this milestone to destabilize.
8. **Surface order** — N/A — machine-facing surface only (runner protocol); no human UI/CLI change.
9. **Dashboard restraint** — N/A — no UI change.
10. **Confused-user next step** — a failing runner interaction surfaces the documented UZ-RUN-* code; `docs/architecture/runner_fleet.md` maps each code to cause — no new doc surface needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven slices ordered by risk — auth shell, then the two atomic-write verbs held by one agent, then separable verbs, config, sweepers, and the harness that grades it all.
- **Alternatives considered:** porting handlers before `afd_fleet` service logic (rejected: handlers without the atomic statements underneath prove nothing); redesigning the lease store while porting (rejected: parity-first family rule; redesigns come post-cutover).
- **Patch-vs-refactor verdict:** this is a **refactor** (same behaviour, new runtime) with verbatim-SQL discipline keeping it honest; no mud-patching — the Zig daemon stays untouched.

## Discovery (consult log)

- **Spec amendment at CHORE(open) — cross-daemon residue removed.** Rows R1/R2
  and test rows 7.2/7.3 still described a `DAEMON=rust|zig` selector and a
  dual-run row differ, contradicting this spec's own §7 and Invariant 5, both of
  which record that M175 §6 deleted the Zig integration lanes so no live Zig
  daemon exists to diff against. `AGENTS.orly.md` §Specification Standards —
  "Spec contradicts a rule → amend spec" — so the residue was amended to match
  §7 rather than a differ being built against a daemon that no longer runs:
  R1's command became `make test-integration-rustd`; R2 became row-SHAPE parity
  against shapes recorded from the ported SQL; test rows 7.2
  (`test_runner_suite_vs_zig_daemon`) and 7.3 (`test_dual_run_row_differ`) were
  deleted as unrunnable; Dimensions 2.1/3.1 and Files-Changed rows for
  `make/test-integration.mk`, `src/build/**` and `tests/**` were reworded to the
  single-daemon shape. No capability was dropped: §7.1's demoable
  stock-runner-against-`agentsfleetd-rs` end-to-end survives unchanged, because
  it needs the runner, not the second daemon.
- **§5 split along its call sites at EXECUTE — Dimension 5.1 narrowed, the
  frontmatter half handed to M178 §3 (Indy, this stream).** The section was
  authored as one because `fleet_runtime/` is one Zig module family; the
  milestone boundary in this family is drawn on ROUTE ownership, and that
  library straddles it. Every non-test caller was traced rather than assumed:
  the stored parser (`parseStoredFleetConfig`) is called by
  `fleet/fleet_session.zig:129` (the lease verb) and
  `http/handlers/runner/credentials_mint_scope.zig:66` (the mint verb) — both
  M177's — plus `fleets/cron_sync.zig:66`, which is M180's; the markdown parser
  (`parseTriggerMarkdownWithJson`) is called by `fleets/create.zig:123`,
  `fleets/patch_txn.zig:114`, `connectors/slack/channel_fleet.zig` (×4) and
  `fleet_library/importer.zig:165` — M178's and M179's, and NONE of them this
  milestone's. So the claim-time half could not move out (§2's gates and §4.5's
  broker are built on it, and §7.1's demoable capability is unreachable without
  it) and the install-time half could not usefully move in (~610 lines and a
  YAML crate with zero callers in this PR, against the Dead Code Sweep and the
  crate-wide `unused_crate_dependencies` deny). Dimension 5.1's corpus is
  therefore the STORED corpus, a new M178 Dimension 3.4 carries the frontmatter
  corpus, and the `serde_norway` implementation default travels with it. What
  this buys is R4 becoming gradeable in the milestone that owns the code it
  grades; what it costs is that "§5 is done" now means one half, which is why
  the section body says so in its own words rather than leaving a reader to
  infer it from the dimension.
- **The observability surface was inventoried at EXECUTE, and most of it is not
  this milestone's (Indy, this stream).** The Zig carries ~6,000 production
  lines under `observability/`; `afd_observability` carries 202. Rather than
  discover the difference at cutover, every file was placed. M176 §6 shipped the
  SPAN pipeline. This milestone owns the runner FAMILIES (§6). M178 §7 owns
  PostHog (`telemetry.zig`, `telemetry_events.zig` — product analytics, not
  OTel). M181 §5 owned "the transport". Two clusters needed no port at all and
  were nearly filed as gaps: `otel_logs.zig` is already decided in M181 §5
  (logfmt on stderr, ingested by a Collector's `filelog` receiver), and
  `library_read_counters.zig` + siblings (~680 lines) are TEST-ONLY — they
  increment under `builtin.is_test` and the module says in its own header that
  it is deliberately not telemetry.
  What was genuinely unowned is the metrics PIPELINE — instruments, family
  registry, label dimensions, aggregation, cardinality, runtime families,
  ~1,450 lines — which M181 §5 was written as if M176 had already shipped.
  M176 shipped spans; there is no metric instrument anywhere in the Rust tree.
  A transport with nothing to carry cannot make §4's `test_metric_continuity`
  gradeable, so the pipeline is now named in M181 §5 alongside it, with
  `opentelemetry_sdk`'s `metrics` feature (a flag on a crate already in the
  lock) replacing the port. The consequence that lands HERE: the SDK marks
  cardinality overflow with `otel.metric.overflow=true` where the Zig uses an
  `_other` label, so Dimension 6.4 was re-scoped to assert the BOUND and the
  constant memory — the property this milestone actually owns — and M181's new
  Dimension 5.5 asserts the spelling against the dashboards that read it.
  Pinning `_other` in a P0 row here would have forced M181's SDK decision by
  the back door.
- **The credit-drain metric is returned as a value, not emitted inline (§2).**
  `service_billing.zig` fuses the decision and the emission: the receive debit's
  success arm calls `otel_metrics.recordCreditConsumed` at the call site. The
  Rust debit answers `Deducted(Nanos)` instead — which is the Zig's own
  `DebitOutcome.deducted` shape — and §6/M181 attaches the instrument to that
  value. What this buys is that §2 needs no metrics pipeline to be correct and
  no test here asserts an exporter. What it COSTS is a seam: if the instrument
  is never attached, the metric silently never fires and no test in either
  milestone catches it, where the Zig gets the coupling for free by accident.
  Recorded as an explicit obligation in the Metrics table above rather than
  left implied.
- **SQL organisation — prior art surveyed before deciding.** `~/Projects/oss/exonum`
  carries zero SQL (RocksDB via `components/merkledb`) and `~/Projects/oss/habitat`
  carries zero SQL and zero diesel (`builder-api` is split out upstream), so
  neither offers a precedent. `~/Projects/oss/core_api-develop` keeps SQL inline in
  `models/<entity>.rs` with no `sql.rs`/`queries.rs` anywhere — but its statements
  are one-line stored-procedure calls (`SELECT * FROM insert_account_session_v2($1..$9)`)
  whose logic lives in Postgres functions under `components/database/migrations/`.
  That shape is unavailable here: SCHEMA GUARD is "no" and Invariant 5 mandates
  the writable-CTEs port verbatim, so `claimAndSettle`'s ~90-line statement stays
  a Rust constant. `afd_fleet` therefore takes a domain-split `sql/` module tree —
  chosen because Invariant 5's only enforcement is REVIEW's side-by-side read
  against `fleet/sql.zig`, which needs the statements collected — plus core_api's
  two transferable habits: one file per domain, and row→struct decoding in its own
  `rows.rs` (already this repository's shape in `afd_state/src/credentials/rows.rs`).
  Typed parameter structs are written only for the three high-arity statements
  (`INSERT_LEASE_WITH_EVENT` 23 binds, `CLAIM_SETTLE_SQL` 17, `INSERT_RUNNER_WITH_EVENT`
  17); statements at four binds or fewer bind at the call site, per
  `M-SIMPLE-ABSTRACTIONS`.
- **Files Changed amended at EXECUTE — `afd_db` gains one test-only constructor.**
  §1's Dimension 1.2 asks for "Postgres down during validation → 503 transport
  class, not 401". Proving that against the production router needs a `Db` whose
  every acquire fails, and `Db::connect` cannot produce one: it PROBES before it
  builds a pool, deliberately, because that probe is the promise that a boot
  which returned has a reachable datastore. Constructing a lazy pool anywhere
  else would be a second way to reach Postgres, which is the exact drift
  `probes.rs` warns against, so the constructor belongs in `afd_db` beside the
  eager one and is gated on `test-util`. `AGENTS.orly.md` §Specification
  Standards — amend the spec rather than smuggle the path — so the Files Changed
  table above carries the row. What it buys is the whole §1 refusal matrix
  (1.1–1.4) running datastore-free in two seconds against the real router, real
  layer order, and real handlers.
- **Substrate coupling localised at EXECUTE — `reconcile` asks for guarantees.**
  `heartbeat_reconcile.zig` asks a host whether it has Landlock, seccomp, the
  `cpu`/`memory`/`pids` controllers and a `bubblewrap` binary — five Linux
  mechanisms named by the CONTROL PLANE, which is what makes the daemon
  bubblewrap-shaped and what would refuse a Firecracker, virtual-machine or
  managed-platform runner delivering identical isolation. The verdict logic is
  now expressed in `Guarantee` (filesystem isolation, syscall filtering,
  resource limits, process containment, egress control) with
  `Guarantee::proven_by` as the ONE substrate-aware function in the crate. Zero
  wire change, zero behaviour change — same refusal order, same operator
  sentences, pinned by `tests/verdict_matrix.rs`. The wire-level change
  (`isolation_class` + a reported guarantee set) is NOT taken here: it would
  break §7.1's unmodified-runner claim and Invariant 5's row parity, and it
  wants its own spec after M181 retires the second daemon. Recorded as a
  question to Indy rather than a decision.
- **Crypto backend consolidated to `aws-lc-rs`; `ring` removed (Indy, in-session).**
  Indy directed that §4.4 adopt `object_store` in THIS milestone rather than a
  follow-up, and separately that the workspace track the latest crates. Those two
  decide the backend: `object_store` 0.14 enables `aws-lc-rs` as a DEFAULT
  feature with no ring alternative, and `reqwest` 0.13 collapsed its TLS features
  so plain `rustls` forces the same. Keeping `ring` alongside would put two
  `CryptoProvider`s in one binary — the state M176's `reqwest` 0.12 pin existed
  to prevent — so the pin is released and the workspace resolves to one provider.
  `cargo tree -i ring` now reports nothing; `cargo tree -i aws-lc-rs` shows
  `afd_identity`, `reqwest`, `redis` and `sqlx-core` all on it.

  Three objections were raised against doing this inside M177 and TWO WERE WRONG
  on facts that had not been checked, which is recorded here rather than quietly
  dropped: (1) "`aws-lc-sys` needs cmake" — it does not at 1.18/0.44; it built
  here on Apple clang with no cmake installed; (2) "it breaks the linux
  cross-compile" — CI ships no Rust binary today (`release.yml` builds only the
  Zig daemon for musl), so that risk is a cutover (M181) concern and is better
  discovered now than then. The third (R5 / Files Changed) is real and is
  resolved by the `afd_identity` row above.

  Two behaviour changes are ACCEPTED and named rather than absorbed: the
  workspace's no-C-linkage posture ends (`aws-lc-sys` is a BoringSSL fork that
  compiles C), and `reqwest` 0.13 removed `webpki-roots` so trust anchors now
  come from the platform verifier rather than the binary — `Dockerfile` installs
  `ca-certificates`, which is what makes that safe rather than merely survivable.
  The musl cross-compile of `aws-lc-sys` remains UNPROVEN and is M181's to prove.

  "ring-compatible" proved to mean "compatible when you ask": `RsaKeyPair::public()`
  is `public_key()` on the `KeyPair` trait, and the components conversion plus
  `modulus_len` live behind the `ring-io` / `ring-sig-verify` features, which are
  aws-lc-rs defaults and must be named explicitly under `default-features = false`.
- **§4.5 mint backend.** `jsonwebtoken` 11 is kept — it is JWT semantics (header
  and claims JSON, base64url segments, expiry validation), not a crypto backend,
  and hand-rolling that on top of a primitive library is the plumbing that earns
  a CVE for saving a dependency. Its backend feature is `aws_lc_rs`, pointing it
  at the same provider as everything else. The `rust_crypto` alternative was the
  right answer only while the workspace was staying on `ring`, and it is not.
- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
