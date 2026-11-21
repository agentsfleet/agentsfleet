# M253_003 Section 5: A settled turn's answer can be emailed — DEFERRED

**Status:** DEFERRED (parked Nov 21, 2026). Reopen when the owner wants scheduled and webhook-triggered fleets to reach people outside the dashboard.

> Owner (Indy, Nov 21, 2026): "I want to defer or park this 5. so 1, 2, 3, 4 will close M252_003" — context: the use case is a fleet nobody watches (an outage watch on a schedule) whose answer has no connector thread; email to the owner and verified members was the answer. Decided with no retry: a failed send would be final.

The original Section, kept for reference:

## §5 — A settled turn's answer can be emailed

A fleet definition may list `x-agentsfleet.deliveries` entries of `{"type":"email","to":[…]}`. Each recipient is `owner` or `member:<user_id>`, with no repeats and no count cap. The save path checks each one against tenant membership and refuses a stranger with `DELIVERY_RECIPIENT_REFUSED`.

When a turn settles answered or failed, the report owes one delivery per recipient, with provider `email` and the user id as destination. `afd_outbound` routes it through a `Channel` enum, Slack or Email, and `Posters` gains an email poster.

The poster checks membership again, looks up the address, and renders `templates/answer.{html,txt}`. The subject is the fleet name and the answer's first line. The body is the answer, and the link is `{dashboard}/w/{workspace_id}/fleets/{fleet_id}`.

It sends through the invite relay with idempotency key `delivery-{delivery_id}`. SMTP 4xx replies retry on today's jittered schedule. A 5xx reply abandons the delivery, and so does an unconfigured relay, with `MAIL_RELAY_UNCONFIGURED`.

- **Dimension 5.1** — the outage email arrives → Test `a_scheduled_turns_answer_is_emailed_to_the_owner` (integration): a schedule turn answering "5xx 4.1% since 08:42" with `owner` as recipient. Expected: one message to the owner holding that answer and the fleet's link.
- **Dimension 5.2** — a stranger cannot be a recipient → Test `saving_a_fleet_that_emails_a_non_member_is_refused` (integration): `member:<uuid of a user outside the tenant>`. Expected: 422 with `DELIVERY_RECIPIENT_REFUSED`.
- **Dimension 5.3** — a removed member gets nothing → Test `a_recipient_removed_before_sending_is_not_emailed` (integration): remove the member, then settle. Expected: the delivery is abandoned with `DELIVERY_RECIPIENT_REFUSED`, and nothing is sent.
- **Dimension 5.4** — a relay outage retries → Test `an_smtp_4xx_retries_then_abandons` (integration): a relay answering 451 every time. Expected: retries on the jittered schedule, then the delivery is abandoned.
- **Dimension 5.5** — one email per delivery → Test `a_redelivered_email_carries_one_idempotency_key` (integration): a send that times out and is retried. Expected: both tries carry `delivery-{delivery_id}`.

