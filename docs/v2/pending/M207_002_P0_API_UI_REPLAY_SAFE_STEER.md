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

# M207_002: A steer sent from the dashboard is admitted once, whatever happens to the response — a client operation id rides every send, automatic retry, Resend and reload

**Prototype:** v2.0.0
**Milestone:** M207
**Workstream:** 002
**Date:** Sep 28, 2026
**Status:** PENDING
**Priority:** P0 — one click can create two durable fleet runs today; the operator pays and reads twice
**Categories:** API, UI
**Batch:** B1 — rides PR #717 beside M207_001 by Kishore's call
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M207_001 — the composer, the refusal notice and the resend flow this workstream makes replay-safe
**Provenance:** LLM-drafted (Claude, Sep 28, 2026) from a Codex review of PR #717
**Canonical architecture:** `docs/architecture/user_flow.md` §User steer

---

## Overview

**Goal (testable):** A steer whose response is lost — socket reset, tab closed, page reloaded, fleet paused in between — is admitted exactly once when sent again, and the dashboard never reports a delivered message as unsent without offering a safe way to confirm it.
**Problem:** The steer POST replays on a dropped socket (`lib/api/retry.ts` lets a transport drop through for every method) and Resend posts the composer's text as a new message, while the body carries no identity. A message the daemon accepted can therefore run twice. Two fast sends that both fail lose the first one, because the composer keeps one failure slot and assistant-ui restores only the newest draft. A refusal that lands after a navigation leaves the new composer with a notice and nothing to resend.
**Solution summary:** The browser mints a UUID v4 operation id before it appends the optimistic row, sends it as the endpoint's existing `operation_id` field, and keeps every unresolved send in a per-fleet ledger mirrored to `localStorage`. The notice lists each unresolved send with its own Resend, which posts the ledger record under the same id. The daemon, which already deduplicates on `operation_id`, scopes the key to the fleet, answers a repeat before the paused and budget gates, and refuses a key reused with a different message. The operator sees one row per message, always.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(app,api): admit a dashboard steer once across retry, Resend and reload
- **Intent (one sentence):** A message an operator sends to a fleet runs once, and the dashboard tells the truth about whether it was delivered.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_events/tests/integration_steer_retry.rs` — the dedup already proven at three depths (event id, ledger row, stream entry); its `operation()` helper prefixes the fleet by hand because the key is global, which §4 fixes.
2. `rustd/crates/afd_admission/src/admit.rs` — the commit path: fleet budget, then the `INSERT … ON CONFLICT` whose conflict arm answers the first row; the digest check that only warns today.
3. `rustd/crates/afd_api_tenant/src/handler/fleet/message.rs` — the steer handler: ingress check before `append`, `read_steer`, and the OpenAPI prose that already promises the retry contract.
4. `ui/packages/app/lib/api/retry.ts` — `#mayRetry`: a dropped socket lifts the idempotency gate for any method; the replay is safe only once the body carries the id.
5. `ui/packages/app/lib/analytics/posthog.ts` — `withMarkerStore`: the storage guard shape (absent or throwing `localStorage` is best-effort, never a crash).
6. https://docs.stripe.com/api/idempotent_requests — client-minted key, same key with different parameters refused, replay answers the first result.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/{fleets,fleets-types}.ts`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/actions.ts` | EDIT | `steerFleet` takes a `SteerRequest` with a required `operation_id`; the Server Action forwards it |
| `ui/packages/app/lib/streaming/pending-sends.ts` | CREATE | The per-fleet ledger of unresolved sends: module state, `localStorage` mirror, `storage`-event sync, listeners |
| `ui/packages/app/lib/streaming/operation-id.ts` | CREATE | `mintOperationId`: UUID v4 from `crypto.randomUUID`, else `crypto.getRandomValues` |
| `ui/packages/app/components/domain/useFleetPendingSends.ts` | CREATE | The React boundary over the ledger (`useSyncExternalStore`) |
| `ui/packages/app/components/domain/useFleetDeliveryFailure.ts` | DELETE | Superseded by the ledger; one slot per fleet was the defect |
| `ui/packages/app/components/domain/{useFleetMessageDelivery,SteerComposer,FleetThread,FleetThreadViewport}.ts(x)` | EDIT | Mint before append; ledger from submit to acknowledgement; per-entry Resend and Dismiss; the unknown-delivery notice |
| `ui/packages/app/lib/streaming/{pending-sends,operation-id}.test.ts`, `ui/packages/app/components/domain/{useFleetPendingSends.test.tsx,SteerComposer.test.tsx}`, `ui/packages/app/tests/fleet-thread/{harness.ts,steer-submission.test.ts}`, `ui/packages/app/lib/api/{fleets.replay.test.ts,client.retry.test.ts}` | CREATE / EDIT | Browser proofs, through the real retry policy and the real composer |
| `ui/packages/app/tests/e2e/acceptance/fleet-resend.spec.ts` | EDIT | Both Server Action bodies carry one operation id; reload recovery |
| `rustd/crates/afd_events/src/steer.rs` | EDIT | Composes the fleet-scoped key; `replayed` lookup for the handler |
| `rustd/crates/afd_http/src/services/event.rs` | EDIT | `FleetSteering::replayed` |
| `rustd/crates/afd_admission/src/{admit,sql,error,lib}.rs` | EDIT | Lookup before the gates; payload drift refuses; `find_repeated` |
| `rustd/crates/afd_core/src/{error_code/fleet,problem/fleet}.rs` | EDIT | `AGENTSFLEET_OPERATION_CONFLICT` (`UZ-AGT-016`, 409) |
| `rustd/crates/afd_api_tenant/src/handler/fleet/{message,message_steer}.rs`, `handler/fleet/message/tests.rs` | EDIT / CREATE | The steer handler moves to its own file (the parent is over the length cap); repeat answered before the ingress check |
| `rustd/crates/afd_admission/src/tests.rs`, `rustd/crates/afd_events/tests/integration_steer_retry.rs`, `rustd/crates/afd_api_tenant/src/handler/fleet/tests.rs` | EDIT | Daemon proofs |
| `public/openapi.json` | EDIT | Regenerated: the 409 gains the operation-conflict code |
| `docs/architecture/user_flow.md` | EDIT | User steer line names the operation id and the ledger |
| `docs/v2/active/M207_001_P1_UI_CHAT_REPLY_ON_ASSISTANT_UI_PARTS.md` | EDIT | Invariant 4 and Out of Scope point here |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC and ORP (`useFleetDeliveryFailure` and `FailedDelivery` leave with every import and test), UFS (storage key, states, notice copy, error code are named constants), ECL (`refused` is the server saying no, `unknown` is the transport saying nothing; the notice and the tests tell them apart), TCF (each pin fails when its subject is removed), PJV (timestamps carried by value), PSR (the UUID comes from the platform, never a hand-rolled parser).
- `dispatch/write_rust.md` — ERR-RS: one new `ErrorKind` variant with its code decided in `Error::answer`; FN-RS: the lookup is a `Result` pipeline, no sentinel; UFS on the key separator.
- `docs/REST_API_DESIGN_GUIDELINES.md` §4 (409 carries `current_state`), §5 (registry code with title and hint; `detail` names no entity value), §6 (the artifact is regenerated, never edited).
- `dispatch/write_ts_adhere_bun.md` — §1 shape decision for the three new modules; §2 `const` and `as const` states; §11 the retry loop stays bounded by `retry-config.ts`.
- `docs/LOGGING_STANDARD.md` — the new refusal logs at warn with `error_code`, no message body.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TS FILE SHAPE DECISION | yes — `pending-sends.ts`, `operation-id.ts`, `useFleetPendingSends.ts` | functions-module for all three, matching `fleet-stream-registry.ts`: module-level `Map` plus listeners is the shape `useSyncExternalStore` wants |
| UI GATE | yes | The notice stays on `Alert` and `Button`; one row per entry, no raw HTML |
| DESIGN TOKEN GATE | yes | Spacing and colour through existing token utilities |
| UFS GATE | yes | `PENDING_SEND_STATE`, `STORAGE_KEY_PREFIX`, notice copy, `KEY_SEPARATOR`, `UZ-AGT-016` are constants |
| ERROR REGISTRY GATE | yes | `AGENTSFLEET_OPERATION_CONFLICT` declared in `error_code/fleet.rs` and `problem/fleet.rs` in the same commit as its first raise |
| LOGGING GATE | yes | `admission_operation_conflict` warn carries `error_code`, producer, fleet, event; never the body or the key |
| File & Function Length (≤350/≤50/≤70) | yes | `message.rs` is 370 already: the steer handler and its readers move to `message_steer.rs` before any line is added; `admit.rs` (345) sheds the lookup into `replay.rs`'s sibling `repeat.rs` |

