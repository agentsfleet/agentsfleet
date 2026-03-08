# Observability Strategy

**Date:** Mar 07, 2026
**Status:** DECISION RECORDED — implementation in M5

---

## Three-Layer Stack

usezombie uses three complementary observability tools. They answer different questions and do not overlap.

| Layer | Tool | Answers |
|---|---|---|
| Infra / ops | OpenTelemetry (M1_003, M4_005) | Is the service healthy? Latency, error rates, traces |
| AI / LLM | Langfuse (M1_003) | Is the agent spending wisely? Token cost, run traces |
| Product / users | PostHog (M5_001, M5_005, M5_006) | Who is using what? Funnels, retention, error attribution |

These are additive. OTel tells you p99 latency is 450ms. PostHog tells you alice@acme.com on the Pro plan has had 3 failed runs this week and is likely to churn. Langfuse tells you the Scout agent burned $0.40 on that run. You need all three.

---

## PostHog: End-to-End Product Analytics

### Decision: option A — SDK per surface, single PostHog project

All surfaces identify users with the same `distinct_id` (Clerk user ID) so PostHog can stitch the full user journey across web, CLI, and backend.

```
app.usezombie.com ──── posthog-js (TypeScript)    ──► PostHog
zombiectl (Node CLI) ── posthog-node (npm)         ──► PostHog   (same project)
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

### 1. Website (`app.usezombie.com`) — M5_005

**SDK:** `posthog-js`
**Key events:**

| Event | Trigger |
|---|---|
| `signup_started` | Signup form opened |
| `signup_completed` | Account created |
| `team_pilot_booking_started` | CTA click |
| `zombiectl_install` | `curl /install` via Vercel edge function |

`distinct_id`: anonymous until login, then aliased to Clerk user ID via `posthog.identify()`.

### 2. CLI (`zombiectl`) — no spec yet, add in M5

**SDK:** `posthog-node`
**Key events:**

| Event | Trigger |
|---|---|
| `user_authenticated` | `zombiectl login` success |
| `workspace_created` | `zombiectl workspace create` |
| `run_triggered` | `zombiectl run start` |
| `cli_error` | Any command exits non-zero with error context |
| `cli_upgrade` | Version mismatch detected on startup |

`distinct_id`: Clerk user ID from stored auth token. Set on `zombiectl login`, persisted in local config.

Feature flags: `posthog-node` `/decide/` calls to gate beta CLI commands per user.

### 3. `zombied` (Zig daemon) — M5_006, depends on M5_001

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

### 4. NullClaw agents

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
| M5_001 | posthog-zig SDK (library) | PENDING |
| M5_005 | Website PostHog integration | PENDING |
| M5_006 | zombied PostHog integration | PENDING |
| — | zombiectl PostHog integration | **no spec yet** |

Next: add M5_007 for zombiectl PostHog integration.

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
