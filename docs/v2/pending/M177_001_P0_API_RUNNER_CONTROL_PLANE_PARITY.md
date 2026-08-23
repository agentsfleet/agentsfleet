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
**Status:** PENDING
**Priority:** P0 — the demoable core of the port; cutover is impossible without it
**Categories:** API
**Batch:** B3 — serial after M176; M178/M179 fan out after it
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
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
| `rustd/crates/afd_auth/**` | EDIT | `runnerBearer` validator (`agt_r` prefix, timing-safe hash lookup, no memoization) |
| `rustd/crates/agentsfleetd/**` | EDIT | sweeper tasks join the supervisor; runner metric families registered |
| `rustd/Cargo.toml` | EDIT | new members |
| `make/test-integration.mk` | EDIT | `DAEMON=rust\|zig` daemon-under-test selector (default `zig`) + dual-run differ lane |
| `src/build/**` | EDIT | integration test-build wiring for the selector and the row-dump/differ steps |
| `tests/**` | EDIT | seeded deterministic scenario set + normalization mapping for the dual-run differ |

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

- **Dimension 1.1** — `agt_r` validation matrix: valid/unknown/revoked/cordoned → documented code each → Test `test_runner_bearer_state_matrix`
- **Dimension 1.2** — Postgres down during validation → 503 transport class, not 401 → Test `test_runner_auth_pg_outage_503`
- **Dimension 1.3** — register mints once, stores only the hash, rejects missing `runner:enroll` with 403 UZ-AUTH-022 → Test `test_register_enroll_gate`
- **Dimension 1.4** — revocation takes effect on the next request on every replica (no memo) → Test `test_revocation_immediate`

### §2 — Lease issue: assignment, money gates, policy

The lease verb performs hot-path writes 1–6 in the worker's order: atomic `runner_affinity` claim (UNIQUE per fleet) with monotonic `fencing_seq`, config resolved fresh per lease (no cache), the two debit points (flat receive + floor-token run estimate), coverage refusal on empty wallet, `ExecutionPolicy` built with inline `secrets_map`, the per-lease resolved provider key (never persisted into `secrets_map`; zeroed after serialization), and the provider-neutral lease network rules ported from `src/agentsfleetd/git/repository_http_policy.zig`, bundle manifest attachment, lease TTL/`MAX_RUNTIME_MS` arithmetic.

**No version negotiation (Indy, Aug 23, 2026 — M175 addendum A1).** The Rust lease handler implements no `leaseWireVersion` equivalent, no `fromCurrent` downgrade, and no `requiresLeaseWireV2` refusal path. The version-two policy fields (`execution_policy.zig` `NetworkPolicy.read_only`, `http_origin_policies`, the richer `repository_binding`) apply unconditionally. **Implementation default:** Rust IGNORES the request's `wire_version` entirely and always serves the current shape — deliberately no explicit "unsupported version" rejection, because that needs a new error code, the ERROR REGISTRY gate fires on new codes, and the registry is single-sourced in Zig which this family does not touch. If the implementing agent judges loud rejection necessary, that is a judgment flag to Indy (📟🔦📈💥☠️), not a unilateral call. The divergence is registered in M181 §4.

- **Dimension 2.1** — the six lease writes land in order with row shapes equal to the Zig daemon on identical input → Test `test_lease_writes_row_parity`
- **Dimension 2.2** — two runners race one fleet → exactly one lease; loser gets the no-work reply → Test `test_lease_affinity_race`
- **Dimension 2.3** — empty wallet → lease refused with the coverage code; no partial writes → Test `test_lease_money_gate_refusal`
- **Dimension 2.4** — provider key rides `ExecutionPolicy` only and is zeroed post-serialization → Test `test_provider_key_placement`

### §3 — Report: fence, flip, dedup

`claimReport()` semantics in one atomic statement: fencing-token verification, lease flip, telemetry dedup via the UNIQUE `(event_id, charge_type)` ledger rows — writes 7–12. Stale writers rejected with UZ-RUN-005. Renew extends to `min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)` and re-checks coverage (UZ-RUN-012 reachable).

