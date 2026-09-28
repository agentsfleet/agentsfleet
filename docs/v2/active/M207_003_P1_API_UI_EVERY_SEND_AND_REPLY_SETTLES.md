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

# M207_003: Every fleet-chat send and reply reaches a visible end

**Prototype:** v2.0.0
**Milestone:** M207
**Workstream:** 003
**Date:** Sep 28, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: a hung send blocks a fleet's chat until reload, a Thought clock can run forever, and a refused reuse invites a Resend that can never land
**Categories:** API, UI
**Batch:** B1 — sole workstream; the follow-up that M207_001 and M207_002 deferred to
**Branch:** feat/m207-003-every-send-settles
**Baseline revision:** 2651207a43e807f850b7ea91c5da2d2885f7da71
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none — M207_001 and M207_002 merged in PR #717 (`5a68c5518`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 28, 2026) from the M207 review and Greptile follow-ups; every behaviour claim read from source at `5a68c5518`
**Canonical architecture:** `docs/architecture/user_flow.md` §8.3 Triggering the Fleet; `docs/architecture/runner_fleet.md` §Live activity (the SSE tail)

---

## Overview

**Goal (testable):** every steer the dashboard sends ends in a state the operator can see and act on, every running reply clock stops when its event ends, and each tool frame pairs with the call that sent it.
**Problem:** a Server Action that never answers blocks every later send on that fleet until reload. Another tab's in-flight send is invisible here, and if that tab closes, the send stays hidden. A dismissed send can come back from another tab. A reused id refused with 409 `UZ-AGT-016` offers Resend, which can only be refused again. A lost completion frame leaves the Thought clock ticking, and a Resend answered with an event the page never loaded leaves a "Queued" row forever. On the daemon, a replayed steer re-reads its row and answers unchecked if the row is gone, and a caller's reused id is logged as an internal failure. A tool call whose start a reconnect missed can only be guessed at by timing.
**Solution summary:** the browser bounds every send, sees other tabs' sends through Web Locks, keeps dismissals as tombstones, gives the reused-id refusal its own state, and settles replies from the detail or backfill when frames stop. The daemon returns the stored digest on the insert it already runs, tells the caller when a 202 is a replay, and classifies a reused id as the caller's conflict. The runner stamps each call's frames with a call id that the daemon carries and the browser keys by. Polish and render-cost fixes ride along.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(app,api,runner): every chat send and reply reaches a visible end
- **Intent (one sentence):** an operator never waits on a send or a clock that cannot finish, and never gets offered a button that cannot work.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/lib/streaming/pending-sends.ts` — the ledger: merge rules, the reader claim, and the three rules that keep tabs from losing each other's entries.
2. `ui/packages/app/components/domain/useFleetMessageDelivery.ts` — the per-fleet delivery queue and how a send ends.
3. `rustd/crates/afd_events/src/steer.rs` — `append`, `replayed`, `repeat_of`: the read-back §4 removes from the append path.
4. `rustd/crates/afd_admission/src/admit.rs` — the insert whose `RETURNING` already carries `payload_digest`, and the drift warn.
5. `src/runner/engine/runner_progress.zig` — where the runner turns NullClaw observer events into tool frames.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/streaming/{pending-sends,pending-sends-storage}.ts` | EDIT | Web Lock liveness for a foreign `sending`; a `dismissed` tombstone state that wins every merge |
| `ui/packages/app/lib/streaming/pending-sends-locks.ts` | CREATE | The Web Lock hold/watch, split out so `pending-sends.ts` stays under its cap |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | EDIT | Send timeout, bounded queue wait, the reused-id outcome, reading a replayed answer's state |
| `ui/packages/app/components/domain/useFleetPendingSends.ts` | EDIT | Exposes the conflict state and the owner-gone reading |
| `ui/packages/app/components/domain/SteerComposer.tsx` | EDIT | Conflict notice without Resend, `redirect_url` on Sign in, byte counter, Send disabled over the limit |
| `ui/packages/app/lib/api/fleets.ts`, `ui/packages/app/lib/api/errors.ts`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/actions.ts` | EDIT | The 202's `replayed` flag reaches the hook; the error code reaches the outcome |
| `ui/packages/app/lib/streaming/{fleet-stream-reply-registry,fleet-stream-registry,fleet-stream-optimistic,fleet-stream-frames,fleet-stream-tool-frames}.ts` | EDIT | Stall-triggered settle, identity-stable backfill and progress, call-id pairing |
| `ui/packages/app/lib/streaming/{fleet-stream-entry,stream-recovery-window}.ts`, `ui/packages/app/lib/api/fleets-types.ts` | EDIT | Each running event's last-heard time; the silence window shared as `REPLY_STALL_MS`; the 202's `SteerAccepted` type |
| `ui/packages/app/lib/api/{events,events-types}.ts` | EDIT | Optional `call_id` on the three tool frames |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts` | EDIT | A tool call keeps the `callId` its frames name |
| `ui/packages/app/components/domain/{useFleetThreadEntries,useFleetEventStream,FleetReplyBody}.ts(x)` | EDIT | Trigger wrapper reuse; `reply` leaves the trigger's bag; wait verb hidden from assistive tech |
| `ui/packages/app/components/domain/{fleetMessageRenderers.tsx,fleetMessageReaders.ts}`, `ui/packages/app/tests/use-fleet-event-stream-frames.test.ts` | EDIT | The activity tick stops reading `reply` from the bag, its only reader, so `readReply` goes |
| `ui/packages/app/lib/utils.ts` | EDIT | Durations of a minute or more read with minutes |
| `ui/packages/design-system/src/design-system/Alert.tsx`, `ui/packages/design-system/src/tokens.css` | EDIT | Dismiss target at least 24 px; settled rows keep focus rings |
| Tests beside each file above, plus `ui/packages/app/tests/fleet-thread/*.test.ts` and `ui/packages/app/tests/e2e/acceptance/fleet-resend.spec.ts` | CREATE / EDIT | One test per Dimension |
| `ui/packages/app/tests/fleet-thread/{ledger-fixtures,steer-copy,steer-helpers,harness}.ts` | CREATE / EDIT | One shared ledger fixture and fake lock manager for the pending-sends suites; conflict copy; the 202 fixture's `replayed`; the thread page's path |
| `ui/packages/app/lib/auth/sign-in-redirect.ts` | EDIT | `signInPath`, the same guarded destination as a same-origin link, and `buildSignInUrl` built on it |
| `rustd/crates/afd_admission/src/{lib,admit,sql}.rs` (+ its tests) | EDIT | `Admitted` carries the stored digest and fleet; no drift warn for a steer |
| `rustd/crates/afd_events/src/{steer,lib}.rs`, `rustd/crates/afd_events/tests/{events_suite,integration_steer,integration_steer_replay,integration_steer_retry}.rs` | EDIT | Append decides from the insert; `replayed` travels to the handler as `Steered` |
| `rustd/crates/afd_events/tests/integration_steer_insert.rs` | CREATE | The §4 tests, split from the replay suite at its length cap |
| `rustd/crates/afd_wire/src/{event,event/steer}.rs`, `rustd/crates/afd_wire/tests/validation.rs` | EDIT | `SteerAccepted.replayed`; NUL refused; the request doc matches `deny_unknown_fields` |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer.rs` (+ `message_steer/tests.rs`), `rustd/crates/afd_api/tests/{fleet_messages_steer,fleet_lifecycle_live/message}.rs` | EDIT | 400 for NUL; `accepted` carries `replayed` |
| `rustd/crates/afd_http/src/services/event.rs` (+ every `FleetSteering` test double) | EDIT | `append` and `replayed` answer whether the 202 is a replay — the trait is how `replayed` reaches the handler |
| `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_fleet/src/lease/activity.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_activity.rs` | EDIT | Optional `call_id` accepted and republished |
| `rustd/crates/afd_api_runner/src/handler/runner/activity.rs` | EDIT | An empty or over-long `call_id` refuses the batch with the existing 400 |
| `rustd/crates/afd_api_tenant/src/handler/stream.rs`, `public/openapi.json` | EDIT | Server-Sent Events (SSE) kind list says `chunk`; 202 schema gains `replayed` |
| `rustd/crates/afd_api/tests/harness/stubs_ingress/answers.rs` | EDIT | `Admitted` literals gain the new fields |
| `src/lib/contract/activity.zig`, `src/runner/engine/runner_progress.zig` | EDIT | Tool frames carry a per-event call id |
| `src/runner/engine/runner_progress_tools.zig` (+ `runner_progress_tools_test.zig`, registered in `src/runner/tests.zig`) | CREATE | A tool call's frames and its call id, split out so `runner_progress.zig` (374 lines at `5a68c5518`) returns under the cap |
| `docs/architecture/user_flow.md`, `docs/architecture/runner_fleet.md` | EDIT | Steer outcomes and tool frame fields |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (timeout, stall window and size figures as named consts), NDC and ORP (the replaced read-back and name-only pairing leave no caller), LOG (the reused-id warn keeps `error_code`, drops the internal class), ERR and ERR-RS (no new code; `UZ-AGT-016` reused), FLL, TSC, TSJ, UIS, DTK, TGU (the conflict outcome is a state, not a flag), XCOMPILE (runner).
- `docs/REST_API_DESIGN_GUIDELINES.md` §9 — `replayed` is a new response field (stable class); a NUL `operation_id` moves from 500 to 400, no working input changes status.
- `docs/RUST_ERROR_STANDARD.md` — `Admitted`'s shape change keeps every error kind; no fallible signature changes class.
- `docs/LOGGING_STANDARD.md` — the drift warn's classification.
- `dispatch/write_zig.md` — the runner's activity-shape edit.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — `pending-sends.ts` (283), `useFleetMessageDelivery.ts` (205), `steer.rs` | the Web Lock and tombstone rules go in `pending-sends-storage.ts` or a sibling before `pending-sends.ts` grows past the cap |
| UFS | yes | `SEND_TIMEOUT_MS`, `REPLY_STALL_MS`, `CALL_ID_MAX_BYTES`, the lock name prefix as named consts |
| UI GATE / DESIGN TOKEN GATE | yes | Alert's Dismiss sizing with token utilities; no raw buttons |
| ZIG GATE | yes | the activity-shape edit keeps init/deinit pairing; cross-compile both Linux targets |
| LOGGING | yes | the steer reuse warn is `steer_operation_conflict` with `UZ-AGT-016` only |
| ERROR REGISTRY | no — no new code | `UZ-AGT-016` and the existing 400 reuse their registry entries |

## Prior-Art / Reference Implementations

- **Reference:** Web Locks API (`navigator.locks`, Mozilla Developer Network (MDN)) for tab liveness, the pattern for "is the owner of this still alive" without heartbeats; `lib/auth/sign-in-redirect.ts` `buildSignInUrl` for the return path; `loadingAccessibleName` in `components/layout/loading-verbs.ts` for announcement text; NullClaw's `tool_call_id` (`dispatcher.zig`) shows the upstream has ids, but the runner mints its own because the observer drops them.

## Sections (implementation slices)

### §1 — Every send ends visibly

A send is bounded, and another tab's send is visible once its tab is gone. **Implementation default:** `SEND_TIMEOUT_MS = 30_000`, above the server's 20 s daemon deadline (`lib/api/retry-config.ts`), so a slow but alive send is never abandoned. A send holds a Web Lock named for its operation id from `begin` to its end; a tab reading a foreign `sending` with no held lock reads it `unknown`. Without Web Locks the behaviour stays as today.

- **Dimension 1.1** — a Server Action with no answer by the timeout ends `unknown` with Resend; a late answer only settles the ledger entry → Test `test_hung_send_times_out_to_unknown` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)
- **Dimension 1.2** — a send queued behind a hung one runs once the timeout frees the queue → Test `test_queue_survives_a_hung_send` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)
- **Dimension 1.3** — another tab's `sending` stays hidden while its lock is held, and reads `unknown` with Resend once it is not → Test `test_foreign_sending_surfaces_when_its_tab_is_gone` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)
- **Dimension 1.4** — a dismissed send stays dismissed in every tab: a `dismissed` tombstone wins every merge and expires with the one-day time to live (TTL) → Test `test_dismissed_send_never_returns` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)

### §2 — A refused reuse is final

`UZ-AGT-016` is a deterministic refusal, so the browser gives it its own state, `conflict`: the notice says the message conflicts with one already sent and offers Dismiss and "Send as new", which mints a new id. Separately, a NUL in `operation_id`, which Postgres refuses as 500 today, is refused up front with the existing 400 sentence.

- **Dimension 2.1** — a 409 `UZ-AGT-016` ends in `conflict`, with no Resend offered → Test `test_operation_conflict_offers_no_resend` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)
- **Dimension 2.2** — the draft a conflict returns sends under a new operation id → Test `test_conflict_draft_mints_a_new_id` — DONE (chat suites 69 files / 580 tests green; ledger, delivery and composer files at 100% coverage)
- **Dimension 2.3** — an `operation_id` containing NUL is refused 400 before any ledger write → Test `test_nul_operation_id_is_refused` — DONE (`tenant_plane` 245 passed; the refusal precedes the store, whose outage stub would answer 503)
- **Dimension 2.4** — the `SteerRequest` doc (and OpenAPI) says unknown fields are refused, as `deny_unknown_fields` does → Test `test_steer_request_doc_matches_its_parser` — DONE (`afd_wire` wire suite 41 passed; reads the regenerated `public/openapi.json`)

### §3 — Every reply clock ends

The 202 says whether it answered an earlier admission. **Implementation default:** a replayed answer whose event is not loaded, or already complete, triggers one detail read for that event instead of waiting for frames. A running event this tab has not heard a frame for in `REPLY_STALL_MS` (the stream's 45 s silence window) gets one detail read, swept on the stream's own frames and heartbeats, so no timer is added. Changed from a backfill read at EXECUTE: backfill reads `since` the newest row seen, so it cannot return an older replayed event, and one detail read settles status and answer through the same `mergeBackfill` reducer the backfill path uses. The public API docs gain `replayed` on a `~/Projects/docs` branch at CHORE(close).

- **Dimension 3.1** — the steer 202 carries `replayed: true` on both replay paths and `false` on a fresh admission → Test `test_steer_202_names_a_replay` — DONE (live `tenant_plane` `integration_fleet_lifecycle::message` 3 passed)
- **Dimension 3.2** — a replayed answer for an unloaded or completed event settles its row from the detail, leaving no "Queued" row → Test `test_replayed_answer_settles_from_detail` — DONE (app coverage gate 343 files / 3135 tests green at 100%; `fleet-stream-registry.stall.test.ts`)
- **Dimension 3.3** — a reply whose completion frame was lost on a live stream stops its Thought clock after the stall read → Test `test_lost_completion_stops_the_thought_clock` — DONE (app coverage gate 343 files / 3135 tests green at 100%; `fleet-stream-registry.stall.test.ts`)

### §4 — Admission decides from its own insert

The conflict insert already returns the stored `payload_digest` (`afd_admission/src/sql.rs` `INSERT_ADMISSION`). **Implementation default:** it also returns `fleet_id`, and `Admitted` carries both, so `Steer::append` compares without `find_repeated`. The paths with no insert (`replayed`, `repeat_despite`) keep their lookup. The drift warn stays for producers whose drift is a deploy, not a caller.

- **Dimension 4.1** — a replayed append compares the digest and fleet from the insert and issues no second statement → Test `test_replayed_append_decides_from_its_insert` — DONE (live `events_suite` steer tests 16 passed; "no second statement" is the append path's shape, proven by the Dead Code Sweep grep, since a statement count is not observable in the lane)
- **Dimension 4.2** — a replayed append can no longer answer 202 unchecked when its row is gone (the fail-open at `unwrap_or(admitted.id)` is closed) → Test `test_replayed_append_never_answers_unchecked` — DONE (live `events_suite` 16 passed; the stored digest and fleet are rewritten between sends)
- **Dimension 4.3** — a steer reuse with a different message logs `steer_operation_conflict` with `UZ-AGT-016` and no `admission_payload_drifted`; a webhook redelivery drift still warns → Test `test_steer_reuse_is_not_an_internal_failure` — DONE (live `events_suite` 16 passed)

### §5 — Tool frames pair with their call

NullClaw runs a batch's calls one at a time, so two calls of one name are never open together; the defect is that a missed or ambiguous frame cannot be paired. **Implementation default:** the runner mints `call_id` as a per-event counter at `tool_call_start` and repeats it on that call's frames; the daemon accepts it as optional (≤`CALL_ID_MAX_BYTES`) and republishes it; the browser keys by it, keeping the timing rule only for frames without one. The daemon's acceptance ships before any runner sends it, because activity structs are `deny_unknown_fields`.

- **Dimension 5.1** — the daemon accepts, bounds and republishes an optional `call_id` on the three tool frames; frames without it are unchanged → Test `test_activity_carries_an_optional_call_id` — DONE (live `daemon_suite` `integration_runner_activity` 3 passed; bridge unit test `tool_frames_republish_their_call_id_and_never_invent_one`)
- **Dimension 5.2** — the runner stamps each call's started and completed frames with one call id, distinct per call in an event → Test `test_runner_stamps_one_call_id_per_call` — DONE (`zig build --build-file build_runner.zig test`: macOS 747/750 passed, 3 skipped; `ci-zig-alpine:0.16.0-r6` native aarch64 Linux 742/750 passed, 8 skipped)
- **Dimension 5.3** — the browser pairs a frame to its call by `call_id`, including a second same-name call whose start was missed → Test `test_tool_frames_pair_by_call_id` — DONE (app coverage 100%: 8,104/8,104 statements, 4,861/4,861 branches; 343 files / 3,137 tests)
- **Dimension 5.4** — the SSE kind list names `chunk`, and `runner_fleet.md` lists each tool frame's fields → Test `test_sse_kind_list_matches_published_kinds` — DONE (`afd_fleet` lib; reads the regenerated `public/openapi.json`)

### §6 — Chat polish

- **Dimension 6.1** — durations of a minute or more read with minutes ("2m 05s"), everywhere `formatSeconds` and `formatMs` render → Test `test_durations_show_minutes` — DONE (`lib/utils.test.ts`; app 343 files / 3,141 tests green)
- **Dimension 6.2** — Sign in from the session notice returns to the fleet (`buildSignInUrl`) → Test `test_sign_in_returns_to_the_fleet` — DONE (`SteerComposer.test.tsx`, `steer-recovery.test.ts`; `signInPath` beside `buildSignInUrl`)
- **Dimension 6.3** — a live byte count shows from 90% of the limit, and Send is disabled over it → Test `test_byte_limit_counter_and_disabled_send`
- **Dimension 6.4** — Alert's Dismiss is at least 24×24 CSS px (Web Content Accessibility Guidelines (WCAG) 2.2 target size) → Test `test_dismiss_target_is_24px`
- **Dimension 6.5** — the wait verb is hidden from assistive tech; the status is still named "Working" or "Queued" → Test `test_wait_verb_is_not_announced`
- **Dimension 6.6** — a focused control on a settled row shows its whole focus ring → Test `test_settled_row_keeps_its_focus_ring`
- **Dimension 6.7** — a `/design-review` pass on the notice and reply rows records its ink-hierarchy findings, each fixed or listed → Test `manual_ink_hierarchy_review`

### §7 — Streaming stays cheap

- **Dimension 7.1** — a progress frame for a running tool returns the same timeline array; only a completion changes it → Test `test_progress_frame_keeps_identity` — DONE (app coverage 100%: 8,104/8,104 statements, 4,861/4,861 branches; 343 files / 3,137 tests)
- **Dimension 7.2** — a backfill page returns the same row objects for unchanged terminal rows, and the same array when nothing changed → Test `test_backfill_keeps_unchanged_identity` — DONE (app coverage 100%: 8,118/8,118 statements, 4,890/4,890 branches; 232 app suites / 2,100 tests)
- **Dimension 7.3** — a reply delta leaves its trigger message's identity alone (`reply` leaves the trigger's bag) → Test `test_reply_delta_keeps_trigger_identity` — DONE (app coverage 100%: 8,118/8,118 statements, 4,890/4,890 branches; 232 app suites / 2,100 tests)

## Interfaces

```
POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages
  202 { "status": "accepted", "event_id": "<millis>-<seq>", "replayed": false }   (true on either replay path)
  400 operation_id containing NUL — existing sentence, "operation_id must be between 1 and 200 bytes …"
  409 UZ-AGT-016 unchanged
