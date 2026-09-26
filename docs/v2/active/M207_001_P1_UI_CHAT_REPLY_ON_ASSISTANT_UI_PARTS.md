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

# M207_001: The fleet chat reply renders as assistant-ui parts — a timed Thought chip, live tool rows, a braille wait state — and a failed send returns to the composer

**Prototype:** v2.0.0
**Milestone:** M207
**Workstream:** 001
**Date:** Sep 26, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing chat; a long reasoning pass reads as a hung fleet, and a failed send leaves a dead row
**Categories:** UI
**Batch:** B1 — sole workstream; rides PR #717 beside its composer-layout commits
**Branch:** fix/chat-composer-scroll-clip
**Baseline revision:** dbd2f32f396c82e7fe0b236d337efba084b133de
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 26, 2026) from Kishore's in-session decisions; every assistant-ui claim read from the `@assistant-ui/react@0.15.22` tag (`bun.lock` resolves react 0.15.22, core 0.3.21)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Live activity (frame kinds, chunk sequencing, the reply decoder); `docs/architecture/user_flow.md` §chat surface

---

## Overview

**Goal (testable):** A streamed fleet reply renders through `MessagePrimitive.GroupedParts` — "⠧ Thinking · 3.2s" live, "Thought · 8.5s" folded, tool rows timed by `useToolCallElapsed`, a braille verb while waiting — and a failed send leaves the thread with its text back in the composer, with zero long tasks while it streams.
**Problem:** During a long reasoning pass the operator sees an untimed "Thinking…" accordion and a paw-print "Working…", so a slow model reads as a hung one. Tools render from a private custom-bag list the library cannot see. A send that fails leaves a red "not sent" row in the thread and a Retry that re-posts from a stale copy.
**Solution summary:** Frontend only. The reply becomes a real assistant message whose content is assistant-ui reasoning, tool-call and text parts with per-part status and tool timing; rendering moves to `GroupedParts` with the kit's composition, restyled with design-system primitives. The row reducer stamps the only values the library does not carry (reasoning span, tool start). A failed POST discards its optimistic row and rejects with assistant-ui's `MessageNotSentError`, so the composer takes the text back itself; Resend is the composer's own Send.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(app): seat the chat composer and render replies as assistant-ui parts
- **Intent (one sentence):** Operators watch a fleet think and work with live clocks, see afterwards how long each step took, and resend a failed message from the composer — on library primitives, without the chat getting slower.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/components/domain/useFleetThreadEntries.ts` — `expandEntry` and `convertEntry`: where a trigger splits from its `:reply` message and where parts will be built.
2. `ui/packages/app/components/domain/FleetReplyBody.tsx` — the reply surface being rebuilt; keeps `useFirstVisiblePaint` and `ReplyActions`.
3. `ui/packages/app/lib/streaming/fleet-stream-optimistic.ts` — why the registry, not `useOptimistic`, owns the pending row (the live frame carries no body; the row holds the text).
4. https://github.com/assistant-ui/assistant-ui/tree/%40assistant-ui/react%400.15.22/packages/ui/src/components/react/assistant-ui/elements — `thread.aui.tsx` (GroupedParts switch), `reasoning.aui.tsx` (live = message running and part running, `useScrollLock`), `tool-group.aui.tsx`. Local copy: `git -C ~/Projects/oss/js/assistant-ui archive @assistant-ui/react@0.15.22`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/streaming/{fleet-stream-row,fleet-stream-frames,fleet-stream-reply-frames,fleet-stream-registry}.ts` | EDIT | Tool start and reasoning span stamped once with an injected clock; `FAILED` status and `markOptimisticFailed` leave |
| `ui/packages/app/components/domain/fleetReplyMessage.ts` | CREATE | Pure row → assistant message (status, reasoning/tool-call/text parts) |
| `ui/packages/app/components/domain/{useFleetThreadEntries,useFleetEventStream}.ts` | EDIT | Operator turns always split; reply rows convert through `fleetReplyMessage`; custom bag drops reasoning/thinking/tools |
| `ui/packages/app/components/domain/FleetReplyBody.tsx`; `FleetThought.tsx`, `useFirstVisiblePaint.ts` | EDIT; CREATE | `GroupedParts` switch and outcome floor; the `group-reasoning` chip, leaf clock, sentence ticker |
| `ui/packages/app/components/domain/FleetToolCalls.tsx` | EDIT | `tool-call` part row on `useToolCallElapsed`; custom-bag reader leaves |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx`, `fleetMessageReaders.ts`, `fleetMessageStatus.ts`, `FleetMessageRow.tsx` | EDIT | Failed badge, orphaned row `annotation` and moved readers leave; status constants alias `AGENTSFLEET_EVENT_STATUS`; reasoning-span readers |
| `ui/packages/app/components/domain/{FleetThread,FleetThreadViewport,SteerComposer}.tsx`, `useFleetMessageDelivery.ts`, `useFleetDeliveryFailure.ts` | EDIT | Claude.ai resend; `onRetry` chain leaves |
| `ui/packages/app/components/layout/loading-verbs.ts`; `ui/packages/app/lib/utils.ts`; `ui/packages/app/lib/api/errors.ts` | EDIT | `loadingVerbFor(key)`; `formatSeconds` on `Intl.NumberFormat`; `HTTP_STATUS_UNAUTHORIZED` beside its siblings |
| `ui/packages/design-system/src/design-system/{BrailleSpinner,index,DashboardPanel,DashboardPrimitives.test}.ts(x)`, `ui/packages/design-system/src/{index.ts,tokens.css}` | CREATE / EDIT | Decorative CSS-only glyph; exports; `braille-spin` keyframes and the `[data-settled]` rule beside `wake-pulse`; orphaned `DashboardPanelFooter` leaves |
| `ui/packages/app/components/domain/{fleetReplyMessage.test.ts,FleetThought.test.tsx,FleetReplyBody.test.tsx}`, `ui/packages/app/lib/{streaming/fleet-stream-reply-frames,utils}.test.ts`, `ui/packages/app/components/layout/loading-verbs.test.ts`, `ui/packages/design-system/src/design-system/BrailleSpinner.test.tsx`, `ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts` | CREATE | Unit proofs |
| `ui/packages/app/tests/fleet-thread/{harness.ts,role-reasoning.test.ts,role-rows.test.ts,role-turns.test.ts,steer-submission.test.ts,malformed-metadata.test.ts}`, `ui/packages/design-system/src/tokens.css.test.ts`, `ui/packages/app/tests/fleet-tool-calls.test.tsx`, `ui/packages/app/lib/streaming/{fleet-stream-registry-optimistic,fleet-stream-registry-backfill,fleet-stream-frames,fleet-stream-frames.live,fleet-stream-frames.tools}.test.ts`, `ui/packages/app/lib/events/run-summary.test.ts`, `ui/packages/app/tests/{use-fleet-event-stream,fleets-install-entry-gate,fleets-install-flow,fleets-install-states}.test.ts`, `ui/packages/app/components/domain/{fleetFailureCopy.test.ts,FleetMessageRow.test.tsx}`, `ui/packages/app/components/domain/{SteerComposer.test.tsx,FleetThreadViewport.test.tsx,useFleetDeliveryFailure.test.tsx,fleetMessageReaders.test.ts}` | EDIT | Amended to the parts model and resend flow |
| `ui/packages/app/tests/e2e/acceptance/{fleet-reply-parts.spec,fleet-resend.spec,fixtures/sse-server}.ts`; `{fleet-thread.spec,fixtures/sse}.ts` | CREATE; EDIT | Frame-by-frame reply stream with the long-task and frame probe; live parts and resend journeys |
| `docs/architecture/user_flow.md` | EDIT | Chat surface line: Thought chip, tool rows, resend |
| PR #717's composer-layout files (`FleetThreadViewport.tsx`, `SteerComposer.tsx`, `ChatView.tsx`, `page.tsx`, their tests) | EDIT | Already committed on the branch by Kishore's call; not re-specified |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (accordion, paw indicator, failed badge, `markOptimisticFailed`, retry chain, custom-bag tool/reasoning readers leave in the same PR), ORP (every removed symbol grepped to zero across tests and docs), UFS (labels, glyph frames, group keys, tick interval are named constants), TIM (tick interval and spinner frame duration explicit), TCF (each pin made red by deleting its subject), PJV (timestamps are numbers carried by value).
- `dispatch/write_ts_adhere_bun.md` — TS FILE SHAPE DECISION for the three new modules; UI GATE; DESIGN TOKEN GATE.
- `docs/DESIGN_SYSTEM.md` §Motion — animate only while live; reduced motion leaves a static, readable frame; no gradient shimmer.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TS FILE SHAPE DECISION | yes — `fleetReplyMessage.ts`, `FleetThought.tsx`, `BrailleSpinner.tsx` | functions-modules: one pure converter; one chip plus its leaf clock; one stateless glyph |
| UI GATE | yes | Disclosure stays the design-system `Accordion`; `Spinner` is a `role=status` live region and cannot sit inside the chip's trigger button, so the glyph is a decorative primitive beside `WakePulse` (same precedent) |
| DESIGN TOKEN GATE | yes | Keyframes and the `[data-settled]` rule live in `tokens.css`, pinned by `tokens.css.test.ts`; consumers set attributes only |
| UFS GATE | yes | Constants for labels, frames, group keys, 100 ms tick |
| File & Function Length (≤350/≤50/≤70) | yes | `FleetReplyBody.tsx` (229) sheds the accordion and indicator into `FleetThought.tsx` |

## Prior-Art / Reference Implementations

**Library-first ledger** — each capability, the primitive that owns it, and the only two values we compute:

| Capability | Owner |
|---|---|
| Parts, grouping, per-group live status | `MessagePrimitive.GroupedParts` + `groupPartByType` (core `react/utils/groupParts.ts`); group `status` running when any member runs |
| Reasoning live/folded | reasoning part `status` (a part-level `MessagePartStreamStatus` is honoured while the message runs — core `utils/normalizePartStatus.ts`) |
| Tool status and clock | tool-call `result` (undefined = running) and `timing` → `useToolCallElapsed` |
| Wait state | GroupedParts `indicator` part (default `"no-text"`) |
| Scroll stability on fold | `useScrollLock` (kit `reasoning.aui.tsx`) |
| Resend and draft restore | `MessageNotSentError` from `onNew` — the composer returns the draft, ahead of anything typed since; `ComposerPrimitive.Send asChild`; `composer().setText` only on a remount, which the library cannot see |
| Streaming markdown priority | React 19 `useDeferredValue` |
| Latest sentence | `Intl.Segmenter` (`granularity: "sentence"`) |
| Duration text | `Intl.NumberFormat` (`style: "unit"`, `unit: "second"`, narrow) — `Intl.DurationFormat` prints `8s 500ms`, not `8.5s` |
| Off-screen layout skip | CSS `content-visibility: auto` (kit `thread.aui.tsx` message root) |
| Render-cost proof | a parent render counter — `<Profiler onRender>` fires for any commit in its subtree, the leaf's own tick included |
| **Computed here:** reasoning span, tool start | `ReasoningTrigger` takes `duration` from its caller; `MessageTiming` has no reasoning span; tool frames carry no start instant |

- **Rejected:** `useOptimistic` for the pending row — the row must outlive a navigation inside the registry's idle window and carries the only copy of the operator's text for the server-row graft (`fleet-stream-optimistic.ts`). `useStreamingTiming` — it keys off a thread-wide `isRunning` this runtime never sets (`FleetThread.tsx` explains why). Deprecated paths — `MessagePrimitive.Parts` group slots, the `components` prop, `ChainOfThoughtPrimitive` (legacy per `guides/chain-of-thought.mdx`).
- **Reference:** opencode's terminal chat ("⠧ Thinking", "Thought · 8.5s"), from Kishore's screenshots — the visual; `WakePulse` + `wake-pulse` — the primitive-plus-keyframe pattern.

## Sections (implementation slices)

### §1 — A failed send returns to the composer

Claude.ai's behaviour, on the library's contract. On `ok:false` or a thrown POST, `discardOptimistic(tempId)` removes the row, the per-fleet failure record keeps `{ text, kind }`, and `onNew` rejects with `MessageNotSentError`, which the composer answers by returning the draft ahead of anything typed since. A remount restores from the record into an empty composer, once. The notice reads "Message not sent."; **Resend** is `ComposerPrimitive.Send asChild`, shown while the composer holds the refused text. A 401 keeps the Sign in notice. `FleetMessageRow`'s `failed` tone stays: fleet-error replies use it. The POST is never replayed automatically: it carries no idempotency key (`lib/api/retry.ts`). **Implementation default:** the record clears on the next submission, because a new send supersedes the notice.

- **Dimension 1.1** — A failed POST removes its row and restores the text into an empty composer under "Message not sent." → Test `test_failed_send_leaves_thread_and_restores_draft` — DONE (steer-submission 10/10 through the real composer; five mutations each turn a pin red)
- **Dimension 1.2** — Resend sends the restored text once (one new pending row, notice cleared); nothing re-sends without that click → Test `test_resend_submits_restored_text_once` — DONE (steer-submission 10/10 through the real composer; five mutations each turn a pin red)
- **Dimension 1.3** — A draft typed while the send was out is kept, with the refused text returned ahead of it; a remount restores into an empty composer → Test `test_failure_restore_respects_existing_draft` — DONE (steer-submission 10/10 through the real composer; five mutations each turn a pin red)
- **Dimension 1.4** — A 401 restores the text and shows Sign in, not Resend → Test `test_session_failure_keeps_sign_in` — DONE (steer-submission 10/10 through the real composer; five mutations each turn a pin red)

### §2 — The reply is an assistant-ui message

An operator turn always splits into its trigger and a `:reply` assistant message; an integration turn splits when a reply, reasoning or tool exists (a tools-only turn is new: today it stays unsplit and its tools render under the trigger). The reply's `status` is running while the event is optimistic or received, complete otherwise. Content, in order: a reasoning part with its own status (running while `thinking`), one tool-call part per `FleetToolCall`, a text part — empty pieces omitted. **Implementation default:** tool `toolCallId` is `<eventId>:tool:<index>` (append-only list, stable), `result` is `null` once done because the wire carries no output, and `timing.completedAt` is `startedAt + ms`.

- **Dimension 2.1** — An in-flight operator turn yields a running `:reply` message with no parts — the indicator renders without a remount at the first word, and the thread never reads as running, so the composer stays enabled → Test `test_inflight_turn_has_running_reply` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 2.2** — A row with reasoning, two tools and an answer converts to ordered parts with the pinned statuses, identifiers and timing → Test `test_reply_parts_from_row` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 2.3** — A tool's first frame stamps `startedAtMs` from the injected clock; completion keeps it; a repeat call after completion gets its own entry → Test `test_tool_start_stamped_from_first_frame` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 2.4** — The reasoning span is stamped once: start on the first reasoning text; end on the first answer text, completion or recovery → Test `test_reasoning_span_stamped_once` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 2.5** — Non-number stamps or a missing tool list read as absent without throwing → Test `test_malformed_reply_metadata_reads_absent` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §3 — Render through GroupedParts, cheaply

`FleetReply` renders `MessagePrimitive.GroupedParts` with a module-constant `groupPartByType({ reasoning: ["group-reasoning"], "tool-call": ["group-tool"] })` and one `switch (part.type)`: `group-reasoning` → Thought chip, `group-tool` → a list named "Tool calls", `text` → markdown, `tool-call` → tool row, `reasoning` → its text, `indicator` → wait state. The text part renders `FleetMarkdown` over `useDeferredValue(text)`, so parsing a growing answer yields to input and scroll. Settled message roots carry `data-settled="true"`, which tokens.css gives `content-visibility: auto`; the streaming row does not.

- **Dimension 3.1** — Each part type renders through the switch, and no deprecated or custom-bag path remains → Test `test_reply_renders_through_grouped_parts` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 3.2** — The text part renders deferred markdown with the stream cursor while running; once settled it shows Copy and the row carries `data-settled` → Test `test_text_part_and_settled_row` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 3.3** — A settled reply with no text part shows its outcome sentence; a fleet error keeps the failed tone → Test `test_outcome_floor_without_text_part` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §4 — The Thought chip

Inside `group-reasoning`: live (group running) the trigger reads `<spinner> Thinking · 3.2s` plus the latest sentence on one truncated line, open; folded it reads `Thought · 8.5s`, closed unless the operator opened it, with `useScrollLock` holding the viewport through the fold. Durations print with `formatSeconds` (§4.6). **Implementation default:** the clock is a leaf component with its own state and a 100 ms interval — the `useToolCallElapsed` pattern at the tenths the label shows — so a tick re-renders the leaf only. The sentence comes from `Intl.Segmenter` over the last 400 characters.

- **Dimension 4.1** — A running reasoning group shows spinner, "Thinking", a ticking clock and the latest sentence → Test `test_thought_chip_live` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 4.2** — On completion it folds to "Thought · 8.5s", closed, content unmounted until opened; an operator-opened chip stays open → Test `test_thought_chip_folds_with_duration` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 4.3** — A tick leaves the parent's render count unchanged; the interval clears on fold and unmount → Test `test_thought_clock_ticks_only_the_leaf` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 4.4** — A remount mid-thought resumes from the row's start stamp; no stamp folds to "Thought" without a duration → Test `test_thought_clock_resumes_from_row_stamp` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 4.5** — The sentence ticker handles empty, boundary-free, trailing-space and non-ASCII text → Test `test_latest_sentence_edges` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 4.6** — `formatSeconds` is one module-constant `Intl.NumberFormat` (unit `second`, narrow, exactly one fraction digit), so a ticking clock keeps its width; `formatMs` keeps its table callers unchanged → Test `test_format_seconds_fixed_width` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §5 — Tool rows on part timing

A `tool-call` part renders its name, a running or done glyph, and `useToolCallElapsed()` through `formatSeconds`. The existing glyph vocabulary (`◐`/`✓`) stays.

- **Dimension 5.1** — A running tool ticks from its start, a done tool shows `✓` and its final duration, adjacent tools share one "Tool calls" list, and a tool left unfinished on a settled reply shows no clock → Test `test_tool_row_reads_part_timing` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §6 — Braille spinner primitive

Ten frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` in one column moved by `translateY` with `steps(10)` over 800 ms (80 ms a frame) — compositor-only, no JavaScript. Reduced motion shows a static `⠶`. `aria-hidden`; the adjacent label carries meaning.

