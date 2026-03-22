# PostHog Product Analytics — Event Catalogue

SDK: [posthog-zig](https://github.com/usezombie/posthog-zig) v0.1.3 (pure Zig, async ring-buffer flush).

> PostHog is one of two observability tools (with Grafana). Langfuse was removed in M12_001. See `docs/spec/v1/M12_001_OBSERVABILITY_CONSOLIDATION.md`.

## Configuration

| Variable | Required | Default |
|---|---|---|
| `POSTHOG_API_KEY` | No | Analytics disabled when absent |

Host: `https://us.i.posthog.com`
Flush interval: 10 s / batch-of-20.
Max retries: 3 (exponential backoff).
All events are fire-and-forget — analytics never blocks execution.

## Initialization

PostHog is initialized in three entry points, all fail-open (null client on error):

| Command | File |
|---|---|
| `zombied serve` | `src/cmd/serve.zig` |
| `zombied worker` | `src/cmd/worker.zig` |
| `zombied reconcile` | `src/cmd/reconcile.zig` |

HTTP handlers access the client via `ctx.posthog`.

---

## Event Reference

All event functions live in `src/observability/posthog_events.zig`.

### Startup Lifecycle

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `server_started` | `cmd/serve.zig` | `port`, `worker_concurrency` | HTTP server ready to accept traffic |
| `worker_started` | `cmd/worker.zig` | `concurrency` | Worker threads spawned |
| `startup_failed` | `cmd/worker.zig` | `command`, `phase`, `reason` | Fatal startup failure (after PostHog init) |

### Auth Lifecycle

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `auth_login_completed` | `auth_sessions_http.zig` | `session_id`, `request_id` | CLI auth session completed (OIDC device flow) |
| `auth_rejected` | `common.zig` | `reason`, `request_id` | Bearer token auth failed (unauthorized, expired, unavailable) |

### Workspace Lifecycle

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `workspace_created` | `workspaces_lifecycle.zig` | `workspace_id`, `tenant_id`, `repo_url`, `request_id` | New workspace provisioned via API |
| `workspace_github_connected` | `github_callback.zig` | `workspace_id`, `installation_id`, `request_id` | GitHub App OAuth callback completed |

### Run Lifecycle

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `run_started` | `runs/start.zig` | `run_id`, `workspace_id`, `spec_id`, `mode`, `request_id` | Run enqueued |
| `run_retried` | `runs/retry.zig` | `run_id`, `workspace_id`, `attempt`, `request_id` | Run retry enqueued |
| `run_completed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `verdict`, `duration_ms` | Run finished (pass/fail) |
| `run_failed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `reason`, `duration_ms` | Run failed (error/timeout) |

### Agent & Scoring

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `agent_completed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `actor`, `tokens`, `duration_ms`, `exit_status` | Agent stage execution finished |
| `agent.run.scored` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `score`, `tier`, `score_formula_version`, `axis_scores` (JSON), `weight_snapshot` (JSON), `scored_at`, `axis_completion`, `axis_error_rate`, `axis_latency`, `axis_resource` | Quality score computed |
| `agent.scoring.failed` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `error` | Scoring computation failed |
| `agent.trust.earned` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `consecutive_count_at_event` | Consecutive positive score streak |
| `agent.trust.lost` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `consecutive_count_at_event` | Trust state dropped |
| `agent.harness.changed` | `pipeline/scoring.zig` | `agent_id`, `proposal_id`, `workspace_id`, `approval_mode`, `trigger_reason`, `fields_changed` (JSON) | Harness config mutation applied |
| `agent.improvement.stalled` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `proposal_id`, `consecutive_negative_deltas` | Score declining after proposal |

### Policy & Billing

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `entitlement_rejected` | handler-level | `workspace_id`, `boundary`, `reason_code`, `request_id` | Plan limit hit (compile/run/stage) |
| `profile_activated` | handler-level | `workspace_id`, `agent_id`, `config_version_id`, `run_snapshot_version`, `request_id` | Agent harness profile activated |
| `billing_lifecycle_event` | handler-level | `workspace_id`, `event_type`, `reason`, `plan_tier`, `billing_status`, `request_id` | Plan transitions, payment events |

### API Error Tracking

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `api_error` | handler-level | `error_code`, `message`, `request_id`, (optional: `workspace_id`) | UZ-* error code fired at HTTP boundary |

Wired into: billing enforcement failures (`workspaces_lifecycle.zig`), billing state errors (`workspaces_billing.zig`). Uses `trackApiError` (without workspace) or `trackApiErrorWithContext` (with workspace).

---

## Error Code Coverage

Error codes from `src/errors/codes.zig` surface in PostHog events as follows:

| Code Prefix | PostHog Coverage |
|---|---|
| `UZ-AUTH-*` | `auth_rejected` event captures reason (unauthorized, token_expired, auth_service_unavailable) |
| `UZ-ENTL-*` | `entitlement_rejected` event with `reason_code` + `api_error` event |
| `UZ-BILLING-*` | `billing_lifecycle_event` + `api_error` event for failures |
| `UZ-STARTUP-*` | `startup_failed` event with `error_code` field |
| `UZ-WORKSPACE-*` | `api_error` event for billing enforcement failures |
| `UZ-INTERNAL-*` | Covered by structured logging (not PostHog — these are operational, not product) |
| `UZ-RUN-*` | Domain validation — covered by structured logging |

## Adding New Events

1. Add a `trackXxx` function in `src/observability/posthog_events.zig`
2. Follow the nullable-client pattern: accept `?*posthog.PostHogClient`, no-op when null
3. Always `catch {}` on capture — analytics must never block
4. Add the new function call to the no-op integration test
5. Wire the call at the appropriate handler/pipeline site
6. Update this document