## Prior-Art / Reference Implementations

- **Reference:** Stripe idempotent requests — client-minted key, 24 h retention, same key with a different body → `idempotency_error`, replay answers the first result. Aligned on all four; retention here is the ledger's own.
- **Reference:** `rustd/crates/afd_admission/src/tests.rs` `Key::Repeated("fleet-1:delivery-9")` — the webhook producer already scopes its key with the fleet; steers adopt the same composition instead of a second namespace or an ownership read.
- **Reference:** `ui/packages/app/lib/streaming/fleet-stream-registry.ts` — module registry, refcounted listeners, `__reset…ForTests`; the ledger is the same shape with a storage mirror.
- **Divergence, named:** Codex's review proposed an ownership check on replay; the fleet-in-key composition makes a cross-fleet answer unrepresentable, so no read is needed. Rollout hazard accepted: no client sends `operation_id` today (`git grep operation_id -- cli/src ui/packages` → 0), so no raw keys exist to bridge.

## Sections (implementation slices)

### §1 — Every browser steer carries an operation id

The id is minted synchronously at the top of `onNew`, before the optimistic append and before any `await`, so a mint failure rejects before the composer's draft is at risk. It rides the Server Action and `steerFleet` into the body's `operation_id`. The retry policy is untouched: the socket-drop replay is now safe because both attempts carry the same body. **Implementation default:** `crypto.randomUUID`, then `crypto.getRandomValues` because an HTTP origin on a LAN has the second and not the first; neither present → `MessageNotSentError`.

