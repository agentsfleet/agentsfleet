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

# M151_001: Steer subscribes to the live tail before it sends the message

**Prototype:** v2.0.0
**Milestone:** M151
**Workstream:** 001
**Date:** Jul 30, 2026
**Status:** PENDING
**Priority:** P1 — a fast run can stream its first words before the CLI is listening; those frames are unrecoverable live
**Categories:** CLI
**Batch:** B1 — standalone; no parallel workstream
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) via `make _lint_zig_test_depth`
**Depends on:** none (M150_001 shipped the eager first-frame flush this protects)
**Provenance:** LLM-drafted (Claude Fable 5, Jul 30, 2026) — grounded in a source read of `cli/src/commands/fleet_steer.ts` and `fleet_steer_events.ts`
**Canonical architecture:** `docs/architecture/data_flow.md` §D. WATCH

---

## Overview

**Goal (testable):** `agentsfleet steer` opens its Server-Sent Events (SSE) tail BEFORE posting the message, buffers frames until the `202`'s `event_id` arrives, then replays matching frames — so no frame of the steered event can be published before the subscriber exists.
**Problem:** `steerTurnEffect` posts the message first and subscribes with the returned `event_id` afterward. The activity channel is non-replayable pub/sub, so an unusually fast run — made more likely by the runner's eager first-frame flush — can publish its first frames into a channel nobody is listening to. The CLI then shows an idle wait and recovers only the durable terminal text, never the live opening words. Dashboard chat is unaffected (its stream is open before the user types); this is a CLI-only gap.
**Solution summary:** Reorder one turn of the steer flow: open the event stream first, post the message while the stream is live, hold pre-`event_id` frames in a small bounded buffer, then filter-and-replay once the `202` names the event. Stream-open failure degrades to today's post-then-poll behaviour — subscribing earlier must never make steer less reliable than it is now.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(cli): steer subscribes to the live tail before sending the message
- **Intent (one sentence):** The first words of a fleet's reply always stream live in the terminal, no matter how fast the run starts.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/commands/fleet_steer.ts` — `steerTurnEffect` is the turn being reordered; note the injected `SteerDeps`/`StreamGetFn` seams the tests already use
2. `cli/src/commands/fleet_steer_events.ts` — `tailEventStream` + the terminal-poll fallback; the `event_id` filter this spec moves behind a buffer
3. `cli/test/fleet-steer.integration.test.ts` and `cli/test/fleet-steer-linecov.unit.test.ts` — the harness patterns to extend; mirror their fake-stream injection
4. `docs/architecture/data_flow.md` §D. WATCH and §Two streams + one pub/sub channel — why the channel has no replay and why the durable table is the recovery source
5. `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — the TypeScript discipline for the edited surface

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/commands/fleet_steer.ts` | EDIT | `steerTurnEffect` reordered: stream first, then POST; wires the pre-id buffer |
| `cli/src/commands/fleet_steer_events.ts` | EDIT | tail accepts frames before the event id is known: bounded buffer + filter-and-replay once the id arrives |
| `cli/test/fleet-steer.integration.test.ts` | EDIT | subscribe-before-send order proven through the injected stream |
| `cli/test/fleet-steer-linecov.unit.test.ts` | EDIT | buffer/filter/fallback branch coverage |
| `cli/test/fleet-steer-errors.integration.test.ts` | EDIT | stream-open failure and post-failure paths |
| `docs/v2/pending/M151_001_P1_CLI_STEER_SUBSCRIBE_BEFORE_SEND.md` | EDIT | lifecycle moves and Dimension DONE marks |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (buffer bounds become named constants, no bare numerics), NDC (no dead code: the buffer and fallback are exercised by tests in the same diff), FLL (file/function caps; `steerTurnEffect` stays within the function cap after the reorder), TST-NAM (test names stay milestone-free)
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — `*.ts` surface: `const` discipline, Bun primitives, no new dependencies

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TS FILE SHAPE | yes — at PLAN | extend the two existing command modules; no new files expected |
| File & Function Length (≤350/≤50/≤70) | yes | `fleet_steer.ts` is at 240 and `fleet_steer_events.ts` at 170; if the buffer pushes either near the cap, extract per the repo's `<module>_<concern>` convention |
| UFS (repeated/semantic literals) | yes | pre-id buffer frame/byte caps are named constants, defined once |
| MILESTONE-ID | yes | source and test identifiers stay milestone-free |
| ZIG / UI / DESIGN TOKEN / LOGGING / SCHEMA | no | no Zig, no UI components, no new log emits, no schema |