- **Dimension 6.1** — Renders the frames in order inside an `aria-hidden` element carrying the `braille-spin` animation; under reduced motion the column hides and `⠶` shows → Test `test_braille_spinner_frames_and_animation` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §7 — The wait state

The `indicator` part renders `<output>` named "Working" (or "Queued" for a queued turn) with the spinner and `loadingVerbFor(eventId)…` from `LOADING_VERBS`, hashed so a verb never changes across renders.

- **Dimension 7.1** — The indicator keeps `role=status` named Working/Queued and shows the spinner and verb → Test `test_indicator_verb_and_accessible_name` — DONE (app 3,038 tests green; a mutation of its subject turns it red)
- **Dimension 7.2** — The same key always yields the same verb; 50 keys yield at least 5 distinct verbs → Test `test_loading_verb_for_is_stable` — DONE (app 3,038 tests green; a mutation of its subject turns it red)

### §8 — Proof on the real app

Against DEV through the acceptance config: a routed stream delivers reasoning, a tool pair and an answer; a Server Action POST is aborted to drive the resend journey; a `PerformanceObserver` counts long tasks while the chip animates and the reply streams.

- **Dimension 8.1** — Live "Thinking", a timed tool row, then "Thought ·" folded above the answer → Test `test_stream_reply_parts_live_then_folded` — DONE (acceptance on DEV, passed)
- **Dimension 8.2** — An aborted send leaves the thread, its text sits in the composer, Resend lands once → Test `test_failed_send_resend_journey` — DONE (acceptance on DEV, passed: refused once, delivered once)
- **Dimension 8.3** — Zero long tasks while the reply streams with the chip live, and frame p95 within the pre-change budget (17.6 ms; the pre-parts tree measured 16.8 ms on `5f236cbcc`) → Test `test_streaming_reply_costs_no_long_tasks` — DONE (acceptance on DEV: 0 long tasks, 336 frames, p95 16.8 ms)