Runner activity (POST /v1/runners/me/leases/{lease_id}/activity) and SSE tool frames:
  tool_call_started   { name, args_redacted, call_id? }
  tool_call_progress  { name, elapsed_ms, call_id? }
  tool_call_completed { name, ms, call_id? }        call_id: string, 1..=CALL_ID_MAX_BYTES
Admitted { id, replayed, stored_digest, stored_fleet }   (afd_admission)
PendingSend.state += "conflict" | "dismissed"            (browser ledger)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Hung Server Action | browser-to-Next request never answers | ends `unknown` at the timeout, queue freed; a late answer settles the entry |
| Owner tab closed mid-send | tab dies while `sending` | lock released; other tabs show `unknown` with Resend |
| No Web Locks | older browser | today's behaviour: hidden until reload or TTL |
| Dismiss races a Resend in another tab | tab A dismisses, tab B's send ends | tombstone wins; the entry stays gone |
| Reused id refused | 409 `UZ-AGT-016` | `conflict` state; Send as new mints an id; no Resend |
| NUL in `operation_id` | hostile or broken client | 400, nothing admitted |
| Replayed answer for an unloaded event | Resend after the event left the page | one detail read settles the row |
| Detail read fails | network or 5xx | the row keeps its state; the stall read retries it once |
| Lost completion frame | frame dropped on a live stream | the stall read settles it |
| Row deleted before comparison | fleet deleted mid-retry | the insert's own digest decides; never an unchecked 202 |
| New runner, old daemon | runner release ahead of the daemon | the batch is refused 400 as today; the release notes pin daemon-first |
| Frame without `call_id` | older runner | the timing rule pairs it, as today |

