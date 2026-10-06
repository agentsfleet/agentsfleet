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

# M211_002: `delegate` and `spawn` run nested loops inside the same lease — same sandbox, same key, same budget, Codex's `spawn_agent` / `wait_agent` / `send_input` shape — and every child ends once, with its parent

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 002
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — the last published tools without a Rust home; a fleet that splits an incident into parallel reads cannot move until they exist
**Categories:** API, INFRA
**Batch:** B2 — the milestone's follow-up Pull Request, after M211_001 merged; M211_003 to M211_005 fold into this spec at its CHORE(open), and its Sections run on M211_001's tools
**Branch:** pending — set at CHORE(open)
**Baseline revision:** `b0138d7b3124b871668f07e2361dba923bc774d2`
**Test Baseline:** pending — measured before the Pull Request; shared with M211_001
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_002 (the loop, the catalog, the router, the run-wide call counter and trace) · M211_001 (children share the sandbox-side tools)
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from `docs/architecture/runner_execution.md` §"Tool catalog" and Indy's decision that the runner carries every published tool; Codex at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Tool catalog" (`delegate`, `spawn`: a nested loop sharing the lease's sandbox and budget), §Process model; `docs/architecture/runner_fleet.md` §"Live activity (the SSE tail)"

---

## Overview

**Goal (testable):** `test_delegate_returns_child_answer` — a run whose model calls `delegate { task: "read a.md and b.md and summarise" }` starts a child loop with the same provider and key, the task as its prompt and a subset of the parent's tools; the child's two `file_read` calls emit frames and trace rows under the run's own counter; the child's answer returns as the `delegate` call's output; the report's token counts include the child's; and when the parent is killed while a `spawn`ed child is mid-call, that child's call ends `interrupted` exactly once.
**Problem:** M210_002 refuses a lease that lists `delegate` or `spawn`, so a fleet that fans an investigation out, or hands a sub-task to a focused prompt, cannot run on Rust. Codex solves the same need with `spawn_agent`, `wait_agent`, `send_input`, `list_agents` and `interrupt_agent` (`core/src/tools/handlers/multi_agents_spec.rs`), and the published page promises `delegate` and `spawn`.
**Solution summary:** A child is a nested `afr_agent::Loop` inside the parent's lease: same provider and key, same sandbox and workspace, same memory store, the run-wide call counter, a tool set that is a subset of the parent's, and depth and count caps. `delegate` runs a child to its answer and returns it; `spawn` returns a child id for `wait_agent`, `send_input`, `list_agents` and `interrupt_agent`. The parent's end, for any reason, ends every child, and every child call ends once. Usage sums into the one report.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): delegate and spawn as nested loops in the same lease
- **Intent (one sentence):** A fleet can split its work into focused sub-runs that share its sandbox and budget, every one of them visible in the thread and ended with the run.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/done/M210_002_P1_API_INFRA_RUST_RUNNER_AGENT_LOOP_AND_HOSTED_TOOLS.md` §1–§2 — the loop a child reuses, the counter and trace it shares.
2. `docs/architecture/runner_execution.md` — §"Tool catalog", §Process model: where the loop lives and what a sandbox is to it.
3. `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/report.rs` — the frames a child emits and the one report its usage sums into.
4. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs — `core/src/tools/handlers/multi_agents_spec.rs` and `multi_agents.rs` (`spawn_agent`, `wait_agent`, `send_input`, `list_agents`, `interrupt_agent`, their arguments and the wait semantics).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_agent/src/nested/` (`child.rs`, `registry.rs`, `spawn.rs`, `delegate.rs`, `wait.rs`, `input.rs`, `interrupt.rs`, `error.rs`) | CREATE | The child loop, the per-run registry, the six handlers' logic |
| `rustd/crates/afr_agent/src/loop.rs`, `rustd/crates/afr_agent/src/context.rs` | EDIT | A loop can be a child: depth, a shared counter, a shared budget |
| `rustd/crates/afr_tools/src/catalog.rs`, `rustd/crates/afr_tools/src/nested.rs` | EDIT / CREATE | `delegate`, `spawn`, `wait_agent`, `send_input`, `list_agents`, `interrupt_agent` as `Supervisor` entries |
| `rustd/crates/afr_agent/tests/nested/` | CREATE | Unit proofs with the scripted provider |
| `rustd/crates/agentsfleetd/tests/support/bundles/delegating_triager/`, `rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | CREATE / EDIT | A bundle that fans two reads out to children, against the real daemon |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (depth, concurrency and total caps are constants), OWN (one owner per child; the registry owns the set and ends it), FLS (a child's provider stream and tool outputs drained on every exit path, including the parent's), TIM (wait timeouts explicit), OBS, ERR-RS, TST-NAM, TCF, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — one `ErrorKind` per crate; a refused spawn carries its code to the model.
- `docs/LOGGING_STANDARD.md` — never log a task prompt or a child's answer; log ids, depth and counts.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` and `error_lifts!` only |
| UFS / LOGGING / MILESTONE-ID | yes | Constants per concern; scoped events; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | One handler per file under `nested/` |
| Architecture consult | yes | `runner_execution.md` §"Tool catalog" names the nested loop; drift is fixed in the same commit |

