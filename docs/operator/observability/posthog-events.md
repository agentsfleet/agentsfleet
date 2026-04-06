# PostHog Product Analytics — Event Catalogue

SDKs:
- `posthog-js` in `ui/packages/website`
- `posthog-js` in `ui/packages/app`
- `posthog-node` in `zombiectl`
- [posthog-zig](https://github.com/usezombie/posthog-zig) in `zombied`

> PostHog is one of two observability tools (with Grafana). Langfuse was removed in M12_001. See `docs/done/v1/M12_001_OBSERVABILITY_CONSOLIDATION.md`.

## Provisioning Contract

One PostHog project key is stored per environment:

| Vault | Item | Field |
|---|---|---|
| `ZMB_CD_DEV` | `posthog-dev` | `credential` |
| `ZMB_CD_PROD` | `posthog-prod` | `credential` |

These values are propagated through the standard playbook/check flow:
- bootstrap: `playbooks/001_bootstrap/001_playbook.md`
- preflight: `playbooks/002_preflight/001_playbook.md`
- credential gate: `playbooks/002_preflight/001_gate.sh`
- M2 gate section: `playbooks/gates/m2_001/section-2-procurement-readiness.sh`

## Surface Configuration

| Surface | Env var | SDK | Default host |
|---|---|---|---|
| Website | `VITE_POSTHOG_KEY` | `posthog-js` | `https://us.i.posthog.com` |
| App | `NEXT_PUBLIC_POSTHOG_KEY` | `posthog-js` | `https://us.i.posthog.com` |
| `zombied` API + worker | `POSTHOG_API_KEY` | `posthog-zig` | `https://us.i.posthog.com` |
| `zombiectl` CLI | `ZOMBIE_POSTHOG_KEY` | `posthog-node` | `https://us.i.posthog.com` |

Optional host overrides remain available in code, but vault provisioning only requires the key.

## Website Events

Emitter: `ui/packages/website/src/analytics/posthog.ts`

| Event | Typical properties | Description |
|---|---|---|
| `signup_started` | `source`, `surface`, `mode`, `path` | Website signup funnel entered |
| `signup_completed` | `source`, `surface`, `mode`, `path` | Website signup flow completed |
| `navigation_clicked` | `source`, `surface`, `target`, `path` | Primary website navigation interaction |
| `lead_capture_clicked` | `source`, `surface`, `cta_id`, `path` | Lead form CTA clicked |
| `lead_capture_opened` | `source`, `surface`, `component`, `path` | Lead form/modal opened |
| `lead_capture_submitted` | `source`, `surface`, `status`, `utm_*`, `path` | Lead form submitted |
| `lead_capture_failed` | `source`, `surface`, `status`, `path` | Lead form submit failed |

## App Events

Emitters:
- `ui/packages/app/instrumentation-client.ts`
- `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx`
- dashboard pages and cards in `ui/packages/app/app/(dashboard)` and `ui/packages/app/components/domain`

Allowed app properties are allowlisted in `ui/packages/app/lib/analytics/posthog.ts`.

| Event | Typical properties | Description |
|---|---|---|
| `page_navigation_started` | `source`, `surface`, `path` | Next.js router transition started |
| `ui_runtime_error` | `source`, `surface`, `path`, `error_message` | Browser runtime error or unhandled rejection |
| `navigation_clicked` | `source`, `surface`, `target`, `path` | Dashboard shell navigation clicked |
| `workspace_list_viewed` | `source`, `surface`, `workspace_count`, `path` | Workspace index viewed |
| `workspace_card_clicked` | `source`, `surface`, `workspace_id`, `workspace_plan`, `paused`, `path` | Workspace selected from card |
| `workspace_detail_viewed` | `source`, `surface`, `workspace_id`, `active_run_id`, `active_run_status`, `path` | Workspace detail page viewed |
| `run_row_clicked` | `source`, `surface`, `workspace_id`, `run_id`, `run_status`, `run_attempts`, `path` | Run selected from table/list |
| `run_detail_viewed` | `source`, `surface`, `workspace_id`, `run_id`, `run_status`, `has_error`, `has_pr_url`, `path` | Run detail page viewed |

Identity:
- user identification happens in `AnalyticsBootstrap.tsx`
- distinct fields: `user_id`, `email`

## CLI Events

Emitters:
- lifecycle: `zombiectl/src/cli.js`
- domain commands: `zombiectl/src/commands/*.js`

CLI properties are sanitized to strings before capture. Shared command context may include `user_id`, `email`, `workspace_id`, `run_id`, `agent_id`, `proposal_id`, `score`, `error_code`, `reason`.

| Event | Typical properties | Description |
|---|---|---|
| `cli_command_started` | `command`, `args`, context ids | Command invocation started |
| `cli_command_finished` | `command`, `exit_code`, context ids | Command invocation completed |
| `cli_error` | `command`, `error_code`, `reason` | Top-level CLI failure |
| `user_authenticated` | `user_id`, `email` | Login or auth success |
| `login_completed` | `user_id`, `email` | Device/browser login completed |
| `logout_completed` | `user_id` | Local logout completed |
| `workspace_add_completed` | `workspace_id` | Workspace create/add completed |
| `workspace_list_viewed` | `workspace_id` or counts | Workspace list displayed |
| `workspace_removed` | `workspace_id` | Workspace removed |
| `specs_synced` | `workspace_id` | Specs sync completed |
| `run_queued` | `workspace_id`, `run_id` | Run trigger completed |
| `run_status_viewed` | `workspace_id`, `run_id`, `run_status` | Run status displayed |
| `runs_list_viewed` | `workspace_id` | Runs list displayed |
| `harness_compiled` | `workspace_id`, `agent_id` | Harness compile completed |
| `harness_active_viewed` | `workspace_id`, `agent_id` | Active harness shown |
| `harness_activated` | `workspace_id`, `agent_id` | Harness profile activated |
| `harness_source_uploaded` | `workspace_id`, `agent_id` | Harness source uploaded |
| `agent_scores_viewed` | `workspace_id`, `agent_id`, `score` | Agent score view displayed |
| `agent_profile_viewed` | `workspace_id`, `agent_id` | Agent profile displayed |
| `agent_improvement_report_viewed` | `workspace_id`, `agent_id` | Improvement report displayed |
| `agent_proposals_viewed` | `workspace_id`, `agent_id` | Proposal list displayed |
| `agent_proposal_approved` | `workspace_id`, `agent_id`, `proposal_id` | Proposal approved |
| `agent_proposal_rejected` | `workspace_id`, `agent_id`, `proposal_id`, `reason` | Proposal rejected |
| `agent_proposal_vetoed` | `workspace_id`, `agent_id`, `proposal_id`, `reason` | Proposal vetoed |
| `agent_harness_reverted` | `workspace_id`, `agent_id`, `proposal_id` | Harness revert executed |

## Runtime Events

Emitters live in `src/observability/posthog_events.zig`.

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
| `auth_rejected` | `common.zig` | `reason`, `request_id` | Bearer token auth failed |

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
| `run_completed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `verdict`, `duration_ms` | Run finished |
| `run_failed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `reason`, `duration_ms` | Run failed |

### Agent & Scoring

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `agent_completed` | `pipeline/worker_stage_executor.zig` | `run_id`, `workspace_id`, `actor`, `tokens`, `duration_ms`, `exit_status` | Agent stage execution finished |
| `agent.run.scored` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `score`, `tier`, `score_formula_version`, `axis_scores`, `weight_snapshot`, `scored_at`, `axis_completion`, `axis_error_rate`, `axis_latency`, `axis_resource` | Quality score computed |
| `agent.scoring.failed` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `error` | Scoring computation failed |
| `agent.trust.earned` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `consecutive_count_at_event` | Consecutive positive score streak |
| `agent.trust.lost` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `consecutive_count_at_event` | Trust state dropped |
| `agent.harness.changed` | `pipeline/scoring.zig` | `agent_id`, `proposal_id`, `workspace_id`, `approval_mode`, `trigger_reason`, `fields_changed` | Harness config mutation applied |
| `agent.improvement.stalled` | `pipeline/scoring.zig` | `run_id`, `workspace_id`, `agent_id`, `proposal_id`, `consecutive_negative_deltas` | Score declining after proposal |

### Policy & Billing

| Event | Emitter | Properties | Description |
|---|---|---|---|
| `entitlement_rejected` | handler-level | `workspace_id`, `boundary`, `reason_code`, `request_id` | Plan limit hit |
| `profile_activated` | handler-level | `workspace_id`, `agent_id`, `config_version_id`, `run_snapshot_version`, `request_id` | Agent harness profile activated |
| `billing_lifecycle_event` | handler-level | `workspace_id`, `event_type`, `reason`, `plan_tier`, `billing_status`, `request_id` | Plan transitions and payment events |
| `api_error` | handler-level | `error_code`, `message`, `request_id`, `workspace_id` | UZ-* error code fired at HTTP boundary |

## Error Code Coverage

| Code Prefix | PostHog Coverage |
|---|---|
| `UZ-AUTH-*` | `auth_rejected` captures auth failures |
| `UZ-ENTL-*` | `entitlement_rejected` and `api_error` |
| `UZ-BILLING-*` | `billing_lifecycle_event` and `api_error` |
| `UZ-STARTUP-*` | `startup_failed` |
| `UZ-WORKSPACE-*` | `api_error` for billing/workspace enforcement failures |
| Browser/runtime UI failures | `ui_runtime_error` in Next.js app |
| CLI command failures | `cli_error` in `zombiectl` |
| `UZ-SANDBOX-*` | structured logs and metrics first; correlate with PostHog run lifecycle via `trace_id` and `run_id` |

## Adding New Events

1. Add or extend the surface helper closest to the emitter (`posthog.ts`, CLI analytics helper, or `posthog_events.zig`).
2. Keep analytics fail-open and non-blocking.
3. Allowlist or sanitize properties before capture.
4. Add unit or integration coverage for the new event path.
5. Update this document and the relevant done-spec if the surface contract changed.