## Invariants

1. A steer's operation id admits at most one event per fleet — the `(producer, producer_key)` unique index, unchanged.
2. A replayed append never answers without comparing digest and fleet — the comparison reads fields of the `Admitted` it holds; `test_replayed_append_never_answers_unchecked` fails without it.
3. A `dismissed` entry is never replaced by another state for its id — `mergeIncoming`'s state precedence, pinned by `test_dismissed_send_never_returns`.
4. No send stays `sending` past `SEND_TIMEOUT_MS` in the tab that owns it — the timeout race in the delivery hook, pinned by `test_hung_send_times_out_to_unknown`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `steer_operation_conflict` (warn, existing) | ops | a steer reuses an id with a different message or sender | `error_code`, `fleet_id`, `event_id` | no message text, no operation id | `test_steer_reuse_is_not_an_internal_failure` |
| `admission_payload_drifted` (warn, existing) | ops | a non-steer producer's redelivery drifts | unchanged | unchanged | `test_steer_reuse_is_not_an_internal_failure` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_hung_send_times_out_to_unknown` | never-settling action → `unknown` at the timeout; a later ok → entry removed, no second row |
| 1.2 | unit | `test_queue_survives_a_hung_send` | hung send then B → B's action is called once the timeout passes |
| 1.3 | unit | `test_foreign_sending_surfaces_when_its_tab_is_gone` | foreign `sending`, lock held → hidden; lock released → `unknown` with Resend; no `navigator.locks` → hidden |
| 1.4 | unit | `test_dismissed_send_never_returns` | tab A dismisses while tab B's Resend fails → the entry stays gone in both |
| 2.1 | unit | `test_operation_conflict_offers_no_resend` | 409 with `UZ-AGT-016` → `conflict`, Dismiss and Send as new, no Resend |
| 2.2 | unit | `test_conflict_draft_mints_a_new_id` | draft back after a conflict, sent again → a new operation id |
| 2.3 | integration | `test_nul_operation_id_is_refused` | `operation_id` with `\u0000` → 400 with the operation-id sentence; no admission row |
| 2.4 | unit | `test_steer_request_doc_matches_its_parser` | doc text says unknown fields are refused; an unknown field → 400 |
| 3.1 | integration | `test_steer_202_names_a_replay` | fresh → `replayed:false`; repeat → `true`; repeat on a stopped fleet → `true` |
| 3.2 | unit | `test_replayed_answer_settles_from_detail` | replayed id not loaded → one detail read → terminal row, no "Queued"; the read fails → the row keeps its state and the stall read retries once |
| 3.3 | unit | `test_lost_completion_stops_the_thought_clock` | thinking, no frame for the stall window → the detail says complete → Thought with a total |
| 4.1 | integration | `test_replayed_append_decides_from_its_insert` | same id and body twice → first event, no `find_repeated` call |
| 4.2 | integration | `test_replayed_append_never_answers_unchecked` | the conflict returns a foreign digest or fleet → 409, never 202 |
| 4.3 | integration | `test_steer_reuse_is_not_an_internal_failure` | reuse with a new message → one `steer_operation_conflict`, zero `admission_payload_drifted`; webhook drift → one warn |
| 5.1 | integration | `test_activity_carries_an_optional_call_id` | frame with `call_id` → published with it; without → unchanged; over the bound → 400; any other unknown field → the batch is still refused |
| 5.2 | unit | `test_runner_stamps_one_call_id_per_call` | two calls of one tool in an event → two ids, each on its started and completed frames |
| 5.3 | unit | `test_tool_frames_pair_by_call_id` | a second call's completion with its own id and no start → a second call, not a restatement; a frame without `call_id` pairs by timing, as today |
| 5.4 | unit | `test_sse_kind_list_matches_published_kinds` | the documented kind list equals the `Published` kinds |
| 6.1 | unit | `test_durations_show_minutes` | 125_000 ms → "2m 05s"; 59_000 → "59.0s"; 850 → "850ms" |
| 6.2 | unit | `test_sign_in_returns_to_the_fleet` | session notice's Sign in href carries the fleet path as `redirect_url` |
| 6.3 | unit | `test_byte_limit_counter_and_disabled_send` | 7,400 bytes → counter visible; 8,193 → Send disabled |
| 6.4 | unit | `test_dismiss_target_is_24px` | Dismiss carries the size utilities for at least 24 px |
| 6.5 | unit | `test_wait_verb_is_not_announced` | verb span `aria-hidden`; output named "Working" or "Queued" |
| 6.6 | e2e | `test_settled_row_keeps_its_focus_ring` | Tab onto Copy on a settled row → the ring box lies inside the painted area |
| 6.7 | manual | `manual_ink_hierarchy_review` | Kishore reviews the `/design-review` report on DEV; each finding fixed or listed in Session Notes |
| 7.1 | unit | `test_progress_frame_keeps_identity` | progress on a running tool → same array reference; completion → new |
| 7.2 | unit | `test_backfill_keeps_unchanged_identity` | same terminal page twice → same row objects and array |
| 7.3 | unit | `test_reply_delta_keeps_trigger_identity` | a reply delta → the trigger message object is unchanged |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every send ends visibly (§1) | `cd ui/packages/app && bunx vitest run components/domain/useFleetMessageDelivery.test.tsx lib/streaming/pending-sends` | exit 0 | P0 | |
| R2 | A reused id is final, NUL refused (§2) | `grep -c "UZ-AGT-016" ui/packages/app/components/domain/useFleetMessageDelivery.ts` | ≥ 1 | P0 | |
| R3 | Replays are named and settle (§3) | `grep -c '"replayed"' public/openapi.json` | ≥ 1 | P0 | |
| R4 | Append never answers unchecked (§4) | `git grep -c "unwrap_or(admitted.id)" -- rustd/crates/afd_events/src/steer.rs` | 0 matches | P0 | |
| R5 | Tool frames carry a call id (§5) | `git grep -c "call_id" -- rustd/crates/afd_wire/src/activity.rs` | ≥ 3 | P1 | |
| R6 | Resend journeys still pass on DEV | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys tests/e2e/acceptance/fleet-resend.spec.ts` | exit 0 | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration lane green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Versions in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository-command rows point at the final `orly gate pr` in Session Notes. A P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| append-path `repeat_of` call | `git grep -n "repeat_of(&admission)" -- rustd/crates/afd_events/src/steer.rs \| grep -c "unwrap_or"` | 0 matches |

## Out of Scope

- `orly gate pr` skipping spec criteria for specs in `done/` ("no active spec"): an engine defect in the orly repository, reported there.
- A `make` lane for the runner's Zig tests: none exists (`make/test.mk:19` runs Rust and TypeScript only); 5.2's test runs through `zig build --build-file build_runner.zig test` and its absence from a lane is recorded in Discovery for Kishore.
- Forwarding NullClaw's provider `tool_call_id`: its observer drops it, and a fork change is a separate NullClaw release.
- Control characters other than NUL in `operation_id`: accepted and stored today, and refusing them would tighten `/v1` validation (the Representational State Transfer (REST) API guide §9).

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator on a flaky connection sends, sees "Couldn't confirm" within half a minute, presses Resend once, and sees one reply whose Thought clock stops.
2. **Preserved user behaviour** — Send, Resend, Dismiss, the draft coming back on a refusal, and one row per message all keep working unchanged.
3. **Optimal-way check** — the optimal shape is a delivery channel that confirms by itself (the stream echoing each operation id); bounding and reading on demand reaches the same moment without a new channel.
4. **Rebuild-vs-iterate** — iterate: every item is a local rule on an existing path; see Decomposition.
5. **What we build** — the timeout, the lock, the tombstone, the conflict state, the stall read, `replayed`, the insert-side comparison, `call_id`, and the polish and render-cost fixes.
6. **What we do NOT build** — a stream-level delivery echo, a NullClaw fork change, tightened `/v1` validation (Out of Scope).
7. **Fit with existing features** — compounds with the ledger, reader claim and Resend from M207_002; must not destabilise at-most-once admission.
8. **Surface order** — UI-first: the defects are the dashboard's; the Command-Line Interface (CLI) steer only gains a field it ignores.
9. **Dashboard restraint** — no new controls except Send as new on a conflict; the byte counter shows only near the limit.
10. **Confused-user next step** — the conflict notice says to send the text as a new message; the timeout notice offers Resend.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections by the path each fix sits on (delivery, refusal, settle, admission, frames, polish, render cost), browser-first so each lands behind its own tests.
- **Alternatives considered:** a heartbeat per pending send in storage (rejected: Web Locks give liveness without writes); forwarding NullClaw's ids (rejected: needs a fork release, and the adapter's counter is enough for pairing).
- **Patch-vs-refactor verdict:** this is a **patch** because each defect has a local cause on a path that works; the refactor worth naming, a delivery echo on the stream, would replace §1 and §3's reads and is not needed for the successful moment.

## Discovery (consult log)

- **Consults** — scope set by Kishore's deferral below. Clerk DEV runs one session per browser (`single_session_mode: true`, public `/v1/environment`, Sep 28); production is unverified and matters for §1's lock names only in that one user owns a tab. Source facts read at `5a68c5518` for every item.
- **Metrics review** — no new events; the reused-id warn is reclassified (Metrics table). No analytics/funnel playbook update required: no funnel step changes.
- **Skill-chain outcomes** — pending.
- **Coverage, per Kishore's bar (TypeScript 100%, Rust patch > 99%)** — app package 100% (8,118/8,118 statements, 4,890/4,890 branches, 2,193/2,193 functions, 7,179/7,179 lines; `vitest --coverage` JSON summary). Rust patch 110/110 instrumented added lines = 100.00%: `make test-coverage-rustd` (exit 0; workspace 97.8505%, 42,883/43,825) wrote `rustd/lcov.info`, crossed with `git diff -U0 origin/main...HEAD -- '*.rs'`.
- **Runner Zig tests have no `make` lane** (Out of Scope): `test_runner_stamps_one_call_id_per_call` ran through `zig build --build-file build_runner.zig test` on macOS (747/750 passed, 3 skipped) and natively on aarch64 Linux in `ghcr.io/agentsfleet/ci-zig-alpine:0.16.0-r6` (742/750 passed, 8 skipped). The x86_64-linux cross-build reaches link on both hosts and stops there only because each host's static curl is for another architecture (`/opt/curl-min/lib/libcurl.a` missing on macOS; arm64-only in the image). For Kishore: no repository lane runs these tests.
- **Deferrals** — this spec carries the items deferred from M207_001 and M207_002:

> Indy (2026-09-28 10:51): "go" — context: answer to an AskUserQuestion recommending that the M207 edge cases (F6, F7, F12, F13, dismissed-send tombstones, the 409 Resend loop, `Admitted` carrying the digest, the drift warn's class, polish and render cost) move to follow-up spec M207_003.
