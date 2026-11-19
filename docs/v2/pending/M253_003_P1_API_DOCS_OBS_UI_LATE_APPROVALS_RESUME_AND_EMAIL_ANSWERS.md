<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M253_003: A late approval resumes its turn, each effect lands once, and answers can be emailed

**Prototype:** v2.0.0

**Milestone:** M253

**Workstream:** 003

**Date:** Nov 19, 2026

**Status:** PENDING

**Priority:** P1, a person who approves late or waits for an email is the customer this fleet serves

**Categories:** API, DOCS, OBS, UI

**Batch:** B3, after M253_002

**Depends on:** M253_002, whose turns, runs and deliveries this builds on

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 19, 2026) from Indy's decisions of Nov 19, 2026

**Canonical architecture:** `docs/architecture/runner_fleet.md` for continuations; `docs/architecture/billing_and_provider_keys.md` for the reservation removed here

---

## Overview

**Goal (testable):** An approval given after its deadline resumes the waiting turn, and the resumed run's message effect is posted exactly once.

**Problem:**
- **Late approvals fail.** The expiry sweep closes an expired approval (`afd_approval/src/sql/expire_gates.sql:1-6`). After that, `resolve_gate.sql:9` matches only `resolved_at IS NULL`, so a person who approves two hours later gets nothing.
- **Effects can double.** A resumed run can post a message or schedule twice, because effects carry no key (`af_wire/src/message_verb/mod.rs:41`).
- **Answers only go back.** A scheduled turn has no connector to answer, so nobody hears about an outage it found.

**Solution summary:**
- **One human-input primitive.** It covers both `ask_user` and approvals: each is an approval item, one fenced verb answers it, and the answer admits one continuation turn.
- **Effect keys.** Every daemon effect is keyed by its item, so a resumed run's repeat finds the first send.
- **No reservation.** The durable-task reservation is removed.
- **Email delivery.** A new email delivery lane sends a turn's answer to the fleet owner and verified members.

