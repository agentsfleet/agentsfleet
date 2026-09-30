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

# M207_005: The fleet chat holds still while a reply runs, on assistant-ui's own steer queue

**Prototype:** v2.0.0
**Milestone:** M207
**Workstream:** 005
**Date:** Sep 30, 2026
**Status:** DONE
**Priority:** P1 — operators watch every fleet reply here; a fold that drags the thread and a flash of reasoning break reading
**Categories:** API, DOCS, INFRA, UI
**Batch:** B1 — sole workstream; carries M207_003's 6.7 leftovers and the M207_003/M207_004 listed items
**Branch:** `fix/m207-003-design-review`
**Baseline revision:** `115e8ba668145e6407c8a47459bc87869fa358c2`
**Test Baseline:** unit=8706 integration=683 — unit `make test-unit-all` exit 0 — Rust 2,879 passed / 0 failed / 708 ignored (162 binaries), app 3,268 (353 files), website 142 (22), cli 1,777 passed / 16 skipped, design-system 640 (60); integration `make test-integration-rustd` exit 0 — 683 passed / 0 failed over 140 binaries (681 + 2 exclusive)
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M207_005-115e8ba66.md`
**Depends on:** M207_003 (steer route, pending-sends ledger, Resend), M207_004 (streaming budget, catching-up)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026)
**Canonical architecture:** `docs/architecture/data_flow.md` §Steer flow end-to-end

---

## Overview

**Goal (testable):** Folding a Thought, sending while a reply runs, and a reply settling each leave the operator's view where it was, with no text sliding or flashing past.
**Problem:** Folding a Thought at the bottom of the thread drags the whole reasoning past the viewport (measured Sep 30: `scrollTop` clamps 9304→7384 over the 200 ms fold). The thread jumps 140 px twice after Send. The reconnect notice is destructive red for a state that heals itself. Four routes answer an unregistered `UZ-401`. Streaming markdown is hand-rolled beside assistant-ui's own renderer. Five test trees copy one tracing recorder, and the runner's Zig tests run in no lane.
**Solution summary:** The steer moves onto assistant-ui's external-store `queue` adapter, so the thread reports `isRunning` truthfully and the composer stays enabled through a run. That unlocks `ThreadPrimitive.Viewport turnAnchor="top"`, whose reserve under the last turn holds the view through folds. Reply markdown keeps the block parser, which measured 21× faster than the library's renderer (§3, cut). The four routes answer `UZ-AUTH-401`, the recorder becomes one `afd_core` test utility, the Zig tests join the unit lane, and the user docs describe the result.

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(app): the chat holds still — steer on the queue, folds anchored`
- **Intent (one sentence):** An operator reading a running fleet reply never loses their place when the reply folds, settles, or when they send another message.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/components/domain/useFleetMessageDelivery.ts` — the one admission path (`onNew`, `resend`, `deliver`); the queue adapter must call it, never duplicate it.
2. https://github.com/assistant-ui/assistant-ui — installed as `@assistant-ui/core` 0.3.21 and `@assistant-ui/react` 0.15.22: `external-store-thread-runtime-core` sends a non-edit message to `queue.steer` while running, else `queue.enqueue`, and never to `onNew`; `topAnchor/topAnchorTurn` engages the reserve only while `thread.isRunning` with user → assistant as the last two messages.
3. `ui/packages/app/components/domain/FleetMarkdown.tsx` — the block parser §3 measured and kept.
4. `docs/RUST_ERROR_STANDARD.md` and `dispatch/write_rust.md` — before any `rustd/` edit.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | runtime gets `queue` and an `isRunning` for the viewer's own turns |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | EDIT | exposes the delivery path to the queue adapter |
| `ui/packages/app/components/domain/useFleetSteerQueue.ts` | CREATE | the `ExternalThreadQueueAdapter` over the delivery path |
| `ui/packages/app/components/domain/FleetThreadViewport.tsx` | EDIT | `turnAnchor="top"` beside `autoScroll`; the offline notice rides in the footer above the composer |
| `ui/packages/app/components/domain/{fleetMessageRenderers.tsx,fleetReplyMessage.ts,useFleetSteerQueue.ts}`, `ui/packages/app/lib/{events/event-summary.ts,streaming/fleet-stream-reply-frames.ts}` | EDIT | only a settled reply skips layout; `isSteerBy` names the viewer's own turn; a non-refusal failure is dropped, not rethrown |
| `ui/packages/app/components/domain/FleetConnectionNotice.tsx` | EDIT | `warning` variant |
| `ui/packages/app/components/domain/FleetThought.tsx` | EDIT | fold lock verified under the reserve |
| `ui/packages/app/components/domain/{FleetReplyBody.tsx,FleetThought.tsx}` | EDIT | a thought that resumes after the answer stays folded |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/{events,events/stream,fleets/[fleetId]/events,fleets/[fleetId]/events/stream}/route.ts` | EDIT | answer `UZ-AUTH-401` as the steer route does |
| `rustd/crates/afd_core/{Cargo.toml,src/lib.rs,src/test_util.rs,src/test_util/trace.rs,tests/core_suite.rs,tests/trace.rs}`, `rustd/Cargo.lock` | EDIT / CREATE | the one tracing capture, behind `test-util` |
| `rustd/crates/{agentsfleetd/src/supervisor/tests.rs,afd_dragonfly/tests/support/recorder.rs,afd_events/tests/support/recorder.rs,afd_fleet/tests/support/fleet_log.rs,afd_fleet/src/lease/test_log.rs}` | EDIT / DELETE | migrate the five copies |
| `rustd/crates/{agentsfleetd,afd_dragonfly,afd_events,afd_fleet}/**` | EDIT | each copy's suite `mod` line, call sites, and a `test-util` dev-dependency on `afd_core` |
| `make/test.mk`, `make/test-unit.mk`, `build_runner.zig`, `scripts/runner_zig_version_test.py`, `playbooks/operations/acceptance/baselines/M207_005-115e8ba66.md` | EDIT / CREATE | runner Zig tests in `test-unit-all`; version guard and its self-test; stale target comment |
| Tests beside each file above; `tests/e2e/acceptance/{fleet-thread,fleet-thread-anchor,fleet-reply-parts,fleet-resend}.spec.ts`, `fixtures/{reply-page,page-event-stream}.ts`; `tests/bench/fleet-markdown-stream.bench.tsx` | CREATE / EDIT | one test per Dimension |
| M207_003 6.6/6.7 files already on the branch: `FleetFailedOutcome.tsx`, `FleetMessageRow.tsx`, `FleetThought.test.tsx`, `tests/fleet-thread/role-*.test.ts`, `tests/e2e/acceptance/fixtures/{page-event-stream,sse-server}.ts`, `docs/v2/done/M207_003_*.md` | EDIT / CREATE / DELETE | shipped in this PR by Indy's call |
| `~/Projects/docs/changelog.mdx`, `~/Projects/docs/fleets/running.mdx` | EDIT | docs repo, own branch `chore/m207-005-chat-holds-still-changelog` |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC and ORP (the recorder copies, `sse-server.ts`), NLR (stale `build_runner.zig` comment), UFS (queue lane names, error codes), FLL, TSC/TSJ, UIS and DTK (notice variant, markdown components), ERR (only registered codes; `UZ-401` is not in `CODE_MAP`), STR (streaming proven over transport), TCF, TST-NAM, TIM (the 400 ms open delay, the 200 ms fold).
- `dispatch/write_ts_adhere_bun.md` — every `*.ts`/`*.tsx` edit; lint is `make lint-app` (oxlint + `tsc`).
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — the `afd_core` test utility and the five migrations.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md`, `dispatch/write_changelog.md` → `docs/CHANGELOG_VOICE.md` — §8.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI GATE / DESIGN TOKEN GATE | yes — `*.tsx` | `Alert` variant, token-mapped markdown components; no arbitrary values |
| UFS GATE | yes | lane names, error codes and delays as named constants |
| File & Function Length (≤350/≤50/≤70) | yes | `useFleetSteerQueue.ts` separate from `useFleetMessageDelivery.ts` (already 283+ lines) |
| MILESTONE-ID GATE | yes | no `M207`/`§` in source or test names |
| LOGGING / ERROR REGISTRY | yes — routes | `ERROR_CODE.AUTH_401` only |

## Prior-Art / Reference Implementations

- **Reference:** `app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route.ts` — the 401 problem shape the four siblings adopt; `…/events/[eventId]/route.ts` made the same move in `e1e5206b4`.
- **Reference:** assistant-ui's external-store `queue` (`ExternalThreadQueueAdapter`) and `turnAnchor` — the library's own steer-while-running and anchored-thread design; this spec diverges only in dispatching every item at once, because the daemon's admission ledger is the queue.
- **Reference:** `afd_core`'s `test-util` feature (`rustd/crates/afd_core/Cargo.toml`) — the home for shared test support.

## Sections (implementation slices)

### §1 — The steer rides assistant-ui's queue

`FleetThread` passes a `queue` built by `useFleetSteerQueue`. Its `enqueue` and `steer` both hand the message to the existing delivery path (`writers.begin` → `appendOptimistic` → `postSteer`, one operation id per send), so M207_002/003's replay and Resend guarantees hold unchanged. `items` and `steerItems` stay empty, because the daemon admits every steer at once and the pending-sends ledger remains the one authority for an unconfirmed send. `isRunning` is `true` exactly while the newest turn is one this tab sent and its reply runs (`reportsOwnRun`): the top anchor pins whatever turn is last wherever the reader is, so another sender's turn, the same operator's from another tab included, must never be the one it pins. A row this tab painted carries `submittedAtMs` through every frame; the server's row that lands before the 202 counts as this tab's while an optimistic send awaits its acknowledgement. **Implementation default:** dispatch immediately rather than `createMessageQueue`, because that controller holds items until the run ends and the fleet must receive a steer mid-run. The queue returns before the send does, so a refused send's text comes back by the library's own `_returnToDraft` rule, applied in the adapter: when no newer send started, the text returns ahead of anything typed since.

- **Dimension 1.1** — the composer's Send stays enabled while a reply runs → Test `test_send_enabled_while_running` — DONE
- **Dimension 1.2** — a send while running and a send while idle each make one POST with one operation id → Test `test_queue_send_posts_once` — DONE
- **Dimension 1.3** — `isRunning` is true exactly while the newest turn is the viewer's own and its reply runs; another sender's turn, even under the viewer's running reply, never sets it → Test `test_is_running_tracks_reply_rows` — DONE (`tests/fleet-thread/steer-queue.test.ts`, `lib/events/event-summary.test.ts`)
- **Dimension 1.4** — Resend still carries the first operation id → Test `test_failed_send_resend_journey` — DONE (`fleet-resend.spec.ts` green in the full thread e2e runs)

### §2 — The thread anchors the turn at the top

Depends on §1. `ThreadPrimitive.Viewport` takes `turnAnchor="top"` beside `autoScroll`: the anchor holds the viewer's own running turn, and `autoScroll` follows the bottom for a turn with no anchor, such as a webhook's. The library reserve absorbs a Thought's fold and a settling reply. The log is top-aligned, only a settled reply skips layout off screen, and the submit-time `scrollToBottom` goes, since the anchor scrolls on send. `FleetThought`'s lock stays for folds mid-thread.

- **Dimension 2.1** — folding a Thought at the bottom wobbles its trigger ≤ 40 px and returns within 1 px → Test `test_fold_at_bottom_holds_view` — DONE
- **Dimension 2.2** — a reply settling (auto-fold) holds the view the same way → Test `test_settle_holds_view` — DONE
- **Dimension 2.3** — after Send, `scrollTop` never falls more than a 40 px wobble behind its furthest point, and rests there → Test `test_send_scrolls_once` — DONE
- **Dimension 2.4** — Jump to latest still reaches the newest row → Test `test_jump_to_latest_after_anchor` — DONE
- **Dimension 2.5** — a turn another sender makes leaves a reader in the history where they are → Test `test_background_turn_leaves_history_alone` — DONE
- **Dimension 2.6** — reasoning that resumes after the answer began keeps the Thought folded, and its label counts every stretch → Test `test_interleaved_reasoning_folds_once`, `test_resumed_thought_stays_folded`, `test_resumed_reasoning_reopens_the_span` — DONE

### §3 — Reply text through `@assistant-ui/react-markdown` (cut)

Measured before the swap: the library re-parses the whole text part on every change, about 21× the block parser's cost on a long streamed reply (Discovery). Indy cut the swap; `StreamingBlocks` stays.

- **Dimension 3.3** — streaming stays inside M207_004's budget: 0 long tasks, frame p95 ≤ 17.6 ms → Test `test_streaming_reply_costs_no_long_tasks` (existing) — DONE (3 of 3 on a quiet machine, and in every full thread run)

### §4 — Four routes answer the registered 401

The events list, the workspace stream, the fleet events list and the fleet stream answer a signed-out request with `{error, code: ERROR_CODE.AUTH_401}`, the body the event-detail sibling already answers (`e1e5206b4`). No browser reader parses these bodies (backfill reads `res.status`; the streams are `EventSource`), so the family keeps its shape.

- **Dimension 4.1** — each route answers signed-out with 401 and `code: "UZ-AUTH-401"` → Test `test_routes_answer_auth_401` (one case per route) — DONE (`tests/live-routes-auth.test.ts`)
- **Dimension 4.2** — no `"UZ-401"` literal remains under `ui/packages/app`, tests included → Test `test_no_unregistered_auth_code` — DONE (widened to `tests/` after two stale route-test pins failed `make test-unit-all`)

### §5 — One tracing capture for Rust tests

`afd_core::test_util::trace` (feature `test-util`) owns the recorder, the layer and the serial guard. The five copies import it and delete their own. The eight ad-hoc single-purpose layers stay (Out of Scope).

- **Dimension 5.1** — the capture records a scoped event with its fields, and serialises concurrent tests → Test `trace_capture_records_fields`, `trace_capture_serialises_concurrent_tests` — DONE
- **Dimension 5.2** — the five crates' suites pass on the shared capture with no local `Layer` impl → Test `make test-unit-rustd` + Dead Code Sweep grep — DONE

### §6 — The runner's Zig tests run in the unit lane

`test-unit-all` gains `test-unit-runner` (`zig build --build-file build_runner.zig test`), refusing with a named message when `zig version` is not the one `build.zig.zon` pins. No CI job runs it: it runs locally before push, inside `make test-unit-all` (Indy, Discovery). `build_runner.zig`'s comment naming the retired target is corrected.

- **Dimension 6.1** — `make test-unit-all` runs the runner's Zig tests and fails when one fails → Test `test_unit_all_runs_zig` (injected failure: exit 2, `747/751 tests passed (3 skipped, 1 failed)`) — DONE
- **Dimension 6.2** — a wrong Zig version fails fast with the named message → Test `test_runner_zig_version_guard` (`scripts/runner_zig_version_test.py`) — DONE

### §7 — The 6.7 leftovers

The reconnect notice uses `Alert variant="warning"` and rides in the sticky footer's flow above the composer: a reader in the history keeps their rows, and a reader at the bottom keeps the newest row above it, never behind it. The Working, Queued and gone rows get a live review the agent runs and verifies.

- **Dimension 7.1** — the reconnect notice is a warning, announced as `alert`, and the transcript does not move when it appears → Test `test_reconnect_notice_warns_in_place` — DONE
- **Dimension 7.2** — a live review of Working, Queued and gone rows has every finding fixed or listed → Test `agent_live_state_review` (the agent re-runs it and records the evidence in Discovery) — DONE

### §8 — The user docs say what changed

On a `~/Projects/docs` branch `chore/m207-005-chat-holds-still-changelog` off `main`: one changelog `<Update>` for M207_003–005's user-visible changes (sends settle, Resend, catching up, the failure mark, quiet Thoughts, the anchored thread), and `fleets/running.mdx` revised for catching up and the anchored turn. A docs Pull Request is opened; Indy merges it. PR #206 (M207_001/002 docs) merges first, or this branch rebases on it.

- **Dimension 8.1** — the docs branch passes the docs repo's own checks and its PR is open → Test `docs_pr_open_and_green` — DONE (agentsfleet/docs#207, stacked on #206: Greptile pass, gitleaks pass)

## Interfaces

```
ExternalThreadQueueAdapter (useFleetSteerQueue):
  items = [], steerItems = []          # the daemon is the queue
  enqueue(m) = steer(m) = deliver(m)   # one POST, one operation id