## Prior-Art / Reference Implementations

- **Reference:** the dashboard's stream registry (`ui/packages/app/lib/streaming/fleet-stream-registry.ts`) — the platform's existing subscribe-early consumer; mirror its posture (stream open before user action, durable list as recovery), not its browser mechanics. The CLI change stays inside the existing `SteerDeps` injection seams, which is what keeps the tests deterministic.

## Sections (implementation slices)

### §1 — Subscribe first, buffer until the event is named

One steer turn becomes: open the stream → POST the message → buffer every arriving frame until the `202`'s `event_id` is known → drop buffered frames for other events, replay the matching ones in arrival order, then continue the existing tail. **Implementation default:** the buffer lives in the events module beside the tail it feeds, capped by named frame-count and byte constants sized to the pre-id window (one `202` round-trip), because a cap without a name is how UFS violations start.

- **Dimension 1.1** — the stream is open before the POST fires (order proven through the injected stream fake) → Test `test_stream_opens_before_post`
- **Dimension 1.2** — frames arriving before the `event_id` is known are buffered and replayed in arrival order once it is → Test `test_pre_id_frames_replayed_in_order`
- **Dimension 1.3** — buffered frames for a different event are dropped, never rendered → Test `test_foreign_event_frames_dropped`
- **Dimension 1.4** — the pre-id buffer is bounded; overflow drops oldest and the tail still functions (durable poll remains the backstop) → Test `test_pre_id_buffer_bounded`

### §2 — Failure paths degrade to today's behaviour

Subscribing earlier must never make steer worse than post-then-subscribe. Every new failure path lands on an existing, tested recovery.

- **Dimension 2.1** — stream fails to open → the turn proceeds exactly as today: POST, then terminal poll; the user sees the same outcome shapes → Test `test_stream_open_failure_degrades_to_poll`
- **Dimension 2.2** — the POST fails after the stream opened → the stream is closed, no orphan connection, the existing error rendering is unchanged → Test `test_post_failure_closes_stream`
- **Dimension 2.3** — abort (Ctrl-C) during the pre-id window → interrupted cleanly, stream closed, no buffered frames rendered → Test `test_abort_in_pre_id_window`

### §3 — Existing surfaces unchanged

- **Dimension 3.1** — REPL multi-turn steer: each turn takes the new order; turn boundaries and prompts render as today → Test: existing `fleet-steer-repl.unit.test.ts` suite passes unmodified
- **Dimension 3.2** — JSON mode output shape (`{ event_id, ...outcome }`) is byte-identical → Test `test_json_mode_shape_unchanged`

## Interfaces

```
No wire or command surface changes. POST /v1/workspaces/{ws}/fleets/{id}/messages,
the SSE stream endpoint, frame shapes, CLI flags, and JSON output are all
unchanged. Only the order of subscribe vs send inside one CLI turn changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stream-open failure | SSE endpoint unreachable / auth rejected | degrade to post-then-poll (today's path); outcome rendering identical; no retry loop added |
| POST failure after subscribe | network / 4xx / 5xx on the message send | close the stream, surface the existing `CliError`; no orphan SSE connection (proven by the fake's close-count) |
| Pre-id buffer overflow | chatty foreign events or a flooded channel during the `202` round-trip | drop-oldest within named caps; matching frames may be lost live but the durable terminal poll still completes the turn |
| Abort mid-window | user Ctrl-C between subscribe and `202` | `failSteerInterrupted` as today; stream closed; nothing rendered |

## Invariants

1. The stream-open call strictly precedes the message POST within a turn — enforced by the ordering test against the injected fakes (1.1); a regression flips the test.
2. No frame belonging to another event is ever rendered — the `event_id` filter applies to buffered and live frames alike (1.3).
3. The pre-id buffer is bounded by named constants — UFS-checked; overflow behaviour is drop-oldest, tested (1.4).
4. Every failure path lands on a pre-existing tested recovery — no new terminal states, no new output shapes (§2, 3.2).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

No new events or log emits: the change is client-side ordering inside one CLI turn. The analytics/funnel playbook needs no update.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_stream_opens_before_post` | injected stream + HTTP fakes record call order → stream-open recorded strictly before POST |
| 1.2 | unit | `test_pre_id_frames_replayed_in_order` | two matching frames arrive pre-id → rendered in arrival order after the id resolves |
| 1.3 | unit (negative) | `test_foreign_event_frames_dropped` | pre-id frames carrying another event id → never rendered, matching frames still replay |
| 1.4 | unit (negative) | `test_pre_id_buffer_bounded` | overflow the named cap pre-id → oldest dropped, no crash, tail continues |
| 2.1 | integration (negative) | `test_stream_open_failure_degrades_to_poll` | stream fake fails to open → POST still sent, terminal poll renders today's outcome |
| 2.2 | integration (negative) | `test_post_failure_closes_stream` | POST fake rejects → stream close-count is 1, existing error surface rendered |
| 2.3 | unit (negative) | `test_abort_in_pre_id_window` | abort signal fires pre-id → interrupted error, stream closed, zero frames rendered |
| 3.1 | regression | existing REPL suite | passes unmodified |
| 3.2 | regression | `test_json_mode_shape_unchanged` | JSON mode → `{ event_id, ...outcome }` byte-identical to today |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Subscribe-before-send proven with buffering and filtering (§1) | `cd cli && bun test 2>&1 \| tail -3` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

