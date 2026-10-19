// Navigation shares these values without initializing setup schemas.
export const DRAFT_STATE = {editing: "editing", saving: "saving", closed: "closed"} as const;
export const MESSAGE_ROLE = {user: "user", assistant: "assistant"} as const;
export const SCHEDULE_RUN_STATE = {
  scheduled: "scheduled", paused: "paused", activationPending: "activation_pending", unavailable: "next_run_unavailable",
} as const;

export const DRAFT_SEARCH_QUERY_MAX = 200;
export const DRAFT_SEARCH_STATUS = {loading: "loading", ready: "ready", failed: "failed", inaccessible: "inaccessible"} as const;