- **Dimension 3.1** — report writes 7–12 row-parity on identical input → Test `test_report_writes_row_parity`
- **Dimension 3.2** — stale fencing token → UZ-RUN-005; no row mutated → Test `test_report_stale_fence_rejected`
- **Dimension 3.3** — duplicate report for one event → ledger stays at two rows per event; idempotent reply → Test `test_report_dedup_idempotent`
- **Dimension 3.4** — renew clamps to max runtime and refuses on empty wallet → Test `test_renew_clamp_and_coverage`

### §4 — Activity, memory, bundles, mint

Activity frames validated and published to `fleet:{id}:activity`; memory GET hydrates the category-pinned byte window deterministically (every `core` entry newest-first, then newest non-core) and POST enforces the single-live-holder capture fencing; bundle GET streams by content hash from Cloudflare R2 (S3-compatible store); `credentials/mint` ports the broker — cached, singleton-per-key, minting short-lived integration tokens from vault handles via the config-driven registry (GitHub App + OAuth refresh).

- **Dimension 4.1** — activity frame → one `PUBLISH` on the fleet channel; malformed frame → typed 4xx → Test `test_activity_publish`
- **Dimension 4.2** — memory hydration byte-window ordering matches the documented rule on a crafted corpus → Test `test_memory_hydration_window`
- **Dimension 4.3** — memory write with a stale capture fence rejected → Test `test_memory_capture_fencing`
- **Dimension 4.4** — bundle fetch streams the exact stored bytes; unknown hash → 404 → Test `test_bundle_fetch_by_hash`
- **Dimension 4.5** — mint: cache hit returns the live token; expiry re-mints once under concurrent callers → Test `test_mint_broker_single_flight`

### §5 — Fleet config resolution (afd_fleet_runtime)

Fleet config parsing — YAML frontmatter + markdown body — with validation parity against `fleet_runtime/config_parser.zig`, feeding `ExecutionPolicy` (approval-gate policy included; rendering stays with M178). **Implementation default:** a maintained serde-compatible YAML crate — `serde_norway` as of authoring (serde_yaml is archived); the agent re-verifies crate health at EXECUTE and records the pick in Discovery — because the fork-pinned `zig-yaml` rationale (build breakage upstream) dissolves only if the replacement is actually maintained.

- **Dimension 5.1** — the committed fleet-config corpus (seeded from the `src/agentsfleetd/fleet_runtime/` frontmatter fixtures and `tests/fixtures/fleetbundle/`) parses to the same accept/reject verdicts and field values as the Zig parser → Test `test_fleet_config_corpus_parity`
- **Dimension 5.2** — malformed frontmatter (unclosed, wrong types, unknown keys) → same error classes as Zig → Test `test_fleet_config_rejects_malformed`

### §6 — Sweepers and runner metrics