## Interfaces

```
Reply message (id "<eventId>:reply", role "assistant"):
  status: { type: "running" } | { type: "complete" }
  content: [ reasoning { text, status: running|complete }?, tool-call { toolCallId, toolName,
             args: {}, result?: null, timing: { startedAt, completedAt? } }*, text { text }? ]
  metadata.custom: actor, status, queued, submittedAtMs, outcome, failureLabel, failureDetail,
                   replyRecovering, reasoningStartedAtMs?, reasoningEndedAtMs?
FleetToolCall += startedAtMs: number;  FleetEvent += reasoningStartedAtMs?, reasoningEndedAtMs?: number
applyLiveFrame(prev, frame, nowMs = Date.now()); applyReplyDelta(prev, eventId, delta, nowMs = Date.now())
FailedDelivery = { text: string; kind: "send" | "session" };  loadingVerbFor(key): LoadingVerb;  formatSeconds(ms): string
<BrailleSpinner className? />   // design-system, aria-hidden, CSS-only
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Draft present on failure | Operator typed while the POST failed | assistant-ui returns the refused text ahead of the draft; nothing typed is lost |
| Two queued sends fail | Serialized POSTs both refused | The composer returns the latest send's text (an earlier send's draft return is superseded by the later send); the notice names the refusal |
| Remount mid-thought, or no stamp | Navigation inside the registry idle window; a reply recovered from detail | Clock resumes from the row stamp; without one it folds to "Thought" with no duration |
| Answer in the reasoning's first delta | Both kinds in one batch | Start and end at one clock read; duration `0.0s` |
| Tool never completes | Stream ends mid-tool | Settled reply → part not running → no clock |
| Tab hidden, or reduced motion | Timers throttle; `prefers-reduced-motion` | Next tick shows true elapsed, no catch-up burst; static `⠶` while the clock text still updates |
| Huge reasoning | Tens of kilobytes | Ticker scans the tail; closed content unmounted |

## Invariants

1. A clock tick never re-renders the reply — the clock owns its state; `test_thought_clock_ticks_only_the_leaf` counts parent renders.
2. The spinner runs no JavaScript per frame — stateless component, CSS keyframes; `test_braille_spinner_frames_and_animation`.
3. Stamps are written once — the reducers stamp only an absent field; `test_reasoning_span_stamped_once`, `test_tool_start_stamped_from_first_frame`.
4. A failed POST is never auto-replayed — the only send path is `onNew`; `test_resend_submits_restored_text_once`.
5. No deprecated or custom-bag render path — rubric S8 greps to zero.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | not applicable | not applicable | not applicable | `test_stream_reply_parts_live_then_folded` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_failed_send_leaves_thread_and_restores_draft` | action ok:false → row gone, composer text = sent text, "Message not sent." |
| 1.2 | unit | `test_resend_submits_restored_text_once` | failure + 5 s fake clock → action called once; Resend → one new pending row, notice cleared |
| 1.3 | unit | `test_failure_restore_respects_existing_draft` | "old" refused after "new" was typed → composer "old\nnew", Resend shown; remount → "old" |
| 1.4 | unit | `test_session_failure_keeps_sign_in` | status 401 → Sign in link, text restored, no Resend |
| 2.1 | unit | `test_inflight_turn_has_running_reply` | optimistic row → trigger + `:reply` running, content []; composer input and Send enabled |
| 2.2 | unit | `test_reply_parts_from_row` | reasoning, tools a(done, 900 ms), b(running), answer → 4 ordered parts, statuses, ids, timing |
| 2.3 | unit | `test_tool_start_stamped_from_first_frame` | started t=1000, progress t=1500, completed ms=700 → start 1000; repeat → new entry |
| 2.4 | unit | `test_reasoning_span_stamped_once` | reasoning t=1000, t=1500, answer t=9500 → 1000/9500; answer-only → neither |
| 2.5 | unit | `test_malformed_reply_metadata_reads_absent` | stamps "x", NaN; tools missing → absent, no throw |
| 3.1 | unit | `test_reply_renders_through_grouped_parts` | reasoning + tool + text → chip, "Tool calls" list, markdown in order |
| 3.2 | unit | `test_text_part_and_settled_row` | running → cursor, no Copy, no `data-settled`; complete → Copy and `data-settled="true"` |
| 3.3 | unit | `test_outcome_floor_without_text_part` | complete, no text → outcome sentence; fleet error → failed tone |
| 4.1 | unit | `test_thought_chip_live` | running, "A. Checking the header" → "Thinking", "Checking the header", clock |
| 4.2 | unit | `test_thought_chip_folds_with_duration` | span 1000→9500 → "Thought · 8.5s", closed, text absent until opened |
| 4.3 | unit | `test_thought_clock_ticks_only_the_leaf` | fake clock +1 s → clock text changes, parent render count unchanged; fold and unmount leave 0 timers |
| 4.4 | unit | `test_thought_clock_resumes_from_row_stamp` | remount at now=5000, start 1000 → "4.0s"; no start → "Thought" without "·" |
| 4.6 | unit | `test_format_seconds_fixed_width` | 400 → "0.4s"; 8000 → "8.0s"; 8500 → "8.5s"; 125300 → "125.3s"; −5 → "0.0s" |
| 4.5 | unit | `test_latest_sentence_edges` | "" → ""; "no stop" → "no stop"; "A. B.  " → "B."; "Ünï. Ça va" → "Ça va" |
| 5.1 | unit | `test_tool_row_reads_part_timing` | running start now−2000 → "2.0s" ticking; done 700 → "✓", "0.7s"; two tools → one list; settled reply + unfinished tool → no clock |
| 6.1 | unit | `test_braille_spinner_frames_and_animation` | ten glyphs in order; aria-hidden; braille-spin class; reduced-motion classes hide the column and show `⠶` |
| 7.1 | unit | `test_indicator_verb_and_accessible_name` | running, no parts → status "Working" + verb; queued → "Queued" |
| 7.2 | unit | `test_loading_verb_for_is_stable` | same key twice → same verb; 50 keys → ≥5 verbs |
| 8.1 | e2e | `test_stream_reply_parts_live_then_folded` | routed reasoning → "Thinking"; tool pair → timed row; answer → "Thought ·" folded |
| 8.2 | e2e | `test_failed_send_resend_journey` | abort the action POST → row gone, text in composer; Resend → one row |
| 8.3 | e2e | `test_streaming_reply_costs_no_long_tasks` | 3 s of live chip and streaming → 0 `longtask` entries; attached frame p95 ≤ budget |
| 4.2 | unit | existing `role-reasoning.test.ts` cases (regression) | fold opens while arriving, closes after the answer, operator can reopen |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Parts, chip, tools, wait state and resend behave (§1–§5, §7) | `cd ui/packages/app && bunx vitest run components/domain lib/streaming lib/utils.test.ts components/layout tests/fleet-thread tests/fleet-tool-calls.test.tsx` | exit 0 | P0 | |
| R2 | Spinner is CSS-only with a static reduced-motion frame (§6) | `cd ui/packages/design-system && bunx vitest run src/design-system/BrailleSpinner.test.tsx` | exit 0 | P0 | |
| R3 | Live parts, resend, zero long tasks and frame p95 within budget on the real app (§8) | `cd ui/packages/app && AGENTSFLEET_UI_ENV_FILE="$HOME/.config/agentsfleet/ui.env.local" bunx playwright test --config=playwright.acceptance.config.ts --project=journeys tests/e2e/acceptance/fleet-reply-parts.spec.ts tests/e2e/acceptance/fleet-resend.spec.ts tests/e2e/acceptance/fleet-stream-transport.spec.ts tests/e2e/acceptance/fleet-thread.spec.ts` | exit 0; attached frame p95 ≤ 17.6 ms | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Versions in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S8 | Orphan and deprecated-path sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** A **deferral** leaves work unowned inside a closed spec; a **transfer** moves the criterion whole into a named successor spec that carries it as its own P0. Mark such a row `MOVED to M{N}_{NNN} R{n}` only when all three hold: (1) the successor exists and carries the row; (2) both specs record the mapping; (3) Discovery carries the owner's verbatim quote. A MOVED row is never rendered ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| failed-row path | `git grep -nE "markOptimisticFailed\|retryFailedDelivery\|AGENTSFLEET_EVENT_STATUS\.FAILED" -- ui/packages` | 0 matches |
| failed badge constant | `git grep -n "STATUS_FAILED" -- ui/packages/app/components` | 0 matches |
| custom-bag reply readers | `git grep -nwE "readTools\|readReasoning\|readThinking\|ToolCalls" -- ui/packages/app` | 0 matches |
| old reasoning and wait surfaces | `git grep -nE "REASONING_LIVE_LABEL\|PawPrintIcon" -- ui/packages/app/components/domain` | 0 matches |
| deprecated assistant-ui paths | `git grep -nE "MessagePrimitive\.Parts\|ChainOfThoughtPrimitive" -- ui/packages/app` | 0 matches |
| panel footer orphaned by the composer move | `git grep -n -w "DashboardPanelFooter" -- ui/packages` | 0 matches |

