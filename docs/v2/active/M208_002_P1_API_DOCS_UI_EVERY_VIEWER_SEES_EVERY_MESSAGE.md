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

# M208_002: Every screen watching a fleet shows each typed message, with its sender's name, the moment the daemon accepts it

**Prototype:** v2.0.0
**Milestone:** M208
**Workstream:** 002
**Date:** Sep 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a message sent from another device or by a teammate is missing on every other screen until a reload
**Categories:** API, DOCS, UI
**Batch:** B1 — second of three M208 workstreams in one Pull Request (PR); §1–§3 need nothing from M208_001, §4's names need its members route
**Branch:** `feat/m208-team-accounts`
**Baseline revision:** `3b61121c3c7da8b97cc348cca1d1dbfb99c3bce4`
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M208_001 (`GET /v1/workspaces/{workspace_id}/members` for sender names; memberships for the two-person e2e)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026); decisions in Discovery are Indy's
**Canonical architecture:** `docs/architecture/data_flow.md` §Steer flow end-to-end

---

## Overview

**Goal (testable):** `test_member_sees_teammate_message_then_reply` — Bob types to MARY-001 and John's screen shows "Bob: check the tests" at once, marked waiting, then MARY-001's reply as it streams; Bob's own screen shows "You".
**Problem:** Another screen learns of a turn only from `event_received`, published when a runner leases it (`rustd/crates/afd_fleet/src/lease/bracket.rs`), and that frame carries no text (`rustd/crates/afd_api_tenant/src/handler/stream.rs:89-90`). A registry test on a phone-sent turn left `text: ""` beside the reply. A turn waiting behind a busy fleet appears nowhere else at all, and a page load cannot show it because `core.events` gains the row only at lease.
**Solution summary:** The steer route publishes `event_admitted` (event id, actor, typed message, time) the moment the admission commits (`core.fleet_admissions`, `schema/910_fleet_admissions.sql`, already holds the text). The thread read returns admitted-but-undelivered steers as `queued` rows. `event_received` also carries the message. Every screen renders the turn at once, marked "Waiting for MARY-001" until the fleet picks it up, under the sender's name.

## PR Intent & comprehension handshake

