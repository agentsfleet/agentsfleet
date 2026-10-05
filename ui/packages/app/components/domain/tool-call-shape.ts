import { TOOL_CALL_STATUS, type ToolCallStatus } from "@/lib/streaming/fleet-stream-tool-trace";
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
  DIFF: "diff",
  CLIPPED_EDIT: "clipped-edit",
  PLAN: "plan",
} as const;

export type ToolBodyKind = (typeof TOOL_BODY)[keyof typeof TOOL_BODY];

/** `afr_tools::plan` step statuses. */
export const PLAN_STATUS = {
  PENDING: "pending",
  IN_PROGRESS: "in_progress",
  COMPLETED: "completed",
} as const;

export type PlanStatus = (typeof PLAN_STATUS)[keyof typeof PLAN_STATUS];

export type PlanStep = { step: string; status: PlanStatus };

/** What sits under a cell's header, besides its arguments. An edit and a
 * patch are one kind: the lines they changed. */
export type ToolBody =
  | { kind: typeof TOOL_BODY.OUTPUT }
  | { kind: typeof TOOL_BODY.COMMAND; rail: readonly string[]; hiddenLines: number }
  | { kind: typeof TOOL_BODY.DIFF; diff: LineDiff }
  | { kind: typeof TOOL_BODY.CLIPPED_EDIT }
  | { kind: typeof TOOL_BODY.PLAN; explanation: string | null; steps: readonly PlanStep[] };

/** The member of `ToolBody` each kind names, so a table keyed by kind can
 * type its rows by the body they draw. */
export type ToolBodyOf<K extends ToolBodyKind> = { [B in ToolBody as B["kind"]]: B }[K];

// Whether a kind shows the call's output once the call succeeded. An output
// or command cell is its output; an edit that worked shows its diff alone, as
// Codex's does, and a plan its steps.
const OUTPUT_WHEN_SUCCEEDED: Record<ToolBodyKind, boolean> = {
  [TOOL_BODY.OUTPUT]: true,
  [TOOL_BODY.COMMAND]: true,
  [TOOL_BODY.DIFF]: false,
  [TOOL_BODY.CLIPPED_EDIT]: false,
  [TOOL_BODY.PLAN]: false,
};

/** Whether a settled cell shows what came back: always when the call did not
 * say it succeeded, else as its kind decides. */
export function showsOutput(body: ToolBody, status: ToolCallStatus | undefined): boolean {
  return status !== TOOL_CALL_STATUS.SUCCEEDED || OUTPUT_WHEN_SUCCEEDED[body.kind];
}

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
