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

# M212_002: A running fleet's schedule writes count against its lease, and the write past `SCHEDULE_WRITES_PER_RUN_MAX` is refused with `UZ-SCHED-012` before it reaches QStash

**Prototype:** v2.0.0
**Milestone:** M212
**Workstream:** 002
**Date:** Oct 06, 2026
**Status:** PENDING
**Priority:** P2 — bounds a cost exposure the 16-live cap leaves open; nothing is broken while it waits
**Categories:** API
**Batch:** B2 — after M212_001 merges; it extends that workstream's verbs and lease row
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M212_001 — the runner schedules verb, and the per-lease count on `fleet.runner_leases` (slot 929) this mirrors
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 06, 2026) from the M212_001 review finding "per-lease schedule-write budget", deferred by Indy into this spec
**Canonical architecture:** `docs/architecture/data_flow.md` §B. TRIGGER (QStash owns the clock)

---

## Overview

**Goal (testable):** `test_schedule_write_budget_refuses_with_code` — after `SCHEDULE_WRITES_PER_RUN_MAX` creates, updates and deletes through one lease, the next write answers 409 `UZ-SCHED-012` with `current_state: at_capacity`, stores nothing and makes no QStash call; reads, runs listings and run-now spend nothing.
**Problem:** A fleet holds at most 16 schedules it made, but nothing bounds how often it changes them. A model stuck in a loop can create and delete a schedule over and over within one run, and every change is a synchronous call to QStash, the external scheduler. The operator pays for that churn, and every fleet on a deployment shares one QStash account (`QSTASH_TOKEN`), so one fleet's burst spends the rate limit the others fire through.
**Solution summary:** Each lease carries a count of schedule writes (slot 931), incremented by one guarded statement that also proves the fence, exactly as `messages_posted` does. The schedules verb's `POST`, `PATCH` and `DELETE` take one from the budget after the body is validated and before the store or QStash is touched. The write past the budget is refused with a new registry code; the cron tools hand that code to the model, which can say so in its answer. A new lease starts at zero.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): bound a running fleet's schedule writes per lease
- **Intent (one sentence):** A fleet that loops on its schedule tools stops after a fixed number of changes per run, so the operator's QStash bill and rate limits are bounded.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/sql/standing.rs` — `COUNT_MESSAGE`: the fence and the cap in one `UPDATE`; the write count copies it.
2. `rustd/crates/afd_fleet/src/lease/message.rs` — how the messages verb spends a slot, and re-proves standing on no row to tell a lost fence from a full cap.
3. `rustd/crates/afd_api_runner/src/handler/runner/schedule_edit.rs` — the `PATCH` and `DELETE` handlers that gain the charge; `schedule.rs` beside it holds `create`.
4. `schema/929_runner_leases_messages_posted.sql` — the additive column shape slot 931 repeats.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — §5 registry codes and the 409 body's `current_state`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/931_runner_leases_schedule_writes.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | The per-lease write count, default 0 |
| `rustd/crates/afd_fleet/src/lease/sql/standing.rs`, `rustd/crates/afd_fleet/src/lease/schedule_write.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/src/lease/schedule_write/tests.rs` | CREATE / EDIT | The fenced count statement and the plane method that spends one write |
| `rustd/crates/afd_http/src/services/leasing.rs` | EDIT | The lease seam's charge, beside `message` and `masked` |
| `rustd/crates/afd_api_runner/src/handler/runner/schedule.rs`, `rustd/crates/afd_api_runner/src/handler/runner/schedule_edit.rs` | EDIT | Charge after validation, before the store; refusal and OpenAPI 409 text |
| `rustd/crates/afd_wire/src/schedule_verb.rs` | EDIT | `SCHEDULE_WRITES_PER_RUN_MAX`, imported by the daemon and the tests |
| `rustd/crates/afd_core/src/error_code/request.rs`, `rustd/crates/afd_core/src/problem/request.rs` | EDIT | `SCHEDULE_WRITE_LIMIT_REACHED` and its problem |
| `rustd/crates/afr_tools/src/verbs/schedules/tests.rs` | EDIT | The cron tools hand the new code to the model |
| `rustd/crates/agentsfleetd/tests/integration_runner_schedules_budget.rs`, `rustd/crates/agentsfleetd/tests/support/e2e_schedules.rs` | CREATE / EDIT | Live proofs against Postgres, Dragonfly and the fake QStash |
| `public/openapi.json` | EDIT | Regenerated |
| `docs/architecture/data_flow.md` | EDIT | The budget beside the 16-schedule cap |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (the budget is one constant the daemon and tests import), STS (the column is an integer; no `CHECK`, no `DEFAULT 'value'`), ERR (the code is declared, registered and referenced), OBS (the refusal logs ids and codes only), TST-NAM, TCF, ORP.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`; `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` §5 and §7; `docs/LOGGING_STANDARD.md` — never log a schedule message.
- `dispatch/write_sql.md` — an additive slot; no `DROP`, no edit to a shipped slot.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ERROR REGISTRY | yes | `UZ-SCHED-012` declared in `afd_core`, mapped to its problem, used by both handler files |
| SCHEMA GUARD | yes | Forward migration `schema/931_runner_leases_schedule_writes.sql`, registered in `afd_db`; `INTEGER NOT NULL DEFAULT 0` |
| UFS / LOGGING | yes | `SCHEDULE_WRITES_PER_RUN_MAX` and `STATE_AT_CAPACITY` named once; refusal logged with ids and code |
| Architecture consult | yes | `data_flow.md` §B states the 16 cap; the budget lands beside it in the same commit |
| File & Function Length (≤350/≤50/≤70) | yes | The plane method in its own `schedule_write.rs`; handlers gain one call each |

## Prior-Art / Reference Implementations

- **Reference:** the messages verb's per-run count (`afd_fleet::lease::message` over `COUNT_MESSAGE`, slot 929) — same fence-and-cap statement, same re-proof on no row, same 409 `at_capacity`. The budget differs only in what it guards.

## Sections (implementation slices)

### §1 — Every schedule write spends one from the lease's budget

A `POST`, `PATCH` or `DELETE` on the runner schedules verb validates its body, then spends one write through a single `UPDATE` on `fleet.runner_leases` that checks the fencing token, the lease status and expiry, the affinity sequence and `schedule_writes < SCHEDULE_WRITES_PER_RUN_MAX`, and increments the count. Only then does it touch `core.fleet_schedules` or QStash. No row back → re-read standing: a lost fence answers the fence refusal, a held lease answers `SCHEDULE_WRITE_LIMIT_REACHED`. A refusal by the store after the charge (cap of 16, not fleet-owned) still spends the write. **Implementation default:** `SCHEDULE_WRITES_PER_RUN_MAX = 32` because it lets one run rebuild all 16 of its schedules twice and stops a loop within one run.

- **Dimension 1.1** — Create, update and delete each spend exactly one → Test `test_schedule_writes_count_against_the_lease`
- **Dimension 1.2** — The write past the budget answers 409 `UZ-SCHED-012`, `current_state: at_capacity`, with no row change and no QStash call → Test `test_schedule_write_budget_refuses_with_code`
- **Dimension 1.3** — Writes racing for the last slot never pass the budget → Test `test_concurrent_schedule_writes_never_pass_the_budget`
- **Dimension 1.4** — A superseded holder's write answers the fence refusal and spends nothing → Test `test_superseded_holder_spends_no_schedule_write`
- **Dimension 1.5** — A body the validator refuses spends nothing → Test `test_refused_body_spends_no_schedule_write`

### §2 — Reads and run-now stay free, and a new lease starts at zero

The list, the runs listing and run-now never charge: run-now is already bounded to one run per leased event by its `run:<event_id>` key. The count lives on the lease, so the next event's lease, or a reclaim, starts at zero.

- **Dimension 2.1** — List, runs and run-now leave the count unchanged → Test `test_reads_and_run_now_spend_no_write`
- **Dimension 2.2** — A new lease on the same fleet starts with the full budget → Test `test_new_lease_starts_with_a_fresh_budget`

### §3 — The code reaches the model and the reference

`SCHEDULE_WRITE_LIMIT_REACHED` is registered with its problem and declared on the three routes' 409; the cron tools return it as the tool's error code, as they do every refusal.

- **Dimension 3.1** — The code has a problem, a 409 status and a sentence naming the budget from its constant → Test `test_schedule_write_limit_problem_is_registered`
- **Dimension 3.2** — `cron_add`, `cron_update` and `cron_remove` hand the code to the model → Test `test_cron_tools_name_the_write_budget`

## Interfaces

```
fleet.runner_leases.schedule_writes INTEGER NOT NULL DEFAULT 0      (slot 931)
SCHEDULE_WRITES_PER_RUN_MAX = 32                                     (afd_wire::schedule_verb)
Charged:   POST /v1/runners/me/leases/{lease_id}/schedules
           PATCH /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}
           DELETE /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}
