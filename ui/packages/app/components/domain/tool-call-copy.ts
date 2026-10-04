import type { JsonValue, ToolArgs, ToolCallStatus } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { truncate } from "@/lib/utils";

// What a tool call says in the thread, the way Codex's transcript says it: a
// verb that is `-ing` while the call runs and past tense once it returns, the
// thing it touched, and what came back. Verbs come from the tool's name
// because hosted tools take structured arguments; Codex parses a shell line.
// Pure: a call's name, arguments and outcome in, words out.

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
} as const;

/** How a cell reads. `SETTLED` is a call that returned without saying how,
 * as an older runner's does: no colour claims it succeeded. */
export const CELL_STATE = {
  RUNNING: "running",
  SUCCEEDED: TOOL_CALL_STATUS.SUCCEEDED,
  FAILED: TOOL_CALL_STATUS.FAILED,
  INTERRUPTED: TOOL_CALL_STATUS.INTERRUPTED,
  SETTLED: "settled",
} as const;

export type CellState = (typeof CELL_STATE)[keyof typeof CELL_STATE];

export const TOOL_BODY = {
  OUTPUT: "output",
  COMMAND: "command",
  EDIT: "edit",
  CLIPPED_EDIT: "clipped-edit",
} as const;

/** What sits under a cell's header, besides its arguments. */
export type ToolBody =
  | { kind: typeof TOOL_BODY.OUTPUT }
  | { kind: typeof TOOL_BODY.COMMAND; rail: readonly string[]; hiddenLines: number }
  | { kind: typeof TOOL_BODY.EDIT; before: string; after: string }
  | { kind: typeof TOOL_BODY.CLIPPED_EDIT };

type Verbs = { readonly running: string; readonly done: string };

export type ToolCopy = {
  verbs: Verbs;
  target: string;
  /** Lines a write adds, when its content shows them whole. */
  addedLines?: number;
  body: ToolBody;
};

/** `afd_wire::tool_trace::ARGS_LEAF_MAX_BYTES`: the runner cuts any longer
 * argument string to this many bytes, on a character boundary, unmarked. */
export const ARGS_LEAF_MAX_BYTES = 256;
// A cut lands on the last whole character, at most one character short.
const UTF8_MAX_CHAR_BYTES = 4;
export const OUTPUT_PREVIEW_ROWS = 3;
export const COMMAND_RAIL_ROWS = 2;
const UNKNOWN_ARGS_MAX_CHARS = 120;
const WORKSPACE_ROOT = "/workspace/";
const DEFAULT_METHOD = "GET";
const LINE_BREAK = /\r?\n/;
const UTF8 = new TextEncoder();

export const INTERRUPTED_VERB = "Interrupted";
export const EMPTY_OUTPUT = "(no output)";
export const OUTPUT_UNAVAILABLE = "(output unavailable)";
export const FAILED_MARK = "(failed)";

const VERBS = {
  WRITE: { running: "Writing", done: "Wrote" },
  APPEND: { running: "Appending", done: "Appended" },
  DELETE: { running: "Deleting", done: "Deleted" },
  EDIT: { running: "Editing", done: "Edited" },
  REQUEST: { running: "Requesting", done: "Requested" },
  REMEMBER: { running: "Remembering", done: "Remembered" },
  FORGET: { running: "Forgetting", done: "Forgot" },
  RUN: { running: "Running", done: "Ran" },
  CALL: { running: "Calling", done: "Called" },
} as const satisfies Record<string, Verbs>;

const ARG = {
  PATH: "path",
  CONTENT: "content",
  OLD: "old_text",
  NEW: "new_text",
  URL: "url",
  METHOD: "method",
  KEY: "key",
  CMD: "cmd",
  COMMAND: "command",
} as const;

// A Map, not an object literal: a tool named `toString` must read as unknown.
const COPY: ReadonlyMap<string, (args: ToolArgs) => ToolCopy> = new Map([
  [TOOL_NAME.FILE_WRITE, (args) => writeCopy(VERBS.WRITE, args)],
  [TOOL_NAME.FILE_APPEND, (args) => writeCopy(VERBS.APPEND, args)],
  [TOOL_NAME.FILE_DELETE, (args) => plainCopy(VERBS.DELETE, pathArg(args))],
  [TOOL_NAME.FILE_EDIT, editCopy],
  [TOOL_NAME.FILE_EDIT_HASHED, editCopy],
  [TOOL_NAME.HTTP_REQUEST, requestCopy],
  [TOOL_NAME.MEMORY_STORE, (args) => plainCopy(VERBS.REMEMBER, stringArg(args, ARG.KEY) ?? "")],
  [TOOL_NAME.MEMORY_FORGET, (args) => plainCopy(VERBS.FORGET, stringArg(args, ARG.KEY) ?? "")],
  [TOOL_NAME.EXEC_COMMAND, (args) => commandCopy(stringArg(args, ARG.CMD))],
  [TOOL_NAME.SHELL, (args) => commandCopy(stringArg(args, ARG.COMMAND))],
]);

/** A call's verbs, target and body. A tool this map does not know reads as
 * Codex reads any dynamic tool: `Called name(arguments)`. */
