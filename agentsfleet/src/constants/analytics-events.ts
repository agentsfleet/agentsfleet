// PostHog analytics event names emitted by the CLI. These strings are
// EXTERNAL surfaces — dashboards, funnels, and queries depend on the
// exact values. Renaming any of them is a coordinated change with the
// downstream PostHog project (see Captain's 20-item review #1b / #5).
//
// One named export per event so every emit site reads from here.
//
// RULE UFS.

export const EVT_USER_AUTHENTICATED = "user_authenticated";
export const EVT_LOGOUT_COMPLETED = "logout_completed";

// Single command-lifecycle event emitted by the supabase-pattern
// withCommandInstrumentation wrapper (services/telemetry/
// command-instrumentation.ts). Properties: exit_code (0|1),
// duration_ms, plus command_run_id / command / flags_used /
// flag_values auto-merged from CurrentAnalyticsContext, plus
// device_id / session_id / is_first_run / is_tty / is_ci / os /
// arch / cli_version auto-merged from TelemetryRuntime.
//
// Error attribution lives on the span (Effect.withSpan) — the NDJSON
// exporter extracts status.exit._tag into the `error_code` field on
// the span line. No separate cli_error event.
//
// Re-exported from command-instrumentation.ts so this file stays the
// single source of truth for event names.
export { EVT_CLI_COMMAND_EXECUTED } from "../services/telemetry/command-instrumentation.ts";

// Auth-flow milestones. `login_completed` is the established wire
// contract — PostHog dashboards key off the bare name. New CLI events
// keep the `cli_` prefix; this one stays as-is to avoid breaking
// downstream funnels.
export const EVT_LOGIN_COMPLETED = "login_completed";

export const EVT_WORKSPACE_CREATED = "workspace_created";
export const EVT_WORKSPACE_ADD_COMPLETED = "workspace_add_completed";
export const EVT_WORKSPACE_LIST_VIEWED = "workspace_list_viewed";
export const EVT_WORKSPACE_USED = "workspace_used";
export const EVT_WORKSPACE_DELETED = "workspace_deleted";