isRunning = the newest event is in flight and its actor isSteerBy the viewer
  move/edit/remove(id)                 # unreachable with empty lanes; refuse an unknown id
401 from the four routes: application/json {"error":"Unauthorized","code":"UZ-AUTH-401"}
afd_core::test_util::trace (feature "test-util"): Capture::install() -> guard; guard.events() -> Vec<CapturedEvent>
make test-unit-runner: zig build --build-file build_runner.zig test   (part of test-unit-all)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Double dispatch | both `onNew` and the queue fire | with `queue` set the runtime never calls `onNew` for a send; test asserts one POST |
| Send fails mid-run | `postSteer` refused or unanswered | the existing unsent notice and Resend, same as idle |
| Stale running flag | a reply row never settles | `isRunning` follows row status; the stall watcher (M207_004) settles the row, releasing the anchor |
| Reserve after backfill | catching-up inserts history above | the reserve is measured from the anchor turn; the view holds (Test 2.2) |
| Signed-out stream | 401 on an `EventSource` | the error path already recovers to sign-in; the code is now registered |
| Zig missing | local machine without 0.16.0 | `test-unit-runner` refuses with the install hint |
| Docs conflict | #206 and this branch both touch `changelog.mdx` | rebase after #206 merges; entries stay dated |

## Invariants

