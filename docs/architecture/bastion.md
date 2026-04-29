# Bastion — the post-MVP shape

> Parent: [`ARCHITECHTURE.md`](../ARCHITECHTURE.md)

Where the v2 wedge points after launch. Not part of v2; documented here so spec authors can avoid foreclosing it.

The MVP wedge ships an internal-only diagnosis posted to the operator's Slack. The longer-term play is the **bastion** — a single durable surface where:

- internal triage continues as today (Slack post, evidence trail, follow-up steers)
- external customer communication is automatically derived from the same incident state (status-page updates, broadcast email or SMS, embedded `<iframe>` widgets)
- the same zombie owns both — the diagnosis and the customer-facing narrative come from one event log, not two

This is post-MVP. It's the shape that competes structurally with Atlassian Statuspage's manual-update model and with the AI-statuspage-automation tools (Dust, Relevance AI, PageCalm) that bolt language models onto an external status-page product.

## What changes structurally to get from MVP to bastion

1. **Per-zombie audience routing.** `TRIGGER.md` / `x-usezombie:` adds `audiences: [internal_slack, customer_status, customer_email]`. The zombie's `SKILL.md` prose teaches it to draft different summaries per audience from the same evidence.
2. **Status-page rendering surface.** A hosted page at `status.<customer-domain>` renders the latest `processed` event's customer-facing summary. Updates as new events land.
3. **Broadcast channels.** The zombie's `tools:` list grows to include `email_send`, `sms_send` (gated, approval-required for the first incident), `webhook_post` (for downstream Statuspage / PagerDuty / etc.).
4. **Approval gating per audience.** The `SKILL.md` prose can require human approval before posting to customer-facing audiences while letting internal Slack go through automatically. The M47 approval inbox handles the mechanic.
5. **Retention and replay for compliance.** Customer-facing communications have stricter retention requirements (Sarbanes-Oxley, General Data Protection Regulation). `core.zombie_events` retention policy becomes per-actor configurable.

## What does not change

- The runtime architecture (worker → session → streaming).
- The sandbox boundary (Landlock + cgroups + bwrap).
- The trigger model (webhook, cron, steer).
- The credential vault, network policy, budget caps, context lifecycle.

The bastion is a `SKILL.md` authoring pattern plus a few new tool primitives plus a new rendering surface. It is not a different product. The MVP's job is to earn enough trust on internal-only diagnoses that customers feel safe letting the same zombie talk to *their* customers.