The four runner-plane sweepers as supervised tasks: liveness (derived three-category state; fresh-mint `last_seen_at = 0` reads `registered`, never a fake `online`), reclaim (expired leases re-leased with a higher fencing token; keyset cursor on `(updated_at, id)`, batch limit 100), retention, repair-verification dispatcher (including the repair-branch naming and trusted-context logic from `src/agentsfleetd/git/` — that module's only consumers live in this milestone). Runner metric families in the fixed 4096-slot table with `_other` overflow routing and zero Postgres on the scrape-free OTLP path.

- **Dimension 6.1** — expired lease reclaimed with a strictly higher fencing token; original writer then fenced out → Test `test_reclaim_bumps_fence`
- **Dimension 6.2** — reclaim pages by keyset cursor honoring the batch limit → Test `test_reclaim_keyset_pagination`
- **Dimension 6.3** — never-connected runner reports `registered` → Test `test_liveness_fresh_mint_sentinel`
- **Dimension 6.4** — metric table overflow routes to `_other`; memory stays constant → Test `test_metric_families_overflow`

### §7 — The parity harness

**Oracle changed by Indy's Aug 23, 2026 override.** M175 §6 deleted `make test-integration` along with the rest of the Zig lanes, so there is no live Zig daemon to diff against. The dual-run differ described below is therefore NOT built: the Rust daemon is proven against the committed wire fixtures (M175 §3), the ported SQL read side by side in REVIEW, and its own integration suite against real Postgres and Redis. What is lost is row-level cross-implementation equivalence; what is gained is not maintaining two daemons to compare them, which is the trade Indy took on the fact that no production user reaches either. Dimensions 7.1 and 7.3 are rewritten accordingly and 7.2 is dropped.

The superseded design, kept for the record: this milestone builds its own oracle — `make test-integration` today builds and drives the Zig daemon only, so the harness gains a **daemon-under-test selector** (`make test-integration DAEMON=rust|zig`, defaulting to `zig` so existing lanes are untouched) and a **dual-run row differ**: the same seeded, deterministic inputs are driven through BOTH live daemons, the touched rows (leases, events, ledger, memory) are dumped, volatile columns (generated ids, timestamps, fencing tokens) are normalized by consistent mapping — never dropped — and the normalized sets are diffed to an artifact that must be empty. Every lease request in the scenario set carries `wire_version: 2` — that is what the real runner sends (`src/runner/daemon/control_plane_client.zig:96`), so the differ compares the path that actually runs; no version-one scenario is seeded, and a version-one row in the scenario set is a defect in the harness, not coverage. The `row_parity` tests compare against the live Zig daemon, not recorded fixtures. The existing runner-side suites (lease hardening, lease transport, credential-mint end-to-end) then run against `agentsfleetd-rs` through the selector — the demoable capability: a stock Zig runner completing work end-to-end against the Rust daemon.

- **Dimension 7.1** — a stock Zig runner completes a lease end to end against the Rust daemon, against real Postgres and Redis → Test `test_runner_suite_vs_rust_daemon` (the demoable capability; unchanged by the override, since it needs the runner, not the Zig daemon)
- **Dimension 7.2** — DROPPED by the override: there is no Zig daemon lane left to run the subset against.
- **Dimension 7.3** — the seeded scenario set produces the expected row shapes in leases, events, ledger and memory, asserted against shapes recorded from the ported SQL rather than against a live second daemon → Test `test_seeded_row_shapes`

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
| runner metric families (4 existing, fixed-slot) | ops | verb traffic / lease grants / releases | runner_id (capped table, `_other` overflow), verb, outcome | no tenant payloads | `test_metric_families_overflow` |
| `fleet.runner_events` rows (existing append-only) | ops | state transitions | state, runner id | no secrets | `test_runner_bearer_state_matrix` |
| product analytics | not applicable | — | no product-event changes; parity port | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration (negative-heavy) | `test_runner_bearer_state_matrix` | valid→200 path; unknown→401 UZ-RUN-001; revoked/cordoned→401 UZ-RUN-009 |
| 1.2 | integration (negative) | `test_runner_auth_pg_outage_503` | stopped Postgres → 503 UZ-AUTH-004, not 401 |
| 1.3 | integration (negative) | `test_register_enroll_gate` | tenant JWT without runner:enroll and any agt_t key → 403 UZ-AUTH-022; with scope → 201, hash-only storage |
| 1.4 | integration | `test_revocation_immediate` | revoke between two requests → second request 401 on every replica |
| 2.1 | integration | `test_lease_writes_row_parity` | six writes: identical rows vs Zig daemon on the same fixture input |
| 2.2 | integration (race) | `test_lease_affinity_race` | concurrent leases, one fleet → one lease row, one no-work reply |
| 2.3 | integration (negative) | `test_lease_money_gate_refusal` | zero-balance tenant → refusal code, zero writes |
| 2.4 | unit | `test_provider_key_placement` | key present on policy, absent from secrets_map, buffer zeroed after serialize |
| 3.1 | integration | `test_report_writes_row_parity` | writes 7–12 identical vs Zig daemon |
| 3.2 | integration (negative) | `test_report_stale_fence_rejected` | fence n after reclaim to n+1 → UZ-RUN-005, no mutation |
| 3.3 | integration (replay) | `test_report_dedup_idempotent` | same report twice → two ledger rows total, same reply |
| 3.4 | integration (negative) | `test_renew_clamp_and_coverage` | renew near max-runtime → clamped; empty wallet → UZ-RUN-012 |
| 4.1 | integration | `test_activity_publish` | frame → one publish on `fleet:{id}:activity`; malformed → 4xx, no publish |
| 4.2 | unit | `test_memory_hydration_window` | crafted corpus → documented core-first newest-first ordering, byte cap honored |
| 4.3 | integration (negative) | `test_memory_capture_fencing` | stale capture fence → rejected, store unchanged |
| 4.4 | integration | `test_bundle_fetch_by_hash` | stored bytes round-trip; unknown hash → 404 |
| 4.5 | integration (race) | `test_mint_broker_single_flight` | concurrent expired-cache mints → one upstream call, one token |
| 5.1 | unit | `test_fleet_config_corpus_parity` | corpus verdicts + field values equal Zig parser output |
| 5.2 | unit (negative) | `test_fleet_config_rejects_malformed` | malformed frontmatter classes → matching error classes |
| 6.1 | integration (race) | `test_reclaim_bumps_fence` | expired lease → re-lease with fence+1; old writer fenced |
| 6.2 | integration | `test_reclaim_keyset_pagination` | >batch-limit expired leases → paged sweep, none skipped/duplicated |
| 6.3 | unit | `test_liveness_fresh_mint_sentinel` | `last_seen_at = 0` → `registered` |
| 6.4 | unit | `test_metric_families_overflow` | slot 4097 routes to `_other`; table size constant |
| 7.1 | e2e | `test_runner_suite_vs_rust_daemon` | existing runner integration subset green with `DAEMON=rust` |
| 7.2 | e2e (regression) | `test_runner_suite_vs_zig_daemon` | same subset green with `DAEMON=zig` (harness sanity) |
| 7.3 | e2e | `test_dual_run_row_differ` | seeded scenarios → empty normalized row diff; seeded delta → named non-empty artifact |
| 4.1 (FM) | integration (negative) | `test_activity_publish_redis_down` | Redis publish failure → verb outcome parity with Zig; drop accounted, never a telemetry-caused 500 |
| 4.4 (FM) | integration (negative) | `test_bundle_fetch_r2_outage` | object store down → 5xx retryable class; lease stays redeliverable |
| 4.5 (FM) | integration (negative) | `test_mint_upstream_failure` | upstream mint failure → typed error; cache never stores a failure |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Stock Zig runner completes a lease cycle on the Rust daemon (§7) | `make test-integration DAEMON=rust` | exit 0 | P0 | |
| R2 | Dual-run row parity on lease + report writes (§2, §3, §7 — vs the live Zig daemon) | `cd rustd && cargo test row_parity` | exit 0 | P0 | |
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
7. **Fit with existing features** — compounds with M176 substrate; must not destabilize the live Zig daemon's integration lanes (they keep running unchanged in CI).
8. **Surface order** — N/A — machine-facing surface only (runner protocol); no human UI/CLI change.
9. **Dashboard restraint** — N/A — no UI change.
10. **Confused-user next step** — a failing runner interaction surfaces the documented UZ-RUN-* code; `docs/architecture/runner_fleet.md` maps each code to cause — no new doc surface needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven slices ordered by risk — auth shell, then the two atomic-write verbs held by one agent, then separable verbs, config, sweepers, and the harness that grades it all.
- **Alternatives considered:** porting handlers before `afd_fleet` service logic (rejected: handlers without the atomic statements underneath prove nothing); redesigning the lease store while porting (rejected: parity-first family rule; redesigns come post-cutover).
- **Patch-vs-refactor verdict:** this is a **refactor** (same behaviour, new runtime) with verbatim-SQL discipline keeping it honest; no mud-patching — the Zig daemon stays untouched.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
