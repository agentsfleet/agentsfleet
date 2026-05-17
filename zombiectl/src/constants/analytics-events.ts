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

export const EVT_WORKSPACE_CREATED = "workspace_created";
export const EVT_WORKSPACE_ADD_COMPLETED = "workspace_add_completed";
export const EVT_WORKSPACE_LIST_VIEWED = "workspace_list_viewed";
export const EVT_WORKSPACE_USED = "workspace_used";
export const EVT_WORKSPACE_DELETED = "workspace_deleted";