- **Dimension 1.1** — the id is minted before the append and rides the action → Test `test_operation_id_minted_before_append`
- **Dimension 1.2** — the fallback generator and the mint failure path → Test `test_mint_falls_back_then_refuses`
- **Dimension 1.3** — a socket drop replays the identical body through the real policy; a 503 does not → Test `test_socket_drop_replays_same_operation_id`
- **Dimension 1.4** — `steerFleet` refuses a call without an id at the type level → Test `test_steer_request_requires_operation_id`

### §2 — The pending-send ledger

One entry per unresolved send, keyed by operation id, from submit until the 202 is reconciled. State moves `sending → refused | session | unknown`; an entry hydrated from storage still `sending` reads as `unknown`, because the tab that owned it is gone. Mirrored to `localStorage` under a workspace-and-fleet key, read once per key on first subscribe, written through on every change, re-read on the `storage` event. **Implementation default:** exact-text match against the ledger reuses that entry's id when the operator presses Send on restored text, because a second id for the same unresolved text is the duplicate this spec exists to end.

- **Dimension 2.1** — an entry lives from submit to acknowledgement and no longer → Test `test_ledger_entry_lives_from_submit_to_ack`
- **Dimension 2.2** — two sends both refused keep two entries → Test `test_two_refused_sends_keep_two_entries`
- **Dimension 2.3** — persistence: a fresh module reads what the last one wrote; a `storage` event notifies → Test `test_ledger_survives_reload_and_syncs_tabs`
- **Dimension 2.4** — storage absent or throwing → in-memory only, no throw → Test `test_ledger_without_storage`
- **Dimension 2.5** — server render reads empty → Test `test_ledger_reads_empty_on_server`

### §3 — The notice: one row per unresolved send

The notice lists every entry with its own Resend (or Sign in) and Dismiss. Resend posts the record's text under its id through the same serialised tail as a new send; if the composer holds exactly that text, it is cleared so Enter cannot send it again. The mount-time restore of the newest refused text into an empty composer stays. Copy: refused → "Message not sent."; unknown → "Couldn't confirm this message was sent."