1. One POST and one operation id per logical send — the queue adapter's only effect is the delivery path's `deliver`; Test 1.2 and the resend journeys.
2. `isRunning` is derived, never set — one selector over reply rows; Test 1.3.
3. Only registered error codes leave a route — `ERROR_CODE` constants; the rubric grep for `"UZ-401"` returns 0.
4. Raw HTML is never parsed — no `rehype-raw` in `package.json`.
5. One tracing capture — the Dead Code Sweep grep returns 0 local `Layer` impls in the five files.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_send_enabled_while_running` | a running reply row → Send not disabled |
| 1.2 | unit | `test_queue_send_posts_once` | send idle and send running → one `postSteer` each, distinct operation ids, no `onNew` |
| 1.3 | unit | `test_is_running_tracks_reply_rows` | own running row last → true; all settled → false; a teammate's or the API's running row last, even over the viewer's → false; empty → false |
| 1.4 | e2e | `test_failed_send_resend_journey` | lost answer → Resend carries the first operation id; `test_reload_recovers_unconfirmed_send` stays green beside it |
| 2.1 | e2e | `test_fold_at_bottom_holds_view` | fold at bottom → trigger wobble ≤ 40 px, back within 1 px |
| 2.2 | e2e | `test_settle_holds_view` | answer lands after a 1 s thought → wobble ≤ 40 px, back within 1 px |
| 2.3 | e2e | `test_send_scrolls_once` | Send → `scrollTop` never > 40 px behind its furthest point, resting there |
| 2.4 | e2e | `test_jump_to_latest_after_anchor` | scrolled up, new row → Jump to latest reaches it |
| 2.5 | e2e | `test_background_turn_leaves_history_alone` | a teammate's turn while turn 1 is read → turn 1 Δ ≤ 1 px in the thread, Jump to latest shown |
| 2.6 | e2e + unit | `test_interleaved_reasoning_folds_once` | reason → answer → reason → answer → the Thought's open state goes true, false and never true again; the span ends at the last hand-off |
| 3.3 | e2e | `test_streaming_reply_costs_no_long_tasks` | 0 long tasks, p95 ≤ 17.6 ms |
| 4.1 | unit | `test_routes_answer_auth_401` | four routes signed-out → 401, `UZ-AUTH-401` |
| 4.2 | unit | `test_no_unregistered_auth_code` | source scan → 0 `"UZ-401"` |
| 5.1 | unit | `trace_capture_records_fields` | one event with two fields → captured once, fields intact |
| 5.2 | unit | `make test-unit-rustd` | five suites green on the shared capture |
| 6.1 | unit | `test_unit_all_runs_zig` | failing Zig test → `make test-unit-all` exits non-zero |
| 6.2 | unit | `test_runner_zig_version_guard` | `zig` 0.15 on PATH → named refusal |
| 7.1 | e2e | `test_reconnect_notice_warns_in_place` | stream drops over a reader in history → warning `alert`, on-screen Δ ≤ 1 px; at the bottom the newest row ends above the notice |
| 7.2 | agent | `agent_live_state_review` | the agent re-runs the live review; every finding fixed or listed in Discovery |
| 8.1 | manual | `docs_pr_open_and_green` | docs PR open, its checks green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The view holds through folds, settles and Send (§1, §2) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys tests/e2e/acceptance/fleet-thread.spec.ts tests/e2e/acceptance/fleet-thread-anchor.spec.ts tests/e2e/acceptance/fleet-reply-parts.spec.ts tests/e2e/acceptance/fleet-resend.spec.ts` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Only registered 401s (§4) | `git grep -n '"UZ-401"' -- ui/packages/app ':!ui/packages/app/tests/live-routes-auth.test.ts'` (the scan test names what it looks for) | 0 matches | P0 | |
| R4 | One tracing capture (§5) | `git grep -lnE 'impl<S[^>]*> Layer<S>' -- rustd/crates/afd_dragonfly/tests rustd/crates/afd_events/tests rustd/crates/afd_fleet rustd/crates/agentsfleetd/src/supervisor ':!rustd/crates/afd_fleet/tests/integration_lease_gates/tenant.rs'` (the lease-gate layer is Out of Scope; at the baseline this lists six files) | 0 matches | P1 | |
| R5 | Zig tests in the unit lane (§6) | `make -n test-unit-all \| grep -c 'build_runner.zig test'` | 1 | P1 | |
| R6 | Docs PR open (§8) | `gh pr list -R agentsfleet/docs --head chore/m207-005-chat-holds-still-changelog --json state -q '.[].state'` | `OPEN` | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository-command rows point at the final `orly gate pr` in Session Notes. A P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/tests/e2e/acceptance/fixtures/sse-server.ts` | `test ! -f ui/packages/app/tests/e2e/acceptance/fixtures/sse-server.ts` |
| the four recorder copies (`afd_dragonfly`, `afd_events` `tests/support/recorder.rs`; `afd_fleet` `tests/support/fleet_log.rs`, `src/lease/test_log.rs`) | `git ls-files rustd/crates \| grep -cE 'support/recorder.rs\|fleet_log.rs\|lease/test_log.rs'` → 0 |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `scheduledSseServer` | `git grep -nw scheduledSseServer -- ui` | 0 matches |
| `test-unit-agentsfleet-runner` | `git grep -n test-unit-agentsfleet-runner -- build_runner.zig make` | 0 matches |

## Out of Scope

- The eight single-purpose tracing layers (`afd_runner`, `afd_admission`, `afd_api` ×2, `afd_outbound`, `afd_fleet` lease gates, `afd_cron`, `agentsfleetd::logs`) — they assert different shapes; folding them is a separate call.
- `Handles::claim` over the 70-line cap, the spurious gap on unsubscribe/resubscribe, and streaming markdown re-parsing after a discarded render — M207_004's listed items; the third stays listed, since the library's renderer re-parses whole (§3, cut).
- The fleet's model emitting an unknown `<nc_choices>` block — model output, shown as text as it should be.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reading a long thought at the bottom of the thread clicks its chevron, and the thought folds into the line under their cursor while nothing else moves.
2. **Preserved user behaviour** — sending mid-run, Resend, reload recovery, Copy, Jump to latest, and the Thought chip's live label.
3. **Optimal-way check** — the library's anchor-and-reserve is the direct fix; the gap was the composer's `isRunning` workaround, which the library's steer lane removes.
4. **Rebuild-vs-iterate** — iterate: the runtime keeps its store and delivery path; only the send's entry point and the viewport mode change.
5. **What we build** — a queue adapter, a truthful running flag, one viewport prop, a markdown swap, four route fixes, one test utility, one make target and CI job, a notice variant, and docs.
6. **What we do NOT build** — a client-side queue that holds messages (the daemon queues), a custom scroll-anchoring system, a Markdown fork.
7. **Fit with existing features** — compounds with M207_003's settle guarantees and M207_004's stream budget; must not destabilise Resend's single operation id.
8. **Surface order** — UI-first: the chat is a dashboard surface; the CLI has no chat.
9. **Dashboard restraint** — no queue UI appears: the lanes stay empty, so no pending-queue chips render.
10. **Confused-user next step** — `fleets/running.mdx` explains the anchored turn and catching up; the reconnect notice keeps its Retry now.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** eight Sections in dependency order; §2 needs §1, the rest are independent.
- **Alternatives considered:** a hand-rolled bottom slack under the last turn (rejected: reimplements the library's reserve and its restore-on-shrink handler); instant folds with no animation (rejected: trades the drag for a jump).
- **Patch-vs-refactor verdict:** this is a **refactor** of the send entry point and viewport mode, because the patch (holding `isRunning` false) is what blocks the library's fix.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, source read at `f892813ab`: the send routing (`external-store-thread-runtime-core.js:407-411`), `composerSendDisabled` (`primitive-predicates.js:7`), the anchor gate (`topAnchorTurn.js:17`), the four `"UZ-401"` routes and the five recorders (file:line in this session's fact sweep). No `catching_up` docs page exists; `fleets/running.mdx` is the chat page. PR #718's body claims drafts "in this PR's Session notes 1", and none exist, so §8 drafts from source. Security-boundary own-PR rule (`dispatch/write_spec.md`): Indy chose to keep this work in the current PR:
> Indy (2026-09-30): "New M207_005, this PR (Recommended)" — context: AskUserQuestion on where the steer/queue move, markdown swap and deferred items live; overrides the security-boundary own-PR rule for the steer path and the 401 routes.
> Indy (2026-09-30): "Run provision-env, Publish ~/Projects/docs, orly P32 rule" — context: which carried-over items the agent does; authorises the `~/Projects/docs` write for §8.
- **Live state review (Sep 30, 2026, local app against DEV's API, for 7.2)** — Queued → Working → Thinking 1.6 s → Thought + answer → Completed, observed on two back-to-back sends. FINDING-003, fixed: a reply's outcome sentence ("Completed.", the gone line) wore the reply's foreground ink; it now takes the integration tick's muted mono (`FLEET_OUTCOME_CLASS`). FINDING-004, listed: the footer says "Still working." while both rows still say "Queued…", because a row turns Working only on its first frame; aligning them is a status-semantics call for Indy. The gone row is reachable only on a 404/410 event, so it was checked in `role-reasoning.test.ts`, not live. The Working spinner is `aria-hidden`.
- **§2 probes (Sep 30, 2026, headless e2e)** — Send's jump was the anchor measuring a skipped operator row at its 208 px stand-in (68 px real), plus our submit-time `scrollToBottom`; both gone. Admission renames the turn's rows and the library re-pins a frame late: one 36 px dip that returns, held to the fold bar. The anchor scrolls to any new running user turn wherever the reader is (`mountTopAnchorReserve.js` `apply`), measured at 1,938 px for a teammate's turn, so `isRunning` counts the viewer's own turns only, restoring `main`'s rule that background replies leave history alone. FINDING-010, listed: the header's arrival cue collapses after connect and slides the thread 37 px, page chrome that predates this spec.
> Indy (2026-09-30): "Yes for test.yml edit." — context: approval of the §6 CI job; superseded below.
> Indy (2026-09-30): "Well dont add the test-unit-runner in test.yml job" and "you can run it locally prior to push" — §6 has no CI job.
> Indy (2026-09-30): "keep it as is or make a better general human friendly name" — context: FINDING-004; kept as is. Indy also confirmed a fold may wobble ≤ 40 px and must return, and that short threads start at the top of the panel.
> Indy (2026-09-30): "7.2 make it agent-verified, i donot have time to eyeball. Only add that if you verified it."
- **Agent live review (Sep 30, 2026, 7.2)** — re-run on `next dev` against DEV's API, frames through the page's `EventSource` plus one real Send; report `~/.gstack/projects/agentsfleet-agentsfleet/designs/design-audit-20260930/design-audit-localhost.md`, screenshots `live2-*.png`. Working (spinner `aria-hidden`), Thinking (opens after 400 ms), Thought + streaming answer, Completed, failed (destructive mark), Queued, the gone row (reached live: a failed run's detail read 404'd), and the offline warning (`alert`, 8.4:1 on its fill) all render as specified. Fixed: 001–003, 005–007, 012 (a teammate's turn yanking the reader). Listed: 004 (Indy: kept), 008 (model output), 010 (arrival cue slides the thread 37 px, pre-existing chrome), 011 (a refused run's completion carries no reply — `pull/refuse.rs:105` — so it flashes "Loading final reply" under the failure for one detail read).
- **Interleaved reasoning (Sep 30, 2026)** — Indy, eyeballing localhost: "the thought is still happening and the answer is still happening, hence when the first thought is done i see 1 sec, with text and folded with an answer then again the thought expands with the seconds increasing and folds" — only on short prompts (`hey`), not on a 1000-word story. Cause: each delta sets `thinking` to its kind, so resumed reasoning re-ran the Thought's 400 ms auto-open, and the span closed at the first answer only. Fix: auto-open only before any answer text; resumed reasoning reopens the span. Dimension 2.6.
- **§3 measured, then cut (Sep 30, 2026)** — `@assistant-ui/react-markdown` 0.14.17 re-parses the whole text part on every change (`MarkdownText.js`: `MarkdownRenderer` is `memo`'d on the full text; it memoises components, not blocks). On the 20 KB, 400-flush corpus: block path 376 ms, library 8,114 ms (memoised components) / 8,133 ms (default), whole re-parse 8,366 ms. §3 as written traded the tail-only parse for a ~21× slower stream, so the swap was not made.
> Indy (2026-09-30): "i think on the markdown i asked to drop 3 -  since you said the assitant-ui is not performant." — §3's swap is cut; the block parser stays.
- **Review (Sep 30, 2026)** — agent pass plus one adversarial subagent over `17535ad79..HEAD`. Fixed: the offline notice hid the newest reply from a reader at the bottom (moved into the footer's flow); `isRunning` counted any own running reply, so a teammate's turn landing under it was pinned (now the newest event only); `isSteerBy`'s continuation branch matched `continuation:steer:…`, which the daemon never writes (`afd_approval/src/inbox/resolve.rs:228`); two route tests pinned `"UZ-401"`. Kept: the steer queue drops a non-refusal rejection silently, as assistant-ui's own send did before §1, since surfacing it needs a new lint suppression. The runner's Zig tests run in no CI job, by Indy's call above.
- **Metrics review** — no analytics/funnel playbook update required: no event is added, renamed or removed.
- **Skill-chain outcomes** — `/orly-write-unit-test` was not invoked as a skill: each Dimension got its test in the Section's commit and was made red before it was trusted (TCF), and the app coverage gate reads 100%. `/orly-write-integration-test`: N/A — the Rust change is test support, and the app changes cross the browser boundary the e2e lane proves. REVIEW: agent pass plus an adversarial subagent (Review bullet above).
- **Deferrals** — none.
