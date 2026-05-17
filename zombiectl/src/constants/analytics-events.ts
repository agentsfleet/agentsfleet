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

// Dispatcher triplet — emitted by the Effect dispatcher around every
// command Effect (started → finished, or started → error → finished).
// cli_session_id / cli_device_id are auto-merged inside the Analytics
// service from TelemetryRuntime so command code cannot accidentally
// drop them.
export const EVT_CLI_COMMAND_STARTED = "cli_command_started";
export const EVT_CLI_COMMAND_FINISHED = "cli_command_finished";
export const EVT_CLI_ERROR = "cli_error";

// Auth-flow milestones.
export const EVT_CLI_LOGIN_COMPLETED = "cli_login_completed";

export const EVT_WORKSPACE_CREATED = "workspace_created";
export const EVT_WORKSPACE_ADD_COMPLETED = "workspace_add_completed";
export const EVT_WORKSPACE_LIST_VIEWED = "workspace_list_viewed";
export const EVT_WORKSPACE_USED = "workspace_used";
export const EVT_WORKSPACE_DELETED = "workspace_deleted";
