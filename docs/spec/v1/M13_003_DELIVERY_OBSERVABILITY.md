# M13_003: Delivery Observability

**Prototype:** v1.0.0
**Milestone:** M13
**Workstream:** 003
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P1 — observability is scope, not afterthought
**Batch:** B2 — parallel with M13_002 after M13_001
**Depends on:** M13_001 (delivery_state transitions to observe)

---

## Problem

The M9 scoring engine scores run quality (completion, error rate, latency, resource) but has no signal on delivery outcome. Did the PR get merged? How long did it take? Was it revised? Without delivery observability, the "Agent SLA" vision ("95% of runs score Gold+ within 3 attempts" → "85% of PRs merged within 48 hours") has no data foundation.

---

## 1.0 PostHog Delivery Events

**Status:** PENDING

Emit PostHog events for every delivery state change (full set per CEO Review Issue 16, Option A).

**Dimensions:**
- 1.1 Emit `pr.opened` event when `delivery_state` is set to `PR_OPEN` in `handleDoneOutcome()`. Properties: `run_id`, `workspace_id`, `agent_id`, `pr_url`, `distinct_id` (Clerk user ID from `requested_by`).
- 1.2 Emit `pr.approved`, `pr.changes_requested`, `pr.ci_failed` events when webhook handler updates `delivery_state`. Properties: `run_id`, `workspace_id`, `delivery_state`, `pr_url`.
- 1.3 Emit `pr.merged` event when `delivery_state` → `MERGED`. Properties include `time_to_merge_ms` (computed in §4.0). This is the primary delivery SLA signal.
- 1.4 Emit `pr.closed` event when `delivery_state` → `CLOSED` (not merged). Properties include `time_to_close_ms` for abandonment analysis.

---

## 2.0 Prometheus Webhook Metrics

**Status:** PENDING

Add counters and histograms for webhook operations to the existing Prometheus `/v1/metrics` endpoint.

**Dimensions:**
- 2.1 `zombie_webhooks_received_total` — Counter, labels: `event_type` (pull_request, pull_request_review, check_suite), `status` (processed, filtered, rejected). Incremented in webhook handler.
- 2.2 `zombie_webhooks_filtered_total` — Counter, labels: `reason` (non_zombie, orphaned, already_terminal, cross_workspace). Incremented on each filter path.
- 2.3 `zombie_delivery_state_transitions_total` — Counter, labels: `from`, `to`. Incremented on every successful `deliveryTransition()`.
- 2.4 `zombie_outbound_webhooks_total` — Counter, labels: `event_type`, `status` (delivered, retry, dead_letter, ssrf_blocked). Incremented by outbox delivery logic.

---

## 3.0 Structured Logging

**Status:** PENDING

Add structured log lines for every webhook codepath. Every failure mode must be diagnosable from logs alone.

**Dimensions:**
- 3.1 Webhook ingestion logs (all in webhook handler):
  - `INFO: webhook.received event={type} delivery_id={id} repo={repo}`
  - `WARN: webhook.signature_failed delivery_id={id} repo={repo}`
  - `DEBUG: webhook.filtered_non_zombie branch={ref}`
  - `DEBUG: webhook.orphaned_run branch={ref} pr_url={url}`
  - `INFO: webhook.delivery_state_changed run_id={id} from={old} to={new}`
  - `DEBUG: webhook.already_terminal run_id={id} state={state}`
- 3.2 Outbound delivery logs (in outbox reconciler):
  - `INFO: webhook.outbound_delivered run_id={id} event={type} url={url}`
  - `WARN: webhook.outbound_failed run_id={id} event={type} url={url} status={code} attempt={n}`
  - `ERROR: webhook.outbound_dead_letter run_id={id} event={type} url={url}`
  - `ERROR: webhook.ssrf_blocked url={url} resolved_ip={ip} workspace_id={ws}`
- 3.3 All log lines use the existing `obs_log` structured logging module. Field names match existing conventions (`run_id`, `workspace_id`). Scoped logger: `.webhook`.

---

## 4.0 Time-to-Merge Computation

**Status:** PENDING

Compute delivery duration metrics when PRs reach terminal delivery states.

**Dimensions:**
- 4.1 When `delivery_state` transitions to `MERGED`, compute `time_to_merge_ms = merged_at - pr_opened_at`. `pr_opened_at` is read from the `run_transitions` table (the row with `to_state = 'PR_OPENED'`). `merged_at` is the webhook event timestamp.
- 4.2 Include `time_to_merge_ms` as a property on the `pr.merged` PostHog event (§1.3). This enables the delivery SLA dashboard: "85% of PRs merged within 48 hours."
- 4.3 When `delivery_state` transitions to `CLOSED`, compute `time_to_close_ms` using the same pattern. Include on `pr.closed` PostHog event.
- 4.4 If `pr_opened_at` cannot be resolved (missing transition row — should not happen for runs that reached DONE), log at WARN and emit the PostHog event without the duration field. Never block the state transition on a missing timestamp.

---

## Alerting Rules (Grafana)

| Alert | Condition | Severity |
|---|---|---|
| `webhook_signature_failures_high` | `zombie_webhooks_received_total{status="rejected"} > 10` in 5 min | Warning |
| `outbound_dead_letters_high` | `zombie_outbound_webhooks_total{status="dead_letter"} > 5` in 1 hour | Warning |
| `ssrf_blocked` | `zombie_outbound_webhooks_total{status="ssrf_blocked"} > 0` | Critical |

---

## Debuggability Runbook

If an operator reports "my PR was merged but UseZombie still shows PR_OPEN":

1. Check `webhook.received` logs — was the merge event delivered by GitHub?
2. Check `webhook.signature_failed` — was it rejected due to secret mismatch?
3. Check `webhook.filtered_non_zombie` — was the branch prefix wrong?
4. Check `webhook.orphaned_run` — was the run_id not resolved from the branch?
5. Query `run_transitions` table for the run — what's the last `delivery_state`?
6. If no logs at all — check GitHub App webhook delivery log (GitHub UI) for delivery failures