## Prior-Art / Reference Implementations

- **Reference:** Codex multi-agents (`2e5fea64e`) — `spawn_agent { message, agent_type?, model?, reasoning_effort? }` returns an id; `wait_agent { agent_id, timeout_ms }` returns the result or a still-running answer; `send_input { agent_id, message }` appends to a running child; `list_agents`, `interrupt_agent`. Taken as the tool shape; ours drops `model` and `agent_type`, because a lease has one provider and one policy.
- **Reference:** the loop itself (M210_002 §2) — a child is the same loop with a different prompt and a narrower catalog; nothing new runs a turn.

## Sections (implementation slices)

### §1 — A child shares the lease and can only have less

A child loop runs with the parent's provider and key, its task as the user turn, the parent's system prompt and trusted repair context, the same sandbox, workspace and memory store, and a tool set that is the requested subset of the parent's tools; a requested tool the parent lacks is refused. At `NESTED_DEPTH_MAX` the child is offered none of the six nested tools. At most `CHILDREN_RUNNING_MAX` children run at once and `CHILDREN_PER_RUN_MAX` are started per run; past either, `spawn` and `delegate` refuse with a code. The context cap and token accounting are the run's: a child's turns count against them, and its usage sums into the one report.

- **Dimension 1.1** — `delegate` runs a child to its answer and returns it as the call's output → Test `test_delegate_returns_child_answer`
- **Dimension 1.2** — `spawn` returns a child id; `wait_agent` reports running then done; `send_input` reaches the child's next turn → Test `test_spawn_wait_send_round_trip`
- **Dimension 1.3** — A child at the depth cap is offered no nested tool → Test `test_nested_depth_capped`
- **Dimension 1.4** — The running and per-run caps refuse with their codes → Test `test_children_caps_refuse`
- **Dimension 1.5** — A child never holds a tool its parent lacks → Test `test_child_tools_subset_of_parent`
- **Dimension 1.6** — Children's usage sums into the report's three counts → Test `test_child_usage_sums_into_report`

### §2 — Children are visible as the run's own calls

A child's tool calls take ids from the run-wide counter and emit the same frames and trace rows as the parent's; the parent's `delegate` or `spawn` call is itself a call whose output is the child's answer or id. The trace caps count every call of the run. The thread therefore shows a child's reads as rows of the same turn.

- **Dimension 2.1** — Child calls carry run-wide ids and appear in the frames and the trace → Test `test_child_calls_share_the_run_trace`
- **Dimension 2.2** — The trace's 200-call cap counts child calls → Test `test_trace_cap_counts_child_calls`

### §3 — Every child ends with its parent, and every child call ends once

