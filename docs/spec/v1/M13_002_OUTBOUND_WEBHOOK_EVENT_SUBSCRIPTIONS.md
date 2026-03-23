# M13_002: Outbound Webhook Event Subscriptions

**Prototype:** v1.0.0
**Milestone:** M13
**Workstream:** 002
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P0 — fulfills the Agents page webhook promise; enables agent-to-agent use case
**Batch:** B2 — after M13_001 (delivery state must exist)
**Depends on:** M13_001 (delivery_state column and webhook handler)

---

## Problem

The Agents page (`usezombie.sh/agents`) shows a `run.completed` webhook payload example, promising external agents that UseZombie will POST event callbacks. This promise has no backend implementation. Without outbound events, the agent-to-agent use case (USECASE.md §3) requires polling `GET /v1/runs/{id}` — which is fragile, wasteful, and not how control planes work.

---

## Design Decisions (from CEO Review, Mar 23, 2026)

- **Event subscription model** — workspaces subscribe to specific event types via `webhook_subscriptions` table (Issue 3, Option B)
- **SSRF prevention** — write-time URL validation + delivery-time IP check (Issue 9, Option C)
- **Subscription auth** — any authenticated workspace member can manage subscriptions (Issue 11, Option B)
- **Reuse existing outbox** — outbound webhooks are new effect types (`webhook:*`) in `run_side_effect_outbox`, not a parallel delivery system

---

## 1.0 Webhook Subscriptions Table and CRUD API

**Status:** PENDING

Create `webhook_subscriptions` table and REST endpoints for managing subscriptions.

**Dimensions:**
- 1.1 Schema migration: `CREATE TABLE webhook_subscriptions (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id), url TEXT NOT NULL, event_types TEXT[] NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL)`. Index on `workspace_id`.
- 1.2 `POST /v1/workspaces/{workspace_id}/webhook-subscriptions` — create subscription. Body: `{ "url": "https://...", "event_types": ["run.completed", "pr.merged", ...] }`. Returns 201 with subscription ID. Auth: any authenticated workspace member (Clerk JWT or API key).
- 1.3 `GET /v1/workspaces/{workspace_id}/webhook-subscriptions` — list subscriptions. Returns array of subscription objects. Auth: same as create.
- 1.4 `DELETE /v1/workspaces/{workspace_id}/webhook-subscriptions/{id}` — delete subscription. Returns 204. Auth: same as create.

---

## 2.0 SSRF Validation

**Status:** PENDING

Prevent outbound webhook URLs from targeting internal services.

**Dimensions:**
- 2.1 **Write-time validation** (on subscription create): URL must be HTTPS. Resolve DNS. Reject if resolved IP is RFC 1918 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), link-local (`169.254.0.0/16`), loopback (`127.0.0.0/8`), or known cloud metadata endpoints. Return 422 with message `"URL must resolve to a public HTTPS endpoint"`.
- 2.2 **Delivery-time validation** (on outbound POST): before connecting, resolve the URL's DNS again. Check resolved IP against the same denylist. If blocked, dead-letter the outbox row with `reason=ssrf_blocked` and emit `ERROR: webhook.ssrf_blocked url={url} resolved_ip={ip} workspace_id={ws}`.
- 2.3 Shared denylist function `isPrivateIp(ip: []const u8) bool` used by both write-time and delivery-time checks. Single implementation, no duplication.
- 2.4 IPv6 equivalents included in denylist: `::1` (loopback), `fc00::/7` (unique local), `fe80::/10` (link-local).

---

## 3.0 Outbox Integration

**Status:** PENDING

Outbound webhooks use the existing `run_side_effect_outbox` pattern. New effect types are inserted when delivery state changes.

**Dimensions:**
- 3.1 When `delivery_state` changes (M13_001 §3.3), query `webhook_subscriptions` for subscriptions matching the workspace and event type. For each matching subscription, insert an outbox row with `effect_key = 'webhook:{event_type}:{subscription_id}'`. Idempotency: `ON CONFLICT (run_id, effect_key) DO UPDATE` — same pattern as existing outbox.
- 3.2 If no matching subscriptions exist for the event type, no outbox rows are inserted. No log, no error — this is the normal case for workspaces without subscriptions.
- 3.3 Outbox payload (stored as JSON in `payload` column): `{ "event": "{event_type}", "run_id": "...", "workspace_id": "...", "delivery_state": "...", "pr_url": "...", "artifacts": {...}, "attempts": N, "duration_seconds": N, "timestamp": "..." }`. For `pr.changes_requested`, include `review_comments` array extracted from GitHub API (best-effort enrichment; empty array if API call fails).

---

## 4.0 Event Delivery with Retry

**Status:** PENDING

Deliver outbound webhooks via HTTP POST with exponential backoff retry.

**Dimensions:**
- 4.1 The existing startup reconciler (`worker_pr_flow.zig` pattern) claims `status='pending'` outbox rows with `effect_key LIKE 'webhook:%'`. For each, performs HTTP POST to the subscription URL with JSON payload body and `Content-Type: application/json` header.
- 4.2 On success (2xx response): mark outbox row `status='delivered'`. Log at INFO: `webhook.outbound_delivered run_id={id} event={type} url={url}`.
- 4.3 On failure (non-2xx, timeout, connection error): increment attempt counter. If attempts < 3, leave as `pending` for next reconciler pass (exponential backoff via reconciler interval). If attempts >= 3, mark `status='dead_letter'`. Log at ERROR: `webhook.outbound_dead_letter run_id={id} event={type} url={url}`.
- 4.4 HTTP POST timeout: 10 seconds per attempt. No following redirects (prevents SSRF via redirect to internal IP).

---

## Supported Event Types

| Event Type | Trigger |
|---|---|
| `run.completed` | Run reaches `DONE` state |
| `run.blocked` | Run reaches `BLOCKED`/`NOTIFIED_BLOCKED` state |
| `pr.merged` | `delivery_state` → `MERGED` |
| `pr.closed` | `delivery_state` → `CLOSED` |
| `pr.changes_requested` | `delivery_state` → `CHANGES_REQUESTED` |
| `pr.approved` | `delivery_state` → `APPROVED` |
| `pr.ci_failed` | `delivery_state` → `CI_FAILED` |
| `agent.trust_changed` | Agent trust level changes (M9 integration) |

---

## Agents Page Contract Alignment

The existing Agents page (`Agents.tsx:34-48`) shows this payload shape:

```json
{
  "event": "run.completed",
  "run_id": "run_01JEXAMPLE",
  "workspace_id": "ws_01JEXAMPLE",
  "status": "DONE",
  "pr_url": "https://github.com/org/repo/pull/42",
  "artifacts": { "plan": "plan.json", "implementation": "implementation.md", "validation": "validation.md", "summary": "run_summary.md" },
  "attempts": 1,
  "duration_seconds": 34
}
```

The outbox payload (§3.3) must match this shape for `run.completed`. Additional fields (`delivery_state`, `review_comments`, `timestamp`) are additive — non-breaking for agents that parse the documented shape.