- **Dimension 3.1** — Resend posts the record under its id and clears a matching draft → Test `test_resend_posts_ledger_record_once`
- **Dimension 3.2** — the unknown state has its own sentence and still offers Resend → Test `test_unknown_delivery_notice`
- **Dimension 3.3** — Dismiss removes the entry and nothing else → Test `test_dismiss_removes_one_entry`
- **Dimension 3.4** — a refusal arriving after a remount shows with Resend → Test `test_failure_after_remount_is_resendable`
- **Dimension 3.5** — Send on a draft equal to an entry's text reuses its id → Test `test_draft_matching_pending_entry_reuses_its_id`
- **Dimension 3.6** — a session failure offers Sign in, no Resend, text restored → Test `test_session_failure_keeps_sign_in` (M207_001 1.4, amended)

### §4 — The daemon answers a repeat before it judges a new message

`Steer::append` composes `producer_key = "<fleet_id>:<operation_id>"`. `Admissions::admit` looks the key up before the fleet budget: found with the same digest → `Admitted { replayed: true }`; found with another digest → `OperationConflict`; absent → the existing path, whose conflict arm now refuses drift instead of warning. The handler asks `FleetSteering::replayed` before the ingress check, so a paused fleet answers a repeat with its first event. **Implementation default:** one registry code for drift, `UZ-AGT-016` 409 `current_state: "admitted"`, because a fleet-scoped key has no cross-tenant case left to name.

- **Dimension 4.1** — the key is fleet-scoped; equal ids on two fleets admit twice → Test `test_operation_ids_are_scoped_to_the_fleet`
- **Dimension 4.2** — a repeat bypasses a spent fleet budget → Test `test_replay_bypasses_fleet_budget`
- **Dimension 4.3** — a repeat bypasses a paused fleet's 409 → Test `test_replay_bypasses_paused_ingress`
- **Dimension 4.4** — same key, different message → 409 `UZ-AGT-016`, nothing admitted → Test `test_payload_drift_is_refused`
- **Dimension 4.5** — the registry entry is reachable and the regenerated artifact carries it → Test `test_operation_conflict_code_registered`
- **Dimension 4.6** — the steer handler moves to `message_steer.rs`; behaviour unchanged → Test `should_refuse_a_steer_that_carries_no_body` (existing, relocated)

### §5 — Proof on the real app

- **Dimension 5.1** — abort the first Server Action POST, Resend: both bodies carry one operation id, one transcript row → Test `test_failed_send_resend_journey` (amended)
- **Dimension 5.2** — reload with a `sending` entry: the notice offers Resend, Resend yields one row → Test `test_reload_recovers_unconfirmed_send`

## Interfaces