Free:      GET …/schedules · GET …/schedules/{schedule_id}/runs · POST …/schedules/{schedule_id}/runs
Refusal:   409 { error_code: "UZ-SCHED-012", current_state: "at_capacity", … }
Code:      SCHEDULE_WRITE_LIMIT_REACHED (UZ-SCHED-012, 409)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Budget spent | Looping model | 409 `UZ-SCHED-012`; nothing stored, no QStash call; the tool returns the code (Dimension 1.2) |
| Race for the last slot | Parallel tool calls | One guarded `UPDATE` admits exactly the budget (Dimension 1.3) |
| Stale fence | Reclaimed lease writes late | Fence refusal, count unchanged (Dimension 1.4) |
| Malformed body | Model sends a bad cron or zone | Validator refusal, count unchanged (Dimension 1.5) |
| Retried `PATCH` | Runner repeats after a lost reply | Spends a second write; the budget's headroom absorbs it (Dimension 1.1) |
| Mixed versions | Old daemon during rollout | The column defaults to 0 and an old build never reads it; nothing is charged until the new build serves (Dimension 2.2) |

## Invariants

1. A lease never records more than `SCHEDULE_WRITES_PER_RUN_MAX` writes — enforced by the `schedule_writes < $cap` predicate in the one `UPDATE` that increments it (Dimension 1.3).
2. No schedule row changes and no QStash call happens for a write the budget refused — the charge precedes every store call in each handler, and the handler returns on refusal (Dimension 1.2).
3. A write only spends against a lease that still holds its fleet — the fence is in the same statement as the count (Dimension 1.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet_schedule_refused` (daemon log, info; existing) | ops | The budget refuses a write | `fleet_id`, `error_code` = `UZ-SCHED-012` | No body, no schedule message | `test_schedule_write_budget_refuses_with_code` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_schedule_writes_count_against_the_lease` | create, patch, the same patch again, delete → `schedule_writes` 1, 2, 3, 4 |
| 1.2 | integration | `test_schedule_write_budget_refuses_with_code` | count at the budget, then create → 409 `UZ-SCHED-012`, `at_capacity`, row count unchanged, fake QStash 0 new calls |
| 1.3 | integration | `test_concurrent_schedule_writes_never_pass_the_budget` | budget − 2 spent, 8 parallel creates → exactly 2 admitted, count = budget |
| 1.4 | integration | `test_superseded_holder_spends_no_schedule_write` | reclaim, then the old token writes → fence refusal, count 0 |
| 1.5 | integration | `test_refused_body_spends_no_schedule_write` | `* * * * * *` cron → 400, count 0 |
| 2.1 | integration | `test_reads_and_run_now_spend_no_write` | list, runs, run-now → count 0 |
| 2.2 | integration | `test_new_lease_starts_with_a_fresh_budget` | a fresh lease reads `schedule_writes` 0; spend the budget, settle, lease the next event → a create succeeds |
| 3.1 | unit | `test_schedule_write_limit_problem_is_registered` | registry → 409, sentence contains the budget's value |
| 3.2 | unit | `test_cron_tools_name_the_write_budget` | daemon answers 409 `UZ-SCHED-012` → tool error carries `UZ-SCHED-012` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The budget refuses a looping fleet (§1, §2) | `make test-integration-rustd && grep -cE "fn test_(schedule_write_budget_refuses_with_code\|concurrent_schedule_writes_never_pass_the_budget)\(" rustd/crates/agentsfleetd/tests/integration_runner_schedules_budget.rs` | 2 | P0 | |
| R2 | The code is declared (§3) | `grep -c '^pub const SCHEDULE_WRITE_LIMIT_REACHED: ErrorCode = ErrorCode::declare("UZ-SCHED-012");' rustd/crates/afd_core/src/error_code/request.rs` | 1 | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md`; a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- A per-fleet or per-workspace write budget across runs — the per-run bound answers a loop; a budget across runs is a quota and a product decision.
- Charging run-now — it is already one run per leased event.
- The public pages in `~/Projects/docs` (the `UZ-SCHED-012` row and the tools page's limit line) ride their own branch, never this worktree.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet whose prompt loops on "reschedule the check" stops after 32 changes; its answer says it could not change schedules again this run, and the operator's QStash usage shows 32 calls, not thousands.
2. **Preserved user behaviour** — A run that creates, changes or removes a handful of schedules sees no difference; schedules people make through the tenant routes and `agentsfleet schedule` are never charged.
3. **Optimal-way check** — One guarded count on a row the verb already proves, with no new table and no new round trip beyond the one `UPDATE`.
4. **Rebuild-vs-iterate** — Iterate on the messages verb's shape.
5. **What we build** — One column, one statement, one plane method, one code, two handler calls.
6. **What we do NOT build** — Cross-run quotas, dashboard counters, an operator knob for the budget.
7. **Fit with existing features** — Compounds with the 16-schedule cap; must not change how a person's schedule edits behave.
8. **Surface order** — API only; the tools surface the code.
9. **Dashboard restraint** — Nothing new on the dashboard until a fleet is seen hitting the budget.
10. **Confused-user next step** — The error page for `UZ-SCHED-012` says the run spent its schedule changes and the next run starts fresh.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream; the count, the code and the proofs are one behaviour.
- **Alternatives considered:** a time-window rate limit in Dragonfly (rejected: a second authority beside the lease row, and a window outlives the run it guards); counting only writes that reach QStash (rejected: a charge after the store call cannot refuse before it).
- **Patch-vs-refactor verdict:** this is a **patch** because the verb, the lease row and the count pattern exist.

## Discovery (consult log)

- **Consults** — M212_001's `/review` found unbounded schedule-write churn per lease. Indy deferred it here.
- **Metrics review** — No analytics or funnel playbook update required: no user action; the existing `fleet_schedule_refused` event carries the new code.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none of its own. Inherited from M212_001:

> Indy (2026-10-06 07:58): "Ship the other 10 open review items as recorded yes" — context: the per-lease schedule-write budget leaves M212_001's review for this spec.