## Out of Scope

- Tool arguments and output blocks (the runner streams no output; `args_redacted` needs its own rendering review), and an idempotency key with silent auto-retry on the steer POST (a security-reviewed backend change) — milestone two.
- Persisted reasoning and durations (history rows carry no reasoning), composer status strip, flight-log rail — milestone two; the React Compiler stays off per `next.config.ts` until the codebase is annotated.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator steers a fleet on a reasoning model, watches "⠧ Thinking · 6.4s  checking whether the webhook is signed", sees `◐ read_file 1.2s` tick to `✓ 1.4s`, and the answer lands under a folded "Thought · 8.5s".
2. **Preserved user behaviour** — Steering while the fleet works; the reasoning fold opening while it arrives and closing after; status queries named Working/Queued; Copy on a settled reply; Sign in on an expired session.
3. **Optimal-way check** — The unconstrained shape persists durations server-side; browser stamps suffice while reasoning itself is live-only.
4. **Rebuild-vs-iterate** — Rebuild the reply surface onto library parts; iterate everything under it (stream, registry, reducers).
5. **What we build** — Reply parts converter, GroupedParts rendering, Thought chip, tool rows, braille glyph, verb wait state, composer resend.
6. **What we do NOT build** — Tool output blocks, auto-retry, persisted thoughts, gradient shimmer (banned).
7. **Fit with existing features** — Compounds with the typed reasoning stream, the first-visible measure and the composer layout; must not raise streaming frame cost.
8. **Surface order** — UI-first; the command-line `steer` printing reasoning as answer is a separate follow-up.
9. **Dashboard restraint** — No duration without a measured start; no token counts (none are streamed for reasoning).
10. **Confused-user next step** — "Message not sent." with the text back in the composer and one Resend button; opening "Thought" shows exactly what the fleet weighed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Resend (§1) is independent; parts (§2) precede rendering (§3), which hosts the chip (§4), tools (§5) and wait state (§7); the glyph (§6) precedes §4 and §7; proof (§8) runs last.
- **Alternatives considered:** keeping the custom-bag renderers and adding a clock (rejected: Kishore asked for library primitives, and the kit already models every state here); a `requestAnimationFrame` text-node clock (rejected: a leaf with its own state gives the same isolation with standard React); `useOptimistic` (rejected, Prior-Art).
- **Patch-vs-refactor verdict:** this is a **refactor** of the reply surface onto assistant-ui parts because the custom bag duplicated what the library models; the stream and registry beneath are unchanged.