```
POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages   body { message, operation_id }   (shape unchanged; the dashboard always sends operation_id)
  202 { status: "accepted", event_id }                          first admission, or the same event for a repeat
  409 UZ-AGT-016 AGENTSFLEET_OPERATION_CONFLICT, current_state "admitted"   operation_id seen before with another message
Ledger key: producer "steer", producer_key "<fleet_id>:<operation_id>"     (afd_events::steer composes it; KEY_SEPARATOR ":")
Admissions::find_repeated(producer, key) -> Result<Option<Repeated { id, digest }>>
FleetSteering::replayed(fleet, workspace, operation_id) -> Result<Option<String>>
SteerRequest = { message: string; operation_id: string }   steerFleet(ws, fleet, request, token, retry?)   steerFleetAction(ws, fleet, message, operationId)
PendingSend = { operationId: string; text: string; state: "sending" | "refused" | "session" | "unknown"; submittedAtMs: number }
pending-sends.ts: begin(ws, fleet, send) · settle(ws, fleet, operationId) · fail(ws, fleet, operationId, state) · dismiss(ws, fleet, operationId) · byText(ws, fleet, text) · subscribe/getSnapshot
Storage: localStorage["agentsfleet:pending-sends:<workspaceId>:<fleetId>"] = JSON PendingSend[]
mintOperationId(): string                                    UUID v4; throws MintUnavailable when no generator exists
SteerComposer props: { pending: PendingSend[]; onResend(operationId): void; onDismiss(operationId): void }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Lost acknowledgement | 202 written, connection gone before it arrived | entry → `unknown`; "Couldn't confirm this message was sent." with Resend; Resend → 202 with the first event; one row |
| Socket drop before any response | `ECONNRESET`, `EPIPE` inside `steerFleet` | the policy replays the identical body; the daemon answers the first admission |
| Two sends both refused | serialised POSTs both fail | two entries, two Resend buttons; the composer holds the latest draft, the notice holds both |
| Refusal after remount | POST fails after a navigation inside the idle window | the new composer subscribes to the ledger; the entry appears with Resend |
| Reload or second tab mid-send | document gone before the 202 | hydrated `sending` reads as `unknown`; Resend is safe |
| Insecure origin | `crypto.randomUUID` undefined | `getRandomValues` fallback; neither → `MessageNotSentError` before the append; draft returns |
| Storage unavailable | private mode, quota, disabled | in-memory ledger; no throw; no cross-tab sync |
| Draft edited after the refusal | composer holds `A\nB` | no exact match → new id → one new message `A\nB`; A's entry keeps its own Resend and Dismiss |
| Repeat on a paused fleet | admitted, then paused | `replayed` finds the key → 202 before the ingress check |
| Repeat under a spent fleet budget | backlog at its cap | `admit` finds the key before `refuse_over_fleet_budget` |
| Same key, different message | client defect or forgery | 409 `UZ-AGT-016`, nothing admitted, warn log without the body |
| Same id on two fleets | a client reusing ids across fleets | fleet-scoped key → two rows, each answers its own fleet |
| Mixed daemon versions during deploy | an old daemon stored a raw key | at most one duplicate per retry crossing the deploy; no raw keys exist beforehand |

## Invariants

1. Every dashboard steer carries an operation id — `SteerRequest.operation_id` is a required string; `test_steer_request_requires_operation_id`.
2. One durable admission per (fleet, operation id) — `UNIQUE (producer, producer_key)` with the fleet inside the key; `test_steer_retry_reuses_its_admission`.
3. A repeat runs no gate its first admission passed — lookup precedes `is_runnable` and `refuse_over_fleet_budget`; `test_replay_bypasses_paused_ingress`, `test_replay_bypasses_fleet_budget`.
4. A key never answers with another payload's event — digest compared before the gates and on the conflict arm; `test_payload_drift_is_refused`.
5. The ledger holds no resolved send — `settle` runs on every `ok`; `test_ledger_entry_lives_from_submit_to_ack`.
6. No second id for text the ledger already holds — `byText` before `mint`; `test_draft_matching_pending_entry_reuses_its_id`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `admission_operation_conflict` (warn, `error_code=UZ-AGT-016`) | ops | a repeat's digest differs from the admitted row's | producer, fleet_id, event_id | no body, no key, no actor | `test_payload_drift_is_refused` |
| `admission_replayed` (existing debug) and `AdmissionOutcome::Replayed` | ops | unchanged; now also fires from the pre-gate lookup | unchanged | unchanged | `test_replay_bypasses_fleet_budget` |
| product analytics | not applicable — `agentsfleet.chat.submit_to_first_visible` unchanged; no funnel change | | | | `test_failed_send_resend_journey` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_operation_id_minted_before_append` | Send "deploy" → action called with a 36-char UUID; `appendOptimistic` called after the mint; the same id on the ledger entry |
| 1.2 | unit | `test_mint_falls_back_then_refuses` | `randomUUID` absent → id from `getRandomValues` matches `/^[0-9a-f]{8}-…-[0-9a-f]{12}$/`; both absent → `onNew` rejects `MessageNotSentError`, no append, no action |
| 1.3 | unit | `test_socket_drop_replays_same_operation_id` | real `requestWithRetry`: attempt 1 rejects with cause `ECONNRESET` after send, attempt 2 → 202: two POSTs, byte-equal bodies with one `operation_id`; 503 → one POST |
| 1.4 | unit | `test_steer_request_requires_operation_id` | `steerFleet(…, { message })` is a type error (`// @ts-expect-error` pin); body JSON carries both fields |
| 2.1 | unit | `test_ledger_entry_lives_from_submit_to_ack` | `begin` → one `sending` entry; `ok` → `settle` → empty; storage mirror empty too |
| 2.2 | unit | `test_two_refused_sends_keep_two_entries` | real composer, A then B before A resolves, both refused → two `refused` entries, two Resend buttons, notice texts A and B |
| 2.3 | unit | `test_ledger_survives_reload_and_syncs_tabs` | write, `vi.resetModules`, re-import → entry present as `unknown`; dispatch `storage` event with a new value → listener fired, snapshot updated |
| 2.4 | unit | `test_ledger_without_storage` | `localStorage` getter throws → `begin`/`fail`/`settle` work in memory; no throw; `test_ledger_reads_empty_on_server` covers `window` undefined |
| 2.5 | unit | `test_ledger_reads_empty_on_server` | `renderToStaticMarkup` → "no pending" |
| 3.1 | unit | `test_resend_posts_ledger_record_once` | entry {id X, "retry this"} + draft "retry this" → Resend → action called with ("retry this", X) once; draft ""; one optimistic row; entry gone on ok |
| 3.2 | unit | `test_unknown_delivery_notice` | entry `unknown` → "Couldn't confirm this message was sent." + Resend; `refused` → "Message not sent." |
| 3.3 | unit | `test_dismiss_removes_one_entry` | two entries, Dismiss the first → one entry remains, composer draft untouched |
| 3.4 | unit | `test_failure_after_remount_is_resendable` | send, unmount, remount, then refuse → new composer shows the entry with Resend; Resend posts under the original id |
| 3.5 | unit | `test_draft_matching_pending_entry_reuses_its_id` | entry {X, "old"} restored into the composer, press Send → action called with ("old", X), no new id |
| 3.6 | unit | `test_session_failure_keeps_sign_in` | 401 → `session` entry, Sign in link, no Resend, text restored |
| 4.1 | integration | `test_operation_ids_are_scoped_to_the_fleet` | same `operation_id` on fleets A and B → two event ids, two ledger rows, `producer_key` = `<fleet>:<id>` |
| 4.2 | integration | `test_replay_bypasses_fleet_budget` | admit K, fill the fleet backlog to its budget, admit K again → `replayed: true`, same id; a fresh key → `OverBudget` |
| 4.3 | unit | `test_replay_bypasses_paused_ingress` | handler: fake `replayed` → `Some(id)`, fake status paused → 202 with id; `None` + paused → 409 `UZ-AGT-012` |
| 4.4 | integration | `test_payload_drift_is_refused` | admit K "a", admit K "b" → `OperationConflict`, `code() == UZ-AGT-016`, one ledger row, one stream entry; refusal logged with `error_code` |
| 4.5 | unit | `test_operation_conflict_code_registered` | `UZ-AGT-016` resolves to status 409, title, hint; the existing `test_openapi_build_is_the_source` stays green after regeneration |
| 4.6 | unit | `should_refuse_a_steer_that_carries_no_body` | relocated with its siblings; `read_steer` behaviour unchanged |
| 4.1 | regression | `test_steer_retry_reuses_its_admission` | the existing three-depth proof holds under the fleet-scoped key; `test_two_operation_ids_with_equal_text_admit_twice` too |
| 5.1 | e2e | `test_failed_send_resend_journey` | abort first steer POST; Resend → both intercepted bodies contain the same UUID; transcript shows the text once |
| 5.2 | e2e | `test_reload_recovers_unconfirmed_send` | seed a `sending` entry in `localStorage`, load the page → notice with Resend; Resend → one transcript row |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Identity, ledger, notice and Resend behave (§1–§3) | `cd ui/packages/app && bunx vitest run lib/api lib/streaming components/domain tests/fleet-thread` | exit 0 | P0 | |
| R2 | The daemon answers repeats first and refuses drift (§4) | `cd rustd && cargo test -p afd_admission -p afd_events -p afd_api_tenant -p afd_core` | exit 0 | P0 | |
| R3 | The artifact is the build (§4) | `cd rustd && cargo test -p afd_api --test openapi_artifact --test openapi_codes` | exit 0 | P0 | |
| R4 | One admission across abort, Resend and reload on the real app (§5) | `cd ui/packages/app && AGENTSFLEET_UI_ENV_FILE="$HOME/.config/agentsfleet/ui.env.local" bunx playwright test --config=playwright.acceptance.config.ts --project=journeys tests/e2e/acceptance/fleet-resend.spec.ts` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed (both M207 specs) | `git diff --name-only origin/main...HEAD` | 0 paths missing from the two Files Changed tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{N}_{NNN} R{n}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** `ui/packages/app/components/domain/useFleetDeliveryFailure.ts` and its test.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the one-slot failure record | `git grep -nwE "useFleetDeliveryFailure\|FailedDelivery\|DELIVERY_FAILURE\|__resetFleetDeliveryFailuresForTests" -- ui/packages` | 0 matches |
| the draft-text Resend | `git grep -nw "SendFailureAction" -- ui/packages/app` | 0 matches |
| the unscoped key helper in the retry test | `git grep -n "fn operation(" -- rustd/crates/afd_events/tests/integration_steer_retry.rs` | 0 matches |

