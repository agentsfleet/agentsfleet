<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M229_002: One tenant's busy draft never stalls another tenant's chat, proven under pool saturation

**Prototype:** v2.0.0
**Milestone:** M229
**Workstream:** 002
**Date:** Oct 17, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — throughput and load behaviour under contention; nothing is broken at today's load
**Categories:** API
**Batch:** B2 — follows M229_001 on the rebuilt schema
**Branch:** feat/m229-data-access-under-load
**Baseline revision:** 0fa9e55356571d57df69cd3a13fb3b90930b0e49
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M229_001 — the rebuilt schema, typed row reads and connection time limits this workstream measures against
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 17, 2026), from the adversarial review of M229_001 and source reads at `7704a01ff`; §2 re-aimed at CHORE(open) (Oct 21, 2026) against `0fa9e5535`
**Canonical architecture:** `docs/architecture/concurrency.md` (tasks, locks and shutdown order)

---

## Overview

**Goal (testable):** `test_locked_draft_does_not_block_other_tenant` — with tenant A's draft row held by another transaction, a runner polling for chat work receives tenant B's queued message without waiting on A, and the saturation benchmark shows no duplicate lease, no capacity oversubscription and full pool recovery.
**Problem:** a chat claim walks up to 64 of the oldest drafts (`SCAN_LIMIT`, `rustd/crates/afd_authoring/src/lease/claim/sql/mod.rs:4`), and for each one locks the runner's host row, then the creator's user row (`FOR SHARE`, `rustd/crates/afd_authoring/src/sql/access/person.sql:7`), then the draft (`FOR UPDATE`, `rustd/crates/afd_authoring/src/sql/draft/lock.sql:9`). Every draft writer takes that draft lock too, and the claim sets no `lock_timeout`, so a claim arriving during a slow write waits unbounded while holding its host row, and other tenants' messages on that poll sit unserved. The wallet lock this spec first named was removed by `f74ad2323`, which reads the balance with a plain `SELECT` (`rustd/crates/afd_billing/src/ledger/sql/select_balance.sql`). Separately, concurrent misses on the fleet live-set cache each run the full workspace query (`rustd/crates/afd_fleet_lifecycle/src/live_set.rs:105-112`), and its capacity counts workspaces rather than fleet identifiers. Neither has been measured: the lease benchmark refuses more runners than pool connections (`rustd/crates/afd_bench/src/lane/lease/mod.rs:79`).
**Solution summary:** first add a saturation profile to `make bench-lease` and record a baseline. Then make the claim take the user and draft locks without waiting and skip a candidate whose lock is held, preparing the expensive prompt context outside the locked section and revalidating before commit. Then make live-set misses load once per workspace and weigh capacity by fleet count, using moka features the crate already depends on. Each change stays only if the benchmark shows it helps or holds level.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): skip locked drafts in chat claims and coalesce live-set loads
- **Intent (one sentence):** one busy tenant cannot hold up other tenants' chat turns, and the numbers prove it.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_authoring/src/lease/claim/mod.rs` — `select_claim` and `admit`: host lock, candidate walk, and the busy/skipped/admitted outcomes.
2. `rustd/crates/afd_authoring/src/lease/claim/selection.rs` — lock order (user row, then draft) in `authorize`.
3. `rustd/crates/afd_fleet_lifecycle/src/live_set.rs` — the cache read and fill on a miss.
4. `rustd/crates/afd_bench/src/lane/lease/mod.rs` — the lease lane and the `fit` guard a saturation profile must bypass deliberately.
5. `docs/architecture/concurrency.md` — lock ordering and task rules this change must keep.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_bench/**`, `bench/baselines/lease-saturation.rig.json` | EDIT | Saturation shape: the production poll (`Plane::claim`) over seeded chat drafts across tenants plus concurrent live-set readers; more requesters than connections; pool wait and hold time reported |
| `make/bench.mk` | EDIT | `BENCH_SHAPE=saturation` on the existing `bench-lease` target, rig only; no new target |
| `rustd/crates/afd_authoring/src/lease/claim/**` | EDIT | Non-waiting user and draft locks (claim-only statements), skip on contention, context prepared outside the locked section |
| `rustd/crates/afd_fleet_lifecycle/src/live_set.rs`, `rustd/crates/afd_fleet_lifecycle/src/sql/**`, `rustd/crates/afd_fleet_lifecycle/src/error/**` | EDIT | Coalesced load per workspace; weighted capacity; bounded snapshot; shared-failure error kind |
| `rustd/crates/afd_authoring/tests/**`, `rustd/crates/afd_fleet_lifecycle/tests/**` | EDIT | Contention and coalescing tests |
| `docs/architecture/concurrency.md` | EDIT | Claim skips a locked draft; live-set load coalescing |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (limits and weights are named constants), NDC, NLR, ORP, FLL.
- `.orly/dispatch/write_rust.md` — deterministic concurrency tests; no sleeps standing in for ordering.
- `docs/RUST_ERROR_STANDARD.md` — a skipped candidate is a placement outcome, not an error.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | Context preparation moves to its own function; `admit` stays under the cap |
| UFS | yes | Skip limits, weights and snapshot caps are named constants |
| MILESTONE-ID | yes | No `M229` in source or test names |
| GREPTILE | yes | End-of-turn read of the rules named above |
| LOGGING | yes | One event when a claim skips a locked draft, carrying no tenant content |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/lease/mod.rs` — the fleet lease claim already arbitrates contention with a conditional upsert on a per-fleet slot instead of waiting. moka 0.12 (`rustd/Cargo.toml:381`) supplies `try_get_with` for coalesced loads and a weigher for weighted capacity.

## Sections (implementation slices)

### §1 — Saturation benchmark first — DONE

`make bench-lease BENCH_SHAPE=saturation` (rig only; `PROFILE` keeps meaning the environment) drives the production poll, `Plane::claim`, so chat claims and fleet leases compete as they do in production, over seeded chat drafts across several tenants, with concurrent live-set readers beside it. Requesters outnumber pool connections. It reports lease rate, p95, pool wait and connection hold time separately, and asserts correctness under pressure. **Implementation default:** the saturation shape bypasses the `fit` guard explicitly, because exceeding the pool is its purpose; the default shape keeps the guard.

- **Dimension 1.1** — DONE — the saturation profile completes or refuses every request within its budget, never issues a duplicate lease or exceeds host capacity, and the pool serves normally afterwards → Test `test_saturation_profile_holds_invariants`
- **Dimension 1.2** — DONE — a baseline run on the M229_001 schema is recorded before §2 or §3 changes anything → Test `test_saturation_baseline_recorded` (manual)

### §2 — Chat claims skip a locked draft

Before the existing reads, the claim takes the creator's user row and the draft row with `NOWAIT`, in the order it already locks them; a held lock makes that candidate skipped and the walk continues to the next. The shared lock statements and their other callers are unchanged. Lock order stays host, then user, then draft. Prompt context serialisation and its size check move out of the locked section and are revalidated against the committed state.

- **Dimension 2.1** — DONE — with tenant A's draft held, a claim admits tenant B's message and does not wait on A → Test `test_locked_draft_does_not_block_other_tenant`
- **Dimension 2.2** — DONE — when every candidate's draft is locked, the claim returns no work promptly instead of blocking → Test `test_all_drafts_locked_returns_no_work`
- **Dimension 2.4** — DONE — with tenant A's wallet row held `FOR UPDATE`, a claim still admits A's message; pins `f74ad2323`'s removal of the wallet lock → Test `test_locked_wallet_does_not_block_claim`
- **Dimension 2.3** — a message whose context changed between preparation and commit is not admitted on stale context → Test `test_stale_context_is_revalidated`

### §3 — Live-set loads coalesce

A miss loads through the cache's coalescing loader, so concurrent misses for one workspace run one query. Capacity is weighted by fleet count, and one snapshot is capped by a named constant.

- **Dimension 3.1** — concurrent misses for one workspace run exactly one database read → Test `test_concurrent_misses_read_once`
- **Dimension 3.2** — a workspace with many fleets evicts by weight before several small ones are lost → Test `test_large_workspace_weighs_more`

### §4 — Keep only what the numbers support

Rerun the saturation profile after §2 and after §3. A change that regresses p95 or rate without a correctness gain is reverted, and the decision is recorded with both runs.

- **Dimension 4.1** — Session Notes carry baseline and after-change numbers for each section with the keep or revert decision → Test `test_saturation_results_recorded` (manual)

## Interfaces

```
No API, CLI or wire change. Internal: the chat claim's placement gains a skip-on-locked-draft path;
the live-set read becomes a coalesced load. `make bench-lease` gains BENCH_SHAPE=saturation (rig only).
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Starved tenant | A tenant's draft is locked on every poll | Its message is skipped while locked and admitted on the first poll after release; ordering by age resumes |
| All locked | Every candidate's draft is locked | Claim returns no work; the runner polls again |
| Stale context | Draft changes between context preparation and commit | Revalidation fails; that candidate is skipped this poll |
| Load error under coalescing | The single shared load fails | Every waiter receives the error; nothing is cached; the next read retries |
| Oversized workspace | A workspace's live set exceeds the snapshot cap | Read is refused with a named error rather than materialising an unbounded set |

## Invariants

1. A claim never waits on a user, draft or wallet row lock — user and draft are taken with `NOWAIT`, the wallet is never locked; `test_locked_draft_does_not_block_other_tenant`, `test_locked_wallet_does_not_block_claim`.
2. No duplicate lease and no capacity oversubscription under saturation — asserted by `test_saturation_profile_holds_invariants`.
3. One database read per workspace per concurrent miss — `test_concurrent_misses_read_once`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `authoring_claim_draft_skipped` | ops | A claim skips a candidate whose user or draft row is locked | runner id, outcome | no tenant, draft or message content | `test_locked_draft_does_not_block_other_tenant` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit + bench | `test_saturation_profile_holds_invariants` | the census judge admits a clean run; live, `make bench-lease BENCH_SHAPE=saturation` with requesters at twice the pool size across 5 tenants → 0 duplicate leases, 0 oversubscribed hosts, pool serves every connection again; the live run refuses non-zero (`afd_bench` is outside `make test-integration-rustd`, `make/test-integration-rustd.mk:155`) |
| 1.2 | manual | `test_saturation_baseline_recorded` | baseline run output (rate, p95, pool wait, hold time) pasted in Session Notes before §2 |
| 2.1 | integration | `test_locked_draft_does_not_block_other_tenant` | A's draft held (owner, older) and B's free (member, newer) → claim admits B while A is held, then admits A once released; fails by timeout without the skip |
| 2.2 | integration | `test_all_drafts_locked_returns_no_work` | every candidate draft held → claim returns none without waiting on any |
| 2.3 | integration | `test_stale_context_is_revalidated` | draft changed after preparation → candidate skipped; next poll admits on fresh context |
| 2.4 | integration | `test_locked_wallet_does_not_block_claim` | A's wallet row held `FOR UPDATE` → claim admits A's message |
| 3.1 | integration | `test_concurrent_misses_read_once` | 20 concurrent misses for one workspace → 1 enumerate query |
| 3.2 | unit | `test_large_workspace_weighs_more` | one large and three small workspaces over capacity → large evicted first |
| 4.1 | manual | `test_saturation_results_recorded` | before/after numbers and keep/revert per section in Session Notes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Saturation shape runs and holds invariants (§1) | `make bench-lease BENCH_SHAPE=saturation` | exit 0 | P0 | |
| R2 | Locked draft does not stall other tenants (§2) | `make test-integration-rustd` | exit 0 | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version in sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. A P1 ❌ requires an owner-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Compile-checked query macros and `dispatches.result` holding only private content — separate cleanup once this lands.
- Pool sizing changes — decided from §1's numbers in a later change, not here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — two teams chat with the setup assistant at once; one has a slow write on its draft in flight and the other still gets its reply without waiting.
2. **Preserved user behaviour** — chat ordering by age, billing correctness and the dashboard fleet wall behave as before.
3. **Optimal-way check** — skipping a locked candidate is the smallest change that removes the wait; the fleet lease path already uses the same non-waiting pattern.
4. **Rebuild-vs-iterate** — iterate: two contained changes behind a benchmark.
5. **What we build** — a saturation profile, non-waiting user and draft locks in claims, coalesced live-set loads.
6. **What we do NOT build** — pool resizing, query macros, a new make target.
7. **Fit with existing features** — rests on M229_001's connection time limits; must not destabilise the fleet lease path.
8. **Surface order** — N/A — no user surface; internal claim and cache behaviour.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; operators read the skip event.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** benchmark, then two independent changes, each kept only on the numbers.
- **Alternatives considered:** folding into M229_001 (rejected by the owner: the lease path must stay stable during the rebuild, and nothing here is measured yet); raising pool size (rejected: hides the wait instead of removing it).
- **Patch-vs-refactor verdict:** this is a **patch** because each change is local to one module and reversible on the benchmark's evidence.

## Discovery (consult log)

- **Consults** — Owner decision via AskUserQuestion, Oct 17, 2026: > Indy (2026-10-17): "Yes make the M229_002 as a follow up" — context: chat-claim contention and live-set coalescing move out of M229_001.
  Owner decision via AskUserQuestion, Oct 21, 2026 (CHORE(open) found the claim's wallet lock already removed by `f74ad2323`): > Indy (2026-10-21): "Claude AIWA CTO what is your recommendation here,  i think it seems 2 to me." — option 2 = retarget §2 at the draft and user row locks; agent recommended the same, adding Dimension 2.4 to pin the wallet-lock removal.
  Owner decision via AskUserQuestion, Oct 21, 2026 (PLAN found `PROFILE` already names the environment and no bench lane reaching the chat claim or the live set): Indy chose "BENCH_SHAPE, rig only (Recommended)" and "Real poll + live set (Recommended)".
- **Metrics review** — adds the operator event `authoring_claim_draft_skipped`; no analytics/funnel playbook change, since it is an operator signal with no product funnel.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **§1 baseline** (Oct 21, 2026, reset rig, `bench/baselines/lease-saturation.rig.json`, built over `89bc76631` plus the §1 lane): 40 hosts × 4 workers on a 20-connection pool, 5 chat tenants × 8 drafts, 200 fleets, slow writer holding tenant A's drafts 1 s of every 1.25 s. `leases_per_second=19.80 lease_p95_ms=5394 chat_lease_p95_ms=4760 pool_wait_p95_ms=93.6 hold_p95_ms=973 live_set_p95_ms=33.2`, 0 doubled, 0 oversubscribed, pool recovered. Two `KEEP_TEST_STATE=1` repeats: chat p95 7524 / 5566 ms, rate 16.8 / 20.0 — roughly 25% spread, the margin §4 decisions must clear. Without the slow writer the same shape ran 7055 polls vs 292, chat stall absent.
- **§2 skip, measured** (Oct 21, 2026; three runs per variant, first on a reset rig; same bench, hold sampler excluding the slow writer's own session; only `lease/claim/{mod,selection,sql/mod}.rs` differ). Medians, before (`8fb113cc8` claim) → after: `leases_per_second` 41.7 → 62.7, `chat_lease_p95_ms` 3006 → 707, `lease_p95_ms` 3207 → 745, `hold_p95_ms` 959 → 17, `pool_wait_p95_ms` 28 → 13; every invariant clean in all six. In-set spread about 4% on rate. Decision: keep. The committed baseline is replaced by the before set's reset-rig run, because the earlier one counted the slow writer's holds as the path's.
- **Deferrals** — none.