When the parent's run ends for any reason — answer, kill, timeout, provider failure, context cap — the registry ends every running child, and each child's open calls close `interrupted` exactly once. `interrupt_agent` ends one child the same way; `list_agents` reports each child's state. A child that ends on its own leaves its answer for `wait_agent`.

- **Dimension 3.1** — Parent end interrupts every running child; open calls close once → Test `test_parent_end_interrupts_children`
- **Dimension 3.2** — `interrupt_agent` ends one child; `list_agents` shows running, done and interrupted → Test `test_interrupt_and_list_agents`

### §4 — A bundle fans out against the real daemon

The integration lane adds a support bundle that delegates two file reads to two children and summarises, driven by the fake model, and checks the thread's trace holds the parent's `delegate` calls and the children's reads under one counter.

- **Dimension 4.1** — The delegating bundle runs end to end and its trace holds parent and child calls → Test `test_delegating_bundle_roundtrip`

## Interfaces

```
delegate        { task, tools? }              → { answer }                     (runs to completion)
spawn           { task, tools? }              → { child_id }
wait_agent      { child_id, timeout_ms? }     → { status: running|done|interrupted, answer? }
send_input      { child_id, message }         → { accepted: bool }
list_agents     {}                            → [{ child_id, status, depth, calls }]
interrupt_agent { child_id }                  → { status }

Constants: NESTED_DEPTH_MAX 2 · CHILDREN_RUNNING_MAX 4 · CHILDREN_PER_RUN_MAX 16
Codes: CHILD_CAP_REACHED · CHILD_TOOL_NOT_HELD · CHILD_NOT_FOUND
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Too many children | Fan-out loop or injection | `CHILD_CAP_REACHED`; the parent continues (Dimension 1.4) |
| Child asks for a tool the parent lacks | Prompt names a wider set | `CHILD_TOOL_NOT_HELD`; nothing started (Dimension 1.5) |
| Child fails its provider call | Upstream fault | The child ends; `wait_agent` or `delegate` returns the failure as its output; the parent decides |
| Parent ends with children running | Kill, timeout, cap, answer | Children interrupted; their open calls close once (Dimension 3.1) |
| Unknown child id | Model fault | `CHILD_NOT_FOUND` |
| Context cap reached by a child | Long fan-out | The run's cap applies: the child is asked for its answer with no tools, as the parent would be |

## Invariants

1. A child never holds more than its parent: tools, depth, budget (Dimensions 1.3, 1.5).
2. Every child call ends exactly once, and no child outlives its parent (Dimensions 3.1, 2.1).
3. One report per lease: children have no report, no lease, no fence of their own (Dimension 1.6).
4. The model key and the sandbox are the lease's; a child creates neither.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `child_started` / `child_ended` (runner log, info) | ops | A child starts or ends | lease id, child id, depth, status, calls | No task text, no answer | `test_spawn_wait_send_round_trip` |
| `child_refused` (runner log, info) | ops | A cap or subset refusal | lease id, `error_code` | No task text | `test_children_caps_refuse` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_delegate_returns_child_answer` | scripted child: 2 reads then "summary" → `delegate` output `summary` |
| 1.2 | unit | `test_spawn_wait_send_round_trip` | spawn → id; wait 0 ms → running; send "also c" → child's next turn holds it; wait → done |
| 1.3 | unit | `test_nested_depth_capped` | child at depth 2 → catalog offered has none of the six |
| 1.4 | unit | `test_children_caps_refuse` | 5th running → `CHILD_CAP_REACHED`; 17th total → same |
| 1.5 | unit | `test_child_tools_subset_of_parent` | parent `[file_read]`, child asks `[file_read, shell]` → `CHILD_TOOL_NOT_HELD` |
| 1.6 | unit | `test_child_usage_sums_into_report` | parent (10,0,5) + child (20,4,6) → 30, 4, 11 |
| 2.1 | unit | `test_child_calls_share_the_run_trace` | parent call 1, child calls 2 and 3 → frames and trace rows with ids 1..3 |
| 2.2 | unit | `test_trace_cap_counts_child_calls` | 150 parent + 60 child calls → `omitted_call_count` 10 |
| 3.1 | unit | `test_parent_end_interrupts_children` | kill parent mid-child-call → one `interrupted` for that call, child `interrupted` |
| 3.2 | unit | `test_interrupt_and_list_agents` | 2 spawned, interrupt one → list shows `running`, `interrupted` |
| 4.1 | integration | `test_delegating_bundle_roundtrip` | support bundle → report answer; trace holds 2 `delegate` rows and 4 `file_read` rows under one counter |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Children share the lease and end with it (§1–§3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_agent nested` | exit 0 | P0 | |
| R2 | A delegating bundle round-trips against the real daemon (§4) | `make test-integration-rustd && grep -c "fn test_delegating_bundle_roundtrip(" rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | 1 | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- A parent-child link in the frames and the thread's cells, so a child's rows fold under its `delegate` row — a wire and chat change for a later spec; here a child's rows are the turn's rows.
- A child on a different model or provider — a lease has one.
- Children across leases or hosts — a child is a loop, not a lease.
- `request_user_input` and `send_message_to_user` — the chat's approval line and the messages verb (M212_001) are their homes.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet asked "why is checkout slow since Tuesday" delegates the deploy history, the error logs and the diff to three children, and the thread shows all three investigations as rows of one turn before the summary.
2. **Preserved user behaviour** — Tool names `delegate` and `spawn` stay as published; a fleet that never lists them runs exactly as before.
3. **Optimal-way check** — A child is the same loop with a narrower catalog, so nothing new runs a turn, and the lease's single report, fence and sandbox stay single.
4. **Rebuild-vs-iterate** — Iterate on M210_002's loop.
5. **What we build** — A child loop, a registry, six handlers, three codes, one support bundle.
6. **What we do NOT build** — Parent links in frames, other models, cross-lease children (see Out of Scope).
7. **Fit with existing features** — Children's calls render through M209_002's cells unchanged; the figures line sums them because the report does.
8. **Surface order** — API first; the published tools page gains the four Codex-shaped names at cutover.
9. **Dashboard restraint** — N/A — no user surface; a refused spawn is a red cell with its code.
10. **Confused-user next step** — N/A — no user surface; `child_refused` names the cap or the tool.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** nested loops in-process, sharing everything the lease holds, because a child that needed its own lease, sandbox or key would be a second fleet, with a second fence and a second report to reconcile.
- **Alternatives considered:** children as separate leases on other runners (rejected: a fleet's affinity slot admits one live holder, `docs/architecture/runner_fleet.md` §"Memory continuity"); per-child sandboxes (rejected: the workspace is the point of sharing); unlimited depth (rejected: a recursion is a budget drain).
- **Patch-vs-refactor verdict:** this is a **patch** because the loop, catalog and router exist; a child is a parameterisation.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "i need all the tools … the sandbox isnt just a sandbox but a harness that decide to operate like codex"; chose "Supervisor loop, sandbox tools". The Codex shape is `spawn_agent`/`wait_agent`/`send_input`; `delegate` and `spawn` keep the published names and the four Codex-shaped names are added.
- **Agent defaults** — depth 2; four running and sixteen per run; a child inherits the parent's system prompt and trusted repair context; children's rows ride the run's counter with no parent link until the chat can fold them.
- **Metrics review** — No analytics or funnel playbook update required: no user surface; three operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
- **Carried from M211_001** — three greptile findings on PR #732, taken up at this spec's PLAN:
  > Indy (2026-10-06, before 23:33): "Okay so fix the apply_path.rs:92 issue only, and reply to others on deferral. upon fix push the PR" — context: `afr_tools/src/sandbox/apply_patch.rs:257`, a move drops the execute bit; `afr_tools/src/sandbox/output.rs:89`, the drain outlasts its yield under a flood; `afr_sandbox/src/toolbox/holds.rs:71`, a crash leaves the rollback release mounted. `apply_patch.rs:92` was fixed on that PR in `1000de460`; PR #732 Session notes 4 and the three threads carry the detail.