export function toolCopy(name: string, args: ToolArgs | undefined): ToolCopy {
  const known = COPY.get(name);
  if (known !== undefined) return known(args ?? {});
  const shown = args === undefined ? "" : `(${truncate(JSON.stringify(args), UNKNOWN_ARGS_MAX_CHARS)})`;
  return plainCopy(VERBS.CALL, `${name}${shown}`);
}

/** The header's verb: `Interrupted` replaces it, as Codex's does. */
export function verbFor(verbs: Verbs, state: CellState): string {
  if (state === CELL_STATE.INTERRUPTED) return INTERRUPTED_VERB;
  return state === CELL_STATE.RUNNING ? verbs.running : verbs.done;
}

/** How a cell reads from its outcome. A call with no outcome that is no
 * longer running was ended by its turn: no call is drawn running after it. */
export function cellState(hasResult: boolean, status: ToolCallStatus | undefined, partRunning: boolean): CellState {
  if (!hasResult) return partRunning ? CELL_STATE.RUNNING : CELL_STATE.INTERRUPTED;
  return status ?? CELL_STATE.SETTLED;
}

export type OutputPreview = { rows: readonly string[]; hiddenLines: number; note: string | null };

/** Codex's preview: the first rows of output, then how many lines it left
 * out. With no rows, a note says whether there was nothing or nothing kept:
 * only a call that returned and said how can claim it printed nothing. */
export function outputPreview(head: string | undefined, lineCount: number | undefined, status: ToolCallStatus | undefined): OutputPreview {
  const lines = head === undefined ? [] : linesOf(head);
  const rows = lines.slice(0, OUTPUT_PREVIEW_ROWS);
  const hiddenLines = Math.max(0, (lineCount ?? lines.length) - rows.length);
  if (rows.length > 0) return { rows, hiddenLines, note: null };
  const unknown = status === undefined || status === TOOL_CALL_STATUS.INTERRUPTED || hiddenLines > 0;
  return { rows, hiddenLines, note: unknown ? OUTPUT_UNAVAILABLE : EMPTY_OUTPUT };
}

/** "+5 lines", "+1 line": what a preview left out. */
export function moreLinesLabel(hidden: number): string {
  return `+${hidden} ${hidden === 1 ? "line" : "lines"}`;
}

/** Whether a string argument may have been cut by the runner. */
export function mayBeClipped(text: string): boolean {
  return UTF8.encode(text).length > ARGS_LEAF_MAX_BYTES - UTF8_MAX_CHAR_BYTES;
}

/** A sandbox path as the operator reads it: relative to the workspace. */
export function workspacePath(path: string): string {
  return path.startsWith(WORKSPACE_ROOT) ? path.slice(WORKSPACE_ROOT.length) : path;
}

/** The text's lines, a final line break ending the last one. */
export function linesOf(text: string): string[] {
  const lines = text.split(LINE_BREAK);
  if (lines.at(-1) === "") lines.pop();
  return lines;
}

/** A string argument, or undefined when it is absent or not a string. */
export function stringArg(args: ToolArgs | undefined, key: string): string | undefined {
  const value: JsonValue | undefined = args?.[key];
  return typeof value === "string" ? value : undefined;
}

/** An edit's two sides as its arguments carry them, or null for any other
 * tool. For a full read, which no leaf cap has clipped. */
export function editSides(name: string, args: ToolArgs | undefined): { before: string; after: string } | null {
  return name === TOOL_NAME.FILE_EDIT || name === TOOL_NAME.FILE_EDIT_HASHED ? editTexts(args) : null;
}

export function pathArg(args: ToolArgs | undefined): string {
  return workspacePath(stringArg(args, ARG.PATH) ?? "");
}

function plainCopy(verbs: Verbs, target: string): ToolCopy {
  return { verbs, target, body: { kind: TOOL_BODY.OUTPUT } };
}

function requestCopy(args: ToolArgs): ToolCopy {
  return plainCopy(VERBS.REQUEST, `${stringArg(args, ARG.METHOD) ?? DEFAULT_METHOD} ${stringArg(args, ARG.URL) ?? ""}`.trim());
}

function writeCopy(verbs: Verbs, args: ToolArgs): ToolCopy {
  const content = stringArg(args, ARG.CONTENT);
  const copy = plainCopy(verbs, pathArg(args));
  return content === undefined || mayBeClipped(content) ? copy : { ...copy, addedLines: linesOf(content).length };
}

function editCopy(args: ToolArgs): ToolCopy {
  const { before, after } = editTexts(args);
  const body: ToolBody = mayBeClipped(before) || mayBeClipped(after)
    ? { kind: TOOL_BODY.CLIPPED_EDIT }
    : { kind: TOOL_BODY.EDIT, before, after };
  return { verbs: VERBS.EDIT, target: pathArg(args), body };
}

function editTexts(args: ToolArgs | undefined): { before: string; after: string } {
  return { before: stringArg(args, ARG.OLD) ?? "", after: stringArg(args, ARG.NEW) ?? "" };
}

function commandCopy(command: string | undefined): ToolCopy {
  const [first = "", ...rest] = linesOf(command ?? "");
  const rail = rest.slice(0, COMMAND_RAIL_ROWS);
  return { verbs: VERBS.RUN, target: first, body: { kind: TOOL_BODY.COMMAND, rail, hiddenLines: rest.length - rail.length } };
}