**Design flow:**
1. **Wait.** A run's approval or question ends it waiting on one item. The report stores that item as an open approval, and the turn waits.
2. **Answer.** The person answers before or after the deadline. The answer verb resolves the approval item under a lock and writes one continuation turn.
3. **Resume.** The continuation's run resumes from the checkpoint and receives the answer as that item's result.
4. **Deliver.** A settled turn owes a delivery to each configured lane. The email lane takes it and sends it through the `smtp-relay` bag (`afd_mail/src/lib.rs:1-12`).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m253): late approvals resume, effects land once, email delivery
- **Intent (one sentence):** A person who answers late still gets the work done, never twice, and hears about it by email.
- **Authoring handshake:**
  - **What Indy asked for:** "Late human approvals remain valid", "One primitive (Recommended)", and email for the outage watch, to "Fleet owner, verified members".
  - **ASSUMPTIONS I'M MAKING:**
    1. **One continuation per approval.** An approval item produces at most one continuation turn, whichever answer wins its lock.
    2. **Late means after the deadline only.** A late approval is valid until the fleet is archived. A denial or an archive closes the approval.
    3. **Email recipients.** They are checked at save and again when the delivery is owed, because membership can change.
    4. **Usage at report.** Removing the reservation leaves the usage check at report, as M253_001 does for setup.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_approval/src/sql/resolve_gate.sql` — the statement whose `resolved_at IS NULL` refuses a late answer.
2. `rustd/crates/daemon/afd_approval/src/sql/expire_gates.sql` — the sweep that marks an approval expired and releases its turn.
3. `rustd/crates/daemon/afd_outbound/src/lib.rs` — the delivery worker, lanes and posters that the email lane joins.
4. `rustd/crates/daemon/afd_mail/src/lib.rs` — templates and the SMTP relay, today used by invites.
5. `rustd/crates/daemon/afd_fleet/src/task/decision/sql/reserve_task.sql` — the reservation removed in §4.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/*_fleet_approval_gates.sql` | DELETE | an approval becomes an `approval` item in `thread.items` |
| `rustd/crates/daemon/afd_approval/src/**` | EDIT | one answer verb; continuation written in its transaction |
| `rustd/guffbolt/src/tools/ask.rs` | EDIT | the question carries the approval's form |
| `rustd/crates/runner/afr_tools/src/**` | EDIT | `request_approval` and `ask_user` share the waiting path; effect keys on message, schedule and cron calls |
| `rustd/crates/protocol/af_wire/src/message_verb/**`, `schedule_verb/**` | EDIT | each request names its `item_id` |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/**` | EDIT | effects deduplicate by key |
| `schema/*_items.sql` | EDIT | an effect's result on its item; approval fields on `approval` items |
| `rustd/crates/daemon/afd_fleet/src/**` message verb | EDIT | the message verb reads and writes the effect's item |
| `rustd/crates/daemon/afd_fleet/src/task/**` and the run statements | EDIT | the reservation and its reads removed |
| `rustd/crates/daemon/afd_fleet/src/task/decision/sql/reserve_task.sql` | DELETE | the reservation |
| `rustd/crates/protocol/af_fleet_runtime/src/definition/**` | EDIT | an `email` delivery entry |
| `rustd/crates/daemon/afd_outbound/src/email/**` | CREATE | the email poster |
| `rustd/crates/daemon/afd_outbound/src/lanes/dispatch/**` | EDIT | the `email` arm |
| `rustd/crates/daemon/afd_mail/src/**`, `templates/answer.{html,txt}` | EDIT, CREATE | the answer letter and its rendering |
| `rustd/crates/foundation/af_core/src/error_code/**` | EDIT | codes for a refused recipient and an unanswerable approval, with registry and problem rows |
| `ui/packages/app/**` approval card | EDIT | an expired approval still offers Approve and Deny |
| `docs/ux.html` | EDIT | the approval card's late state |
| `public/openapi.json` | EDIT | regenerated |
| `docs/architecture/runner_fleet.md`, `billing_and_provider_keys.md` | EDIT | continuations; no reservation |
| `docs/v2/pending/M253_003_P1_API_DOCS_OBS_UI_LATE_APPROVALS_RESUME_AND_EMAIL_ANSWERS.md` | CREATE | this spec, moved to `active/` then `done/` |
| `CHANGELOG.md` | EDIT | late approvals and email delivery |
| `~/Projects/docs` branch `chore/m253-late-approvals-email-changelog` | EDIT | approvals and delivery pages |

## Applicable Rules

- **`domains/sql.md`** — schema edited in place before the first production deploy; statuses bound as named constants
- **`domains/http.md`** — HTTP-VERB (an answer is a `PATCH` of the approval item's status), HTTP-IDEMKEY
- **`domains/auth.md`** — an approval is answered only by a member of its workspace; recipients come from verified members
- **`domains/logging.md`** — no email address in a log line; `error_code` on every refusal
- **`languages/rust.md`** — ERR-RS, RS-MAPERR
- **`tests.md`** — a negative test per failure mode; integration tests across the report and answer paths

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | clippy, TypeScript and source rules |
| error-code registry check | yes | each new code lands with its family const, `REGISTRY` row and `problem/` entry |
| `docs.language` | yes | changed pages in short sentences |
| gitleaks | yes | the relay credential stays in the vault bag |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the email poster mirrors the Slack poster's split |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/daemon/afd_outbound/src/slack/**` — mirror the poster: take the lane head, send, then stamp, release or abandon by holder token.
- **Reference:** `rustd/crates/daemon/afd_mail/src/invite/**` — mirror the askama letter and the relay mailer.
- **Reference:** `~/Projects/oss/rs/codex/codex-rs` — an approval is an item the turn waits on, and the answer resumes the same turn. Diverge: here the answer admits a continuation turn, because the run has ended.

## Sections (implementation slices)

### §1 — One primitive for every question a run asks a person

`ask_user` and `request_approval` both end the run as guffbolt's `Waiting`. The report stores that call's item as an open approval, with its question, evidence and deadline.

The turn's status becomes `waiting`. The approval card in the conversation shows the question with Approve and Deny, or an answer box for a plain question.

- **Dimension 1.1** — a question becomes an approval item → Test `an_ask_user_ending_stores_an_open_approval_item` (integration): a run ending waiting on its `ask_user` call. Expected: one open `approval` item for that call, and the turn `waiting`.
- **Dimension 1.2** — both tools share the path → Test `request_approval_and_ask_user_both_end_waiting` (unit): each tool in turn. Expected: both end the loop as `Waiting` on their own call.

### §2 — An answer, early or late, admits exactly one continuation

`PATCH …/turns/{turn_id}/items/{item_id}` with `approved`, `denied` or an answer resolves the approval item. It works for an open approval and for one the sweep marked `expired`.

Its transaction locks the item row, records the answer, and writes one continuation turn on the thread. It refuses only an approval already answered, or one whose fleet is archived.

The sweep keeps marking approvals `expired`, so the card can say "Deadline passed" while still offering both buttons.

- **Dimension 2.1** — a late approval resumes → Test `an_approval_after_the_deadline_resumes_the_turn` (integration): an approval expired 2 hours ago, then approved. Expected: one continuation turn, and its run receives "approved" as that item's result.
- **Dimension 2.2** — two answers make one continuation → Test `two_answers_to_one_gate_write_one_continuation` (integration): approve and deny race. Expected: one wins, the other gets 409, and one continuation exists.
- **Dimension 2.3** — an archived fleet's approval closes → Test `an_answer_to_an_archived_fleets_approval_is_refused` (integration): archive, then approve. Expected: 409 with the archived code, and no turn written.
- **Dimension 2.4** — another workspace cannot answer → Test `a_member_of_another_workspace_cannot_answer_a_gate` (integration): a token for workspace B. Expected: 404.
- **Dimension 2.5** — the card stays answerable → Test `an_expired_approval_card_still_offers_approve_and_deny` (e2e): a Playwright run of the conversation with an expired approval. Expected: "Deadline passed" with both buttons enabled.

### §3 — Each effect lands once, however often its run resumes

A message, schedule or cron call is an item, and the item is its key. An item is unique per turn and the provider's id for the call, which the resumed run reuses from its checkpoint.

The effect's verb records its result on the item. A repeat finds the item done, moves it to the resuming run, and answers the stored result without a second effect. A holder below the current fence is refused first.

- **Dimension 3.1** — a repeated message posts once → Test `a_message_effect_repeated_by_a_resumed_run_posts_once` (integration): run 1 posts through one call and dies. Run 2 at fence +1 repeats that call. Expected: one Slack post, one item owned by run 2, and both answers identical.
- **Dimension 3.2** — a different call is a different effect → Test `two_calls_in_one_turn_post_two_messages` (integration): two different calls in one turn. Expected: two items and two posts.
- **Dimension 3.3** — a superseded holder posts nothing → Test `an_effect_from_a_superseded_fence_is_refused` (integration): a send at fence 1 after fence 2 was assigned. Expected: 409, and no post.

### §4 — A durable task reserves nothing

`reserve_task.sql` and every read of `reserved_model_nanos` go. They sit in assignment, metering, the decision budget, task runs and report rows. A task's slice is checked against the wallet at assignment and charged at report, as a turn is.

- **Dimension 4.1** — no reservation remains → Test `a_task_assignment_reserves_no_model_nanos` (integration): assign a task slice. Expected: the wallet's reserved amount is unchanged, and the slice runs.
- **Dimension 4.2** — an empty wallet still stops a task → Test `a_task_slice_on_an_empty_wallet_is_not_assigned` (integration): a zero balance. Expected: no assignment, and the task stays queued with the balance code.

### §5 — A settled turn's answer can be emailed

A fleet definition may name an `email` delivery with its recipients. At save, every recipient must be the fleet owner or a verified member of the fleet's workspace. When a turn settles, the report owes one delivery per recipient, with provider `email`.

The email lane takes it and renders the answer letter from `templates/answer.{html,txt}`. It sends the letter over the `smtp-relay` bag's relay. The delivery is stamped, retried with jitter, or abandoned, as Slack deliveries are.

- **Dimension 5.1** — the outage email arrives → Test `a_scheduled_turns_answer_is_emailed_to_the_owner` (integration): a schedule turn answering "5xx 4.1% since 08:42" with the owner as recipient. Expected: one SMTP message to the owner whose body holds that answer and the fleet's link.
- **Dimension 5.2** — a stranger cannot be a recipient → Test `saving_a_fleet_that_emails_a_non_member_is_refused` (integration): recipient `x@example.com`, not a member. Expected: 422 naming the recipient field.
- **Dimension 5.3** — a removed member gets nothing → Test `a_recipient_removed_before_settlement_is_not_emailed` (integration): remove the member, then settle. Expected: no delivery for them, and one `delivery_recipient_dropped` event.
- **Dimension 5.4** — a relay outage retries → Test `an_smtp_relay_refusal_retries_then_abandons` (integration): a relay answering 451 every time. Expected: jittered retries, then the delivery is abandoned with its code.
- **Dimension 5.5** — a repeated delivery sends once → Test `a_delivery_taken_twice_sends_one_email` (integration): two workers take one lane. Expected: one SMTP message.

## Interfaces

- **Gate answer:** `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}/items/{item_id}` with `{"status":"approved"}`, `{"status":"denied"}`, or `{"status":"answered","answer":"…"}`. It answers 200 with the item and its `continuation_turn_id`.
- **Effect key:** message, schedule and cron requests from the runner carry `{"item_id":"019f…"}`, beside the run's fence.
- **Fleet definition:** `x-agentsfleet.deliveries` accepts `{"type":"email","to":["owner"]}` or `{"type":"email","to":["member:019f…"]}`. Recipients are named by role or user id, never by address, so the definition holds no email address.
- **Delivery lane:** provider `email`, destination the user id. The address is looked up at send time from `identity.users`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Late approval | failure | an answer after the deadline | resolved; one continuation | `an_approval_after_the_deadline_resumes_the_turn` |
| Racing answers | race | approve and deny together | one wins; the other gets 409 | `two_answers_to_one_gate_write_one_continuation` |
| Archived fleet | failure | an answer after archive | 409 | `an_answer_to_an_archived_fleets_approval_is_refused` |
| Foreign answer | security | another workspace's token | 404 | `a_member_of_another_workspace_cannot_answer_a_gate` |
| Repeated effect | replay | a resumed run repeats a call | the first result, no second effect | `a_message_effect_repeated_by_a_resumed_run_posts_once` |
| Stale effect | replay | a superseded holder | 409 | `an_effect_from_a_superseded_fence_is_refused` |
| Empty wallet | exceeded quota | no balance for a task slice | not assigned; queued | `a_task_slice_on_an_empty_wallet_is_not_assigned` |
| Stranger recipient | security | a non-member address | 422 at save | `saving_a_fleet_that_emails_a_non_member_is_refused` |
| Removed member | security | membership ends before settlement | no delivery; event logged | `a_recipient_removed_before_settlement_is_not_emailed` |
| Relay down | dependency unavailable | SMTP 4xx | jittered retry, then abandon | `an_smtp_relay_refusal_retries_then_abandons` |
| Double take | race | two workers on one lane | one email | `a_delivery_taken_twice_sends_one_email` |

## Invariants

1. An approval item admits at most one continuation. The answer statement updates the item `WHERE answered_at IS NULL`, and the continuation insert reads that row in one transaction.
2. An effect lands once. A unique index on `items (turn_id, provider_call_id)` refuses a second item, and the verb reads back the first result.
3. A definition holds no email address — the delivery entry's type accepts only `owner` or `member:<uuid>`, so an address does not parse.
4. No reservation reads remain — Dead Code Sweep's `reserved_model_nanos` row runs in the rubric.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `approval_answered` | product | a person answers an approval item | `item_id`, status, late (bool), delay | no answer text | `an_approval_after_the_deadline_resumes_the_turn` |
| `effect_deduplicated` | operator | a repeated call finds its item done | `run_id`, `item_id`, effect kind | none needed | `a_message_effect_repeated_by_a_resumed_run_posts_once` |
| `delivery_recipient_dropped` | operator | a recipient is no longer a member | `delivery_id`, `user_id` | no email address | `a_recipient_removed_before_settlement_is_not_emailed` |
| `email_delivered` | product | the relay accepts the letter | `delivery_id`, `fleet_id`, try number | no address, no body | `a_scheduled_turns_answer_is_emailed_to_the_owner` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A late approval resumes the turn (§2) | `make test-integration-rustd` | `an_approval_after_the_deadline_resumes_the_turn` passes | P0 | |
| R2 | A resumed run's effect lands once (§3) | `make test-integration-rustd` | `a_message_effect_repeated_by_a_resumed_run_posts_once` passes | P0 | |
| R3 | No reservation remains (§4) | `git grep -n -w -E "reserved_model_nanos|reserve_task"` | no output | P0 | |
| R4 | The outage answer reaches the owner by email (§5) | `make test-integration-rustd` | `a_scheduled_turns_answer_is_emailed_to_the_owner` passes | P0 | |
| R5 | A stranger cannot be emailed (§5) | `make test-integration-rustd` | `saving_a_fleet_that_emails_a_non_member_is_refused` passes | P0 | |
| R6 | The expired card still answers (§2) | `make dry-app` | `an_expired_approval_card_still_offers_approve_and_deny` passes | P1 | |
| S1 | Declared commands green | `make lint-all` | exit 0 | P0 | |
| S2 | Declared commands green | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared commands green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared commands green | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `reserve_task.sql` | `git grep -n -w "reserve_task"` | no output |
| `reserved_model_nanos` | `git grep -n -w "reserved_model_nanos"` | no output |

## Out of Scope

- Inbound email (E2): not planned.
- Email to addresses outside the workspace: not planned, per Indy's "Fleet owner, verified members".

---

## Product Clarity (authoring record)

1. **Successful user moment** — At 09:00 the outage fleet finds 5xx at 4.1%, and the owner reads "Outage: 5xx 4.1% since 08:42, likely deploy #812" in their inbox.
2. **Preserved user behaviour** — Approvals before the deadline, Slack answers, and task budgets work as before.
3. **Optimal-way check** — The email lane reuses the outbound worker and the invite relay; nothing shorter sends mail safely.
4. **Rebuild-vs-iterate** — Iterate: approvals, the outbound worker and the mailer exist, and each gains one path.
5. **What we build** — one answer verb, the continuation, effect keys, the reservation's removal, and the email lane.
6. **What we do NOT build** — inbound email, and arbitrary recipients, because Indy limited recipients to members.
7. **Fit with existing features** — compounds with M247 respawn, which resumes the continuation from its checkpoint; it must not destabilise Slack delivery, which shares the worker.
8. **Surface order** — API and worker first, then the card's late state.
9. **Dashboard restraint** — no new screens; the card gains a "Deadline passed" line.
10. **Confused-user next step** — a refused recipient names the field and says recipients must be workspace members.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** questions first (§1), answers and continuations (§2), effect keys (§3), reservation removal (§4), then email (§5).
- **Alternatives considered:**
  - **Reopen the expired run instead of a continuation.** A finished run has released its fence, and reusing it would break fencing, so it was rejected.
  - **Free-form email addresses.** That conflicts with Indy's recipient decision and puts addresses in definitions, so it was rejected.
- **Patch-vs-refactor verdict:** a **patch** because each piece extends an existing path.

## Discovery (consult log)

- **Consults** — Indy's decisions, quoted:
  - > Owner (Indy, Nov 19, 2026): "Late human approvals remain valid." — context: §2.
  - > Owner (Indy, Nov 19, 2026): "One primitive (Recommended)" — context: §1, one human-input primitive.
  - > Owner (Indy, Nov 19, 2026): "Fleet owner, verified members (Recommended)" — context: §5 recipients.
  - > Owner (Indy, Nov 19, 2026): "W1 setup context (Recommended), E1 email delivery (Recommended)" — context: E1 is in scope, E2 is not.
  - > Owner (Indy, Nov 19, 2026): "Accept all (Recommended)" — context: decisions D4–D10 and D12.
    - **D5:** remove `reserve_task`.
    - **D9:** an idempotency key per call on message, schedule and cron.
  - > Owner (Indy, Nov 19, 2026): "donot do the lint, test-unit, test-integration check but rather do it at the end of every workstream. but for every dimension do cargo check, cargo fmt (clippy) so we know code compiles." — context: per-Dimension compile checks; S rows at the end of each Workstream.
  - > Owner (Indy, Nov 19, 2026): "Ensure the rust code for individual files is 99% and overall is 99% if possible or else stick to the numbers we have currently." — context: coverage targets 99% per file and overall, never below the 98% line and 95% patch floors.
- **Consults, agent defaults** — the approval answer is a `PATCH` of its item's status, per HTTP-VERB. Recipients are named by role or user id, so no address sits in a definition.
- **Model reviews** — none yet; `egm-review` is invoked by Indy.
- **Open blockers** — none.
- **D9 amended** — D9 named the key "(lease, fence, call number)", which a resumed run at fence +1 never matches. An effect is keyed by its item, and a superseded fence is refused separately.
  - > Owner (Indy, Nov 19, 2026): "Key by the item (Recommended)" — context: §3 effect keys.
- **Deferrals and transfers** — none.