## Discovery (consult log)

- **Consults** — Kishore (Sep 26, 2026): "Switch to chain of thought, reasoning, reply, tool parts everything to the best practice so assistant-ui, as opposed to us handrolling"; "try and use the standar features of React 19 or Next.js as relevant … to make sure we are performant"; "focussed on robust seamless elegant performant user experience … and the thinking spinner". Chose the Claude.ai resend over Retry and the braille spinner; "I want all in this PR"; "also cleanup any dead orphaned code" — every symbol this PR orphans leaves in it, the Dead Code Sweep table is the ledger, and VERIFY extends it with any newly dead code found after the refactor (`DashboardPanelFooter`: zero production consumers since the composer moved into `ViewportFooter`; the design system is `private`, so no outside consumer). Agent choices, flagged for Kishore: reuse `LOADING_VERBS` instead of a new nautical verb list; leaf-state clock instead of the earlier text-node loop. `spec.ordering` will be red (the branch's first commit is code) — Kishore's override at PR time, not the agent's.
- **Findings** — 0.15.22 infers `thread.isRunning` from the last message when the adapter omits it (`thread-runtime.ts:211-221`), which disabled Send under a running reply; `FleetThread` now passes `isRunning: false`. A refused send uses the library's own draft return (`MessageNotSentError`, `types/error.ts:55-72`), which replaced the hand-built restore effect and a Restore button. Frame baseline on `5f236cbcc` (pre-parts rendering): 0 long tasks, 334 frames, p95 16.8 ms.
- **Metrics review** — no events added; `agentsfleet.chat.submit_to_first_visible` keeps its trigger (first answer, reasoning or tool paint).
- **Skill-chain outcomes** — pending. Open PR #717 obligations carried in: reply to Greptile P1 `4111114471` (unreachable composer — fixed by `51656f7ac`) and P2 `4111168712` (live chunk masked by completion — fixed by the chunk-only fixture); Session notes 3; `/review` rerun on the final diff; `orly-babysit-prs`.
- **Deferrals** — none.