- **PR title (eventual):** the M208 PR (see M208_001)
- **Intent (one sentence):** whoever types, from whichever device, every screen on that fleet shows it immediately and says who typed it.
- **Handshake** — Oct 1, 2026, stated to Indy before EXECUTE: when anyone types to a fleet, every screen on it shows the message at once, marked waiting until a runner picks it up, under "You" or the sender's name. Assumptions stated: "You" compares the session's subject claim with the steer's actor; `Steered` carries the admission instant; queued rows lead the first page, merged by event id; documentation is the OpenAPI stream description and `docs/architecture/data_flow.md`; the command-line client parses no frames. Indy: "Go, R1 after merge (Recommended)".

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer.rs` — the steer route; the admission commits here and the 202 leaves here.
2. `schema/910_fleet_admissions.sql` — the admission ledger: `actor`, `request_json`, `event_created_at`, `delivered_at`.
3. `rustd/crates/afd_fleet/src/lease/bracket.rs` — `publish_received`, the pattern `event_admitted` mirrors.
4. `ui/packages/app/lib/streaming/fleet-stream-frames.ts` — how an opening frame becomes a row.
5. `ui/packages/app/lib/streaming/fleet-stream-held.ts` — the two-tab hold, which must learn the new opening frame.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/src/tail.rs` (+ tests) | EDIT | `EventAdmitted`; `EventReceived.message` |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer.rs` | EDIT | publish `event_admitted` after the admission commits |
| `rustd/crates/afd_api/tests/{integration_fleet_admitted,tenant_plane_suite}.rs`, `rustd/crates/afd_events/tests/integration_steer_{insert,replay}.rs`, `rustd/crates/afd_api/tests/harness/stubs_ingress/answers.rs` | CREATE/EDIT | the admitted frame over live stores; `Steered` and `Repeated` gain the admission instant |
| `rustd/crates/afd_approval/src/inbox/resolve.rs` | EDIT | a continuation's received frame carries no message |
| `rustd/crates/afd_http/src/services/event.rs`, `rustd/crates/afd_events/src/steer.rs` | EDIT | the announce seam on `FleetSteering`; `Steered` carries the admission instant |
| `rustd/crates/afd_tenant/src/sql/member.rs`, `rustd/crates/afd_api_tenant/src/handler/tenant/member.rs`, `rustd/crates/afd_wire/src/team.rs` | EDIT | a workspace member carries `actor`, the string that member's steers record |
| `ui/packages/app/lib/api/tenant-members.ts` | EDIT | `listWorkspaceMembers` |
| `rustd/crates/afd_fleet/src/lease/bracket.rs` | EDIT | `event_received` carries the steer's message |
| `rustd/crates/afd_admission/src/pending.rs` | CREATE | read a fleet's undelivered steer admissions |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message.rs` | EDIT | thread read adds `queued` rows, deduplicated by event id |
| `rustd/crates/afd_api_tenant/src/handler/stream.rs` | EDIT | stream description names `event_admitted` and `message` |
| `public/openapi.json` | EDIT | frame and `queued` status |
| `ui/packages/app/lib/api/{events-types,events}.ts` | EDIT | frame kind, frame and row types |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts` | EDIT | `queued` status constant |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | apply `event_admitted`; `event_received` moves `queued` to `received` and takes `message` |
| `ui/packages/app/lib/streaming/fleet-stream-held.ts` | EDIT | `event_admitted` opens a held turn too |
| `ui/packages/app/lib/streaming/workspace-stream.ts` | EDIT | the wall treats `event_admitted` as a no-op |
| `ui/packages/app/lib/events/event-summary.ts` | EDIT | sender labels; "Waiting for {fleet}" for `queued` |
| `ui/packages/app/components/domain/{FleetThread,FleetMessageRow}.tsx` | EDIT | members' names reach the rows |
| `ui/packages/app/lib/streaming/fleet-stream-admitted.ts`, `ui/packages/app/lib/events/sender-names.ts` | CREATE | the admitted frame's handling, beside `fleet-stream-frames.ts` at its cap; the viewer's and members' labels |
| `ui/packages/app/lib/auth/credential.ts` | EDIT | the session subject, the viewer's own steer actor |
| `rustd/crates/afd_admission/src/{lib,repeat}.rs` | EDIT | the admission answers with its `event_created_at` |
| `rustd/crates/afd_events/src/history/**` | EDIT | the queued rows' shape beside the thread's |
| `docs/architecture/data_flow.md` | EDIT | the admitted frame in the steer flow |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (`event_admitted` and `queued` as one named constant each side), ECL (a publish failure never fails the steer), NDC, FLL.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the thread read's response grows a status value; the change is additive.
- `docs/LOGGING_STANDARD.md` — a failed publish logs once with `event` and ids, never the message text.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS | yes | frame kind and status as constants in `afd_wire` and `events-types.ts` |
| LOGGING | yes | a dropped frame is logged once by the shared publisher (`tail_frame_dropped`, `afd_dragonfly/src/streams/tail.rs:76-91`), as every daemon frame is; no body |
| File & Function Length (≤350/≤50/≤70) | yes — `fleet-stream-frames.ts` is near its cap | admitted-frame handling in a sibling module |

## Prior-Art / Reference Implementations

- **Reference:** `publish_received` in `rustd/crates/afd_fleet/src/lease/bracket.rs` — best-effort publish through `publish_frame`; a lost frame is recovered by backfill.
- **Reference:** `reconcileRows` in `ui/packages/app/lib/streaming/fleet-stream-optimistic.ts` — the one-id merge the queued row joins.

## Sections (implementation slices)

### §1 — Announce a message the moment it is accepted

After the steer's admission commits and before the 202, the route publishes `event_admitted` `{event_id, actor, event_type, message, created_at}`: the logical id the 202 returns, the admission's `event_created_at`, and the typed text (at most `STEER_MESSAGE_MAX_BYTES`). Publishing is best-effort through `FleetStreams::publish_frame`, as every tail frame is: a failure is logged once there and the steer still answers 202. A replayed steer (`replayed: true`) publishes nothing, on either replay path, since its first admission already did.

- **Dimension 1.1** — a steer publishes one `event_admitted` whose id equals the 202's → Test `test_steer_publishes_admitted_frame` — DONE (`afd_api/tests/integration_fleet_admitted.rs`, John's open tail; fails with the publish removed)
- **Dimension 1.2** — a publish failure still answers 202 and logs once → Test `test_admitted_publish_failure_still_accepts` — DONE (`integration_fleet_admitted.rs`, an unreachable queue)
- **Dimension 1.3** — a replayed steer publishes nothing → Test `test_replayed_steer_publishes_nothing` — DONE (`integration_fleet_admitted.rs`)

### §2 — Late joiners see waiting and started turns with their text

The thread read (`GET …/fleets/{fleet_id}/messages`) returns the fleet's receipted, undelivered steer admissions (`receipt IS NOT NULL AND delivered_at IS NULL`, the ledger's own meaning of queued, `schema/910_fleet_admissions.sql:37-39`) as rows with status `queued`, first page only, merged by event id so a leased turn appears once. The read rides `idx_fleet_admissions_undelivered`, which holds only in-flight work; no schema change. `event_received` carries `message`, parsed from `Acquired.request_json` for `steer:*` actors and omitted above the byte cap or when unparsable.

- **Dimension 2.1** — an undelivered steer appears in the thread read as `queued` with its text → Test `test_thread_read_includes_queued_steers`
- **Dimension 2.2** — once leased, the same event id appears once, no longer `queued` → Test `test_thread_read_dedupes_leased_steer`
- **Dimension 2.3** — `event_received` carries a steer's message; a webhook's carries none → Test `test_event_received_carries_steer_message` — DONE (`afd_fleet/src/lease/bracket.rs`)
- **Dimension 2.4** — an unparsable body publishes without `message` and logs once → Test `test_event_received_without_parsable_body` — DONE (`afd_fleet/src/lease/bracket.rs`, with the byte bound at its edge)

### §3 — Every screen renders the turn at once

`event_admitted` creates the row with the typed text and status `queued`, rendered "Waiting for {fleet}". `event_received` for that id moves it to `received` without a second row. The two can arrive in either order: `append` has written the queue entry before the route publishes, so a runner can lease and announce first. An `event_admitted` for a row already held fills only an empty text and never moves the row back to `queued`. The two-tab hold (`HeldTurns`) treats `event_admitted` as an opening frame, so the sending tab still waits for its 202 before deciding whose turn it is. The wall ignores `event_admitted`: a tile changes when work starts.

- **Dimension 3.1** — `event_admitted` renders a `queued` row with the text → Test `test_admitted_frame_renders_queued_row`
- **Dimension 3.2** — `event_received` moves `queued` to `received`, one row → Test `test_received_moves_queued_row`
- **Dimension 3.3** — an own-account `event_admitted` waits on this tab's 202 → Test `test_held_turns_hold_admitted_frames`
- **Dimension 3.4** — the wall ignores `event_admitted` → Test `test_wall_ignores_admitted_frame`
- **Dimension 3.5** — `event_admitted` after `event_received` keeps the row's status, one row → Test `test_late_admitted_frame_keeps_status`

### §4 — Messages carry their sender's name

The viewer's own messages read "You"; a member's read their display name from `GET /v1/workspaces/{workspace_id}/members` (M208_001); any other actor, and a member with no display name, keeps today's label (`senderLabelFor`). No account identifier ever renders. The route's `user_id` is `core.users.id` (`afd_tenant/src/sql/member.rs:8`) while a steer records `steer:<oidc_subject>` (`message_steer.rs:226`), so each item gains `actor`, the string that member's steers record. Nothing new is exposed: every event row a member reads already carries it.

- **Dimension 4.1** — viewer, member and unknown actors label as "You", the name, and the existing fallback → Test `test_sender_labels_name_members`
- **Dimension 4.2** — two people on one fleet each see the other's message, named, then the reply → Test `test_member_sees_teammate_message_then_reply`
- **Dimension 4.3** — a workspace member item carries `actor` equal to that member's steer actor → Test `test_workspace_members_carry_actor`

### §5 — Documentation

The stream description and OpenAPI name `event_admitted` and `message`; `docs/architecture/data_flow.md` shows where it fires. The public API reference renders the stream description from OpenAPI; no other public page documents frames (Discovery).

- **Dimension 5.1** — every `TailFrame` kind appears in the stream description → Test `test_stream_description_names_every_frame`

## Interfaces

```
event_admitted  {kind:"event_admitted", event_id, actor, event_type, message, created_at}
                 event_id = the 202's event_id; message <= STEER_MESSAGE_MAX_BYTES; steers only
event_received  gains  message?   (steer:* only; omitted above the cap or when unparsable)
GET …/fleets/{fleet_id}/messages  rows may carry status:"queued" (admitted, not yet picked up)
GET /v1/workspaces/{workspace_id}/members  items gain  actor   ("steer:<oidc_subject>")
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Publish lost | pub/sub unavailable | steer still 202; other screens show the turn at `event_received` or on reload |
| Frame beats the 202 | admission publish reaches the sender tab first | `HeldTurns` holds it; it lands on the sender's row after the 202 |
| Reconnect gap | a screen was offline at admission | the reconnect backfill reads the events list (`fleet-stream-backfill.ts:89`), which holds no admissions: the turn reappears at its `event_received`, and a reload shows it `queued` |
| Admitted after received | a runner leased the turn before the route published | the row keeps `received`; the frame fills only an empty text (3.5) |
| Replayed steer | same operation id retried | no second frame; the one row stands |
| Oversized or unparsable body | legacy or corrupt row | frame without `message`; row text as today; one warn |
| Fleet never picks it up | fleet paused or no runner | the row stays "Waiting for {fleet}" on every screen, matching the ledger |

## Invariants

1. A turn has one event id from admission to completion — `event_admitted`, the 202, `event_received` and the thread read all use the admission's logical id; tested end to end in 2.2 and 3.2.
2. A publish never decides a steer — the route answers from the admission commit; asserted by 1.2.
3. `message` never exceeds `STEER_MESSAGE_MAX_BYTES` — the publisher checks length and omits above it; unit-tested at the boundary.
4. No account identifier renders as a sender — `senderLabelFor`'s opaque-id guard stays, tested in 4.1.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tail_frame_dropped` (existing, shared by every daemon frame) | ops | the admitted frame could not be published | fleet id, reason | never the message | `test_admitted_publish_failure_still_accepts` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_steer_publishes_admitted_frame` | steer "hi" → one frame, `event_id` equals the 202's, `message:"hi"` |
| 1.2 | integration | `test_admitted_publish_failure_still_accepts` | unreachable queue → 202, one `tail_frame_dropped`, no text in the log |
| 1.3 | integration | `test_replayed_steer_publishes_nothing` | same operation id twice → one frame |
| 2.1 | integration | `test_thread_read_includes_queued_steers` | undelivered admission → row `queued` with text |
| 2.2 | integration | `test_thread_read_dedupes_leased_steer` | lease it → one row, not `queued` |
| 2.3 | unit | `test_event_received_carries_steer_message` | `steer:u1` → `message`; `webhook:gh` → no key |
| 2.4 | unit | `test_event_received_without_parsable_body` | `request_json:"{"` → no `message`, one warn |
| 3.1 | unit | `test_admitted_frame_renders_queued_row` | frame → row `queued`, text set, label "Waiting for MARY-001" |
| 3.2 | unit | `test_received_moves_queued_row` | admitted then received → one row, `received` |
| 3.3 | unit | `test_held_turns_hold_admitted_frames` | own-account admitted while waiting → held until the 202 |
| 3.4 | unit | `test_wall_ignores_admitted_frame` | wall state unchanged by the frame |
| 3.5 | unit | `test_late_admitted_frame_keeps_status` | received then admitted → one row, `received`, text filled |
| 4.1 | unit | `test_sender_labels_name_members` | viewer, member, `steer:api`, unknown → "You", name, "API", fallback |
| 4.2 | e2e | `test_member_sees_teammate_message_then_reply` | two signed-in members on one fleet: each sees the other's text, named, then the reply |
| 4.3 | integration | `test_workspace_members_carry_actor` | member with subject `user_b` → item `actor:"steer:user_b"` |
| 5.1 | unit | `test_stream_description_names_every_frame` | every `TailFrame` kind string is in the description |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Two people see each other's messages (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys -g test_member_sees_teammate_message_then_reply` | `1 passed` | P0 | post-merge (Indy, Discovery) |
| R2 | Admitted frame and queued rows (§1, §2) | `make test-integration-rustd` | exit 0 | P0 | |
| R3 | Rows render once, held when owed (§3) | `cd ui/packages/app && bunx vitest run lib/streaming` | exit 0 | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Versions in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears verbatim above (`make test-integration-rustd` is R2). See `dispatch/lifecycle.md` for timing.

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results. **Ship gate:** any ❌ returns to EXECUTE; a P1 ❌ needs an Indy-acked deferral quote; a P0 is MOVED only into a named successor that carries it, with Indy's quote, and is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- A queued message's edit or cancel — sent is sent, as today.
- Typing indicators and read receipts.
- Pushing turns to people who are not watching (notifications).

---

## Product Clarity (authoring record)

1. **Successful user moment** — Bob types "check the tests"; John's screen shows "Bob: check the tests · Waiting for MARY-001" at once, then MARY-001's answer growing under it.
2. **Preserved user behaviour** — the sender's own row, Resend, the pending-sends ledger, the anchor and the two-tab hold behave as today.
3. **Optimal-way check** — this is the direct shape: the text is already durable at admission, so the frame announces a fact instead of a guess.
4. **Rebuild-vs-iterate** — iterate: one new frame, one field, one union in the thread read.
5. **What we build** — `event_admitted`, `message` on `event_received`, `queued` rows, names on messages.
6. **What we do NOT build** — edit or cancel, typing indicators, notifications.
7. **Fit with existing features** — the M207 chat's hold and anchor keep deciding whose run it is; must not destabilize the steer path's 202.
8. **Surface order** — User Interface (UI) first; the frame is public and documented for API consumers.
9. **Dashboard restraint** — "Waiting" shows only while the ledger says undelivered; no invented progress.
10. **Confused-user next step** — a message stuck "Waiting for MARY-001" points at the fleet's status, which says whether it is paused or has no runner.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** announce at admission (§1), make page loads agree (§2), render (§3), name (§4), document (§5).
- **Alternatives considered:** a client detail read per foreign turn (rejected: one extra read per viewer per turn, and still nothing before the lease); writing `core.events` at admission (rejected: moves the row's authority out of the lease, a far larger change).
- **Patch-vs-refactor verdict:** this is a **patch**: the admission ledger already holds everything the frame needs.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, Indy: "showing a message before the fleet picks it up, which needs a send-time broadcast is also a must have since then i can test John invited to Bob and they both see the same fleet. Bob types a message and John can see the typed message and response." Source findings: `event_received` has no body (`handler/stream.rs:89-90`) and fires at lease (`lease/bracket.rs`); `core.fleet_admissions` holds `request_json` and commits before the 202 (`schema/910_fleet_admissions.sql`).
- **Source corrections** — Sep 30, 2026, read before CHORE(open): sender names need a join key the members route lacked (§4, Dimension 4.3); `event_admitted` can trail `event_received` (§3, Dimension 3.5); queued rows ride the existing undelivered index (§2); a dropped frame is logged by the shared publisher, not a new warn (§1); a reconnect restores a waiting turn only at its lease (Failure Modes). `orly gate work` allows one active spec per worktree, so M208_001 closes before this opens (Aiwa's call while Indy was away; the PR is one either way).
- **Metrics review** — pending.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
- **PLAN source corrections** — Oct 1, 2026: no "You" label exists; every `steer:*` actor reads "Operator" (`ui/packages/app/lib/events/event-summary.ts:51,101`), so §4 learns the viewer's actor from the session's subject claim (`lib/auth/credential.ts`), which `steer:<oidc_subject>` records (`message_steer.rs:219-229`, `schema/220_users.sql`). The admission answers with id, digest and fleet only (`afd_admission/src/repeat.rs:20-28`), so it gains `event_created_at` for the frame. The thread reads newest first (`afd_events/src/history/statement.rs:92`), so queued rows lead the first page. `~/Projects/docs` holds no frames page (`event_received` appears only in `changelog.mdx`), so §5 drops that row. The command-line client parses no frames (`cli/src`, no `event_received`).
- **R1 after merge** — Oct 1, 2026, Indy via AskUserQuestion: "Go, R1 after merge (Recommended)". The journey calls the shared dev API, which runs `main`; the Pull Request opens under an Orly-Override Indy records, and Dimension 4.2 is graded after merge.