Pruned standard rows: S3 integration (no daemon HTTP/schema/Redis change; the CLI integration tier runs inside `bun test` via R1), S4 e2e (the CLI integration tests drive the real command path against fakes at the system boundary — the established house tier for this surface), S5 memleak / S6 cross-compile (no Zig touched), S9 orphan sweep (no deletions).

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- **Wake-the-poller** (the 0–1 s lease-pickup gap) — its own follow-up workstream on the daemon lease surface.
- **Dashboard changes** — the browser stream is already open before the user types; nothing to do.
- **Replayable activity channel** — a server-side redesign the platform deliberately rejected (`data_flow.md`: durable history is the recovery source); this spec closes the gap client-side.
- **Changelog `<Update>`** — rides CHORE(close) in `~/Projects/docs` (cross-repo, own-branch flow).

---

## Product Clarity (authoring record)

1. **Successful user moment** — you run `agentsfleet steer` against a warm, fast fleet and the reply's first words stream into your terminal live — never a silent wait that ends with text appearing all at once from the history fallback.
2. **Preserved user behaviour** — every flag, output shape (human and JSON), REPL turn flow, error message, and recovery path renders exactly as today.
3. **Optimal-way check** — the unconstrained optimum is a replayable channel server-side; the platform's settled design says durable history is the recovery source and the live tail is best-effort. Given that, subscribe-first is the complete client-side fix, not a partial one.
4. **Rebuild-vs-iterate** — patch. One turn's ordering plus a bounded buffer inside existing injection seams; no architectural surface moves.
5. **What we build** — the reorder, the bounded pre-id buffer with filter-and-replay, seven new tests, two regression pins.
6. **What we do NOT build** — server replay, dashboard work, retry loops on stream-open failure (the degrade path is today's behaviour by design).
7. **Fit with existing features** — completes M150's eager first-frame flush (the frames it ships early are now always heard); must not destabilize the steer REPL, pinned by §3.
8. **Surface order** — CLI-only by definition of the gap.
9. **Dashboard restraint** — N/A — no UI surface.
10. **Confused-user next step** — unchanged: a steer that misses live frames still completes from the durable event history, and `agentsfleet events` remains the self-serve history view.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one behavioural Section (the reorder + buffer), one failure Section (each new path degrades to a tested recovery), one regression Section — the smallest split where every Dimension is independently verifiable.
- **Alternatives considered:** (a) poll the durable events list immediately after the `202` to backfill missed frames — rejected: adds a read per steer for a window that subscribe-first eliminates outright, and still shows the opening words late; (b) server-side replay buffer on the activity channel — rejected: contradicts the platform's settled eyeballs-vs-audit split and is a daemon redesign for a client-side ordering bug.
- **Patch-vs-refactor verdict:** this is a **patch** because the injection seams (`SteerDeps`, `StreamGetFn`) already support the reordered flow; the defect is call order, not architecture.

## Discovery (consult log)

- **Consults** — Architecture: `data_flow.md` §D WATCH + §Two streams answer the flow; no conflict (the channel is deliberately non-replayable; this spec works with that grain). Origin: surfaced as finding F3 during the M150_001 review (greptile P1 + Codex cross-model pass), recorded in that spec's Discovery with an Indy go-ahead to spec the CLI fix as its own workstream.
- **Metrics review** — no analytics/funnel playbook update required: no signal changes.
- **Skill-chain outcomes** — (populated as work proceeds)
- **Deferrals** — (none)
