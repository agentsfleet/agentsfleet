import type { LineDiff } from "./tool-call-diff";

// What a tool cell is made of, shared by every copy map: the tool names, the
// body kinds a cell can draw, and the copy a call's arguments become.

/** Tool names as `afr_tools::catalog` spells them. */
export const TOOL_NAME = {
  FILE_READ: "file_read",
  FILE_READ_HASHED: "file_read_hashed",
  FILE_WRITE: "file_write",
  FILE_APPEND: "file_append",
  FILE_DELETE: "file_delete",
  FILE_EDIT: "file_edit",
  FILE_EDIT_HASHED: "file_edit_hashed",
  HTTP_REQUEST: "http_request",
  MEMORY_STORE: "memory_store",
  MEMORY_RECALL: "memory_recall",
  MEMORY_LIST: "memory_list",
  MEMORY_FORGET: "memory_forget",
  EXEC_COMMAND: "exec_command",
  SHELL: "shell",
  WRITE_STDIN: "write_stdin",
  APPLY_PATCH: "apply_patch",
  GIT: "git",
  IMAGE: "image",
  BROWSER: "browser",
  BROWSER_OPEN: "browser_open",
  SCREENSHOT: "screenshot",
  WEB_FETCH: "web_fetch",
  WEB_SEARCH: "web_search",
  PUSHOVER: "pushover",
  UPDATE_PLAN: "update_plan",
  MESSAGE: "message",
  SCHEDULE: "schedule",
  CRON_ADD: "cron_add",
  CRON_LIST: "cron_list",
  CRON_REMOVE: "cron_remove",
  CRON_UPDATE: "cron_update",
  CRON_RUN: "cron_run",
  CRON_RUNS: "cron_runs",
  DELEGATE: "delegate",
  SPAWN: "spawn",
} as const;

export const TOOL_BODY = {
  OUTPUT: "output",
  COMMAND: "command",
  EDIT: "edit",
  CLIPPED_EDIT: "clipped-edit",
  PATCH: "patch",
  PLAN: "plan",
} as const;

/** `afr_tools::plan` step statuses. */
export const PLAN_STATUS = {
  PENDING: "pending",
  IN_PROGRESS: "in_progress",
  COMPLETED: "completed",
} as const;

export type PlanStatus = (typeof PLAN_STATUS)[keyof typeof PLAN_STATUS];

export type PlanStep = { step: string; status: PlanStatus };

/** What sits under a cell's header, besides its arguments. */
export type ToolBody =
  | { kind: typeof TOOL_BODY.OUTPUT }
  | { kind: typeof TOOL_BODY.COMMAND; rail: readonly string[]; hiddenLines: number }
  | { kind: typeof TOOL_BODY.EDIT; before: string; after: string }
  | { kind: typeof TOOL_BODY.CLIPPED_EDIT }
  | { kind: typeof TOOL_BODY.PATCH; diff: LineDiff }
  | { kind: typeof TOOL_BODY.PLAN; explanation: string | null; steps: readonly PlanStep[] };

export type Verbs = { readonly running: string; readonly done: string };

export type ToolCopy = {
  verbs: Verbs;
  target: string;
  /** Lines a write adds, when its content shows them whole. */
  addedLines?: number;
  body: ToolBody;
};

/** What a known tool's cell names when the runner kept none of its arguments:
 * it drops them whole past its own budget, and nothing is invented instead. */
export const ARGS_NOT_RECORDED = "(arguments not recorded)";

export function plainCopy(verbs: Verbs, target: string): ToolCopy {
  return { verbs, target, body: { kind: TOOL_BODY.OUTPUT } };
}