## Out of Scope

- `agentsfleet steer` from the command line sending an operation id (its own review: the terminal has no ledger to recover from) — flagged for Kishore.
- A `replayed` field on the 202 body; recovery across devices or signed-out browsers; retention or expiry of ledger entries beyond Dismiss.
- The five rendering defects from the same review (malformed tool name, backfill dropping reasoning stamps, tools-only turns grouped, duplicate tool completion, history reconverted per frame) — folded into M207_001's Sections by Kishore's call.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Wi-Fi drops as the operator hits Send; the notice says the message could not be confirmed; one click on Resend and the thread shows the message once, already running.
2. **Preserved user behaviour** — Sending while the fleet works; the refused text returning to the composer; Sign in on an expired session; a person who sends the same sentence twice on purpose gets two messages.
3. **Optimal-way check** — The unconstrained shape is what ships: a client identity the server already honours, plus a ledger the browser already needed.
4. **Rebuild-vs-iterate** — Iterate: the daemon's contract exists; the browser gains the half that was never wired.
5. **What we build** — Minted id, ledger with storage mirror, per-entry Resend and Dismiss, unknown-delivery copy, fleet-scoped key, pre-gate replay, drift refusal.
6. **What we do NOT build** — Server-side draft storage, a CLI ledger, ledger expiry, a new endpoint.
7. **Fit with existing features** — Completes M207_001's resend; keeps the serialised delivery tail and the streaming path untouched.
8. **Surface order** — API contract first (already public), dashboard second; CLI later.
9. **Dashboard restraint** — The notice shows the message text and two verbs; no ids, no counters.
10. **Confused-user next step** — "Couldn't confirm this message was sent." with Resend and Dismiss; whichever they pick, the thread never shows the message twice.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 (identity) and §4 (daemon) are independent; §2 (ledger) precedes §3 (notice); §5 runs last against the deployed daemon.
- **Alternatives considered:** an ownership read on replay instead of the fleet-in-key composition (rejected: a second read to answer a question the key can make unaskable, and the webhook producer already composes its key this way); disabling the socket-drop replay for steers (rejected: the replay is the cheap recovery once the body carries an id); `useOptimistic` or the composer draft as the recovery record (rejected in M207_001 — the row must outlive a navigation and the draft is one slot); one `Steer::append` that takes the ingress verdict (considered: one lookup instead of two on a fresh keyed send; rejected for now because it moves a 409 into the events crate's error vocabulary — revisit if the extra point lookup ever shows in a trace).
- **Patch-vs-refactor verdict:** a **completion** of an existing contract: the daemon half is patched at three points, the browser half is built once.

## Discovery (consult log)

- **Consults (Sep 28, 2026)** — Codex review of PR #717, fifteen findings, verdict "Block merge on making steer delivery replay-safe". Kishore: "Fix all fifteen here"; on the split proposed under `dispatch/write_spec.md` §Authoring discipline ("Security-boundary or backend-heavy follow-ups get their own spec + PR"): **"Fold all fifteen into #717"** — recorded as the override; this spec is the separate file the template's length cap requires, on the same branch and PR. Docs: "Yes, write the docs branch" — `~/Projects/docs` `chore/m207-fleet-chat-streaming-changelog`, PR #206.
- **Findings** — the daemon already accepts `operation_id` (`afd_wire::event::SteerRequest`), validates 1–200 bytes, and deduplicates on `UNIQUE (producer, producer_key)`; no client sends it. `retry.ts` `#mayRetry` replays a dropped socket for every method since `0e802cd4b`. The paused check and `refuse_over_fleet_budget` both run before the INSERT can conflict. Digest drift only warns (`admission_payload_drifted`). The unique key is global across fleets.
- **Metrics review** — no analytics or funnel playbook update required: one operational warn log added, no product event.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
