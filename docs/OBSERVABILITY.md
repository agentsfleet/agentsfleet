# Observability Strategy

**Date:** Mar 07, 2026
**Status:** DECISION RECORDED — implementation in M5

---

## Two-Layer Stack

usezombie uses two observability tools — Grafana and PostHog. They answer different questions and do not overlap.

| Layer | Tool | Answers |
|---|---|---|
| Infra / ops | Grafana Cloud — Prometheus metrics, Loki logs, Tempo traces (M1_003, M4_005, M12_001) | Is the service healthy? Latency, error rates, traces, token spend alerting |
| Product / users | PostHog (M5_001, M5_005, M5_006) | Who is using what? Funnels, retention, error attribution, per-user cost |

LLM token/cost data is dual-emitted to both layers: Grafana metrics (`zombie_agent_tokens_total`, `zombie_agent_duration_seconds`) for ops alerting, and PostHog event properties (`agent_completed.tokens`, `agent_completed.duration_ms`) for per-user cost attribution.

---

## PostHog: End-to-End Product Analytics

### Decision: option A — SDK per surface, single PostHog project

All surfaces identify users with the same `distinct_id` (Clerk user ID) so PostHog can stitch the full user journey across web, CLI, and backend.

```
usezombie.com (website) ─ posthog-js (TypeScript)  ──► PostHog
app.usezombie.com ─────── posthog-js (TypeScript)  ──► PostHog
zombiectl (Node CLI) ──── posthog-node (npm)       ──► PostHog   (same project)
zombied (Zig daemon) ── posthog-zig (M5_001)       ──► PostHog
nullclaw agents ─────── via zombied's client        ──► PostHog
Vercel edge (/install) ─ posthog-node server-side  ──► PostHog
```

### Why not a Redis Streams bus?

Considered: emit analytics events to Redis Streams from Zig, consume with a Node.js forwarder (single PostHog integration point, fire-and-forget from service perspective).

Decided against for M5 because:
1. posthog-zig is its own open-source SDK (M5_001) — the bus pattern would make it internal-only and kill the OSS value
2. posthog-zig already implements the buffer/retry/batch pattern that the bus would provide — it IS the fire-and-forget layer
3. Adding a consumer service adds an operational dependency for analytics, which must not block run execution

Bus pattern remains valid if posthog-zig proves operationally painful. Revisit in M6 if needed.

---

## Surface-by-Surface Integration

### 1. Website (`usezombie.com`) — M5_005

**SDK:** `posthog-js`
**Key events:**

| Event | Trigger |
|---|---|
| `signup_started` | Website CTA click to `app.usezombie.com` (for example hero/header Mission Control actions) |
| `signup_completed` | Website CTA click to quickstart/onboarding flow (`docs.usezombie.com/quickstart`) |
| `team_pilot_booking_started` | Team pilot CTA click (`mailto:team@usezombie.com`) |
| `navigation_clicked` | Website navigation click (for example Docs link in header) |

`distinct_id`: anonymous until login, then aliased to Clerk user ID via `posthog.identify()`.

### 2. App Dashboard (`app.usezombie.com`) — implemented Mar 13, 2026

**SDK:** `posthog-js`
**Key events:**

| Event | Trigger |
|---|---|
| `navigation_clicked` | Mission Control header/sidebar navigation click |

`distinct_id`: PostHog anonymous ID by default; can be upgraded to Clerk ID in later auth-identify wiring.

### 3. CLI (`zombiectl`) — implemented Mar 13, 2026

**SDK:** `posthog-node`
**Key events:**

| Event | Trigger |
|---|---|
| `cli_command_started` | Any routed command begins execution |
| `cli_command_finished` | Routed command exits with code |
| `user_authenticated` | `zombiectl login` success |
| `workspace_created` | `zombiectl workspace add` success |
| `run_triggered` | `zombiectl run` start success |
| `cli_error` | Any command exits non-zero with error context |

`distinct_id`: Clerk user ID (`sub`) parsed from stored auth token when available, otherwise `anonymous`.

Feature flags: `posthog-node` `/decide/` calls to gate beta CLI commands per user.

### 4. `zombied` (Zig daemon) — M5_006, depends on M5_001

**SDK:** `posthog-zig`
**Key events:**

| Event | Trigger |
|---|---|
| `run_started` | `handleStartRun` |
| `run_completed` | Pipeline completion with verdict, pr_url |
| `run_failed` | Terminal failure with error context |
| `run_retried` | `handleRetryRun` with attempt count |
| `agent_completed` | `emitNullclawRunEvent` — actor, tokens, duration |
| `workspace_paused` | `handlePauseWorkspace` |
| `entitlement_rejected` | Policy deny — conversion signal |
| `$exception` | Any unhandled error or panic in zombied |

`distinct_id`: Clerk user ID from Bearer token claims extracted per request.

### 5. NullClaw agents

Agents run inside zombied's process space. They emit events through zombied's posthog-zig client — no separate SDK needed.

Agent-specific events flow as properties on `agent_completed`:
- `actor`: Echo | Scout | Warden
- `tokens`: total token count
- `duration_ms`: wall-clock time
- `exit_status`: success | failure | timeout

---

## Error Tracking

PostHog has a built-in Error Tracking UI (GA'd 2024) that groups exceptions and shows user context. This is distinct from OTel error spans — it answers "which users hit this bug" not "how many 500s did we serve."

**`$exception` event shape** (required for PostHog Error Tracking UI):

```json
{
  "event": "$exception",
  "properties": {
    "distinct_id": "user_clerk_id",
    "$exception_type": "ZombiedError",
    "$exception_message": "workspace not found: ws_abc",
    "$exception_stack_trace_raw": "[...]",
    "$exception_handled": false,
    "$exception_level": "error",
    "workspace_id": "ws_abc",
    "run_id": "run_xyz"
  }
}
```

posthog-zig exposes `client.captureException()` as a first-class API (not in original M5_001 spec — added here). The spec must be updated.

---

## Identity Contract

All surfaces must use the same `distinct_id` to stitch the user journey in PostHog.

| Surface | distinct_id source |
|---|---|
| Website (pre-login) | PostHog anonymous ID |
| Website (post-login) | `posthog.identify(clerkUserId)` — aliases anonymous → Clerk ID |
| zombiectl | Clerk user ID from stored auth token |
| zombied | Clerk user ID from Bearer token claims |

Group analytics: workspace-level events include `$groups: { workspace: "ws_abc" }` so PostHog can aggregate by workspace in addition to by user.

---

## Spec Index

| Spec | Surface | Status |
|---|---|---|
| M5_001 | posthog-zig SDK (library) | DONE |
| M5_005 | Website PostHog integration | DONE |
| M5_006 | zombied PostHog integration | DONE |
| — | app dashboard + zombiectl PostHog integration | DONE (implementation; spec backfill pending) |

---

## What PostHog Enables at Full Coverage

1. **Activation funnel** — install → login → first workspace → first run → first PR merged
2. **Retention** — weekly active workspaces, cohort curves
3. **Churn signals** — users with no run activity in 7 days
4. **Error attribution** — which users hit backend exceptions, their plan and usage context
5. **Feature rollout** — gradual flag-gated feature release across CLI + web + backend
6. **Cost-per-user product metrics** — agent token spend correlated to the user who triggered it
7. **CLI adoption** — which commands are used, which fail, version upgrade rates
8. **Conversion** — pricing page → pilot booking → paid conversion
