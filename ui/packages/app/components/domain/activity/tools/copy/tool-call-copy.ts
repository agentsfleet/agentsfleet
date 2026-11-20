import type { FileChange, ToolArgs, ToolCallStatus } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { RUNNER_COPY, TAKES_NO_ARGS, patchRows } from "./tool-call-copy-runner";
import { lineRows, type DiffRow } from "../diff/tool-call-diff";
import { ARGS_NOT_RECORDED, TOOL_BODY, TOOL_NAME, plainCopy, type ToolCopy, type Verbs } from "../presentation/tool-call-shape";
import { CLIP_MARK, clipMarked, compactArgs, linesOf, mayBeClipped, outputLines, pathArg, stringArg } from "../presentation/tool-call-text";

// What a tool call says in the thread, the way Codex's transcript says it: a
// verb that is `-ing` while the call runs and past tense once it returns, the
// thing it touched, and what came back. Verbs come from the tool's name
// because hosted tools take structured arguments; Codex parses a shell line.
// Pure: a call's name, arguments and outcome in, words out. Files, requests,
// memory and commands are here; every other runner tool reads in
// `tool-call-copy-runner`. An edit's files and line counts are the ones the
// runner's file tool recorded, never worked out from its arguments.

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

export const OUTPUT_PREVIEW_ROWS = 3;
export const COMMAND_RAIL_ROWS = 2;
const DEFAULT_METHOD = "GET";

export const EMPTY_OUTPUT = "(no output)";
export const OUTPUT_UNAVAILABLE = "(output unavailable)";

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
  OLD: "old_text",
  NEW: "new_text",
  // A hashed edit names the line it changes by its Hashline tag (`L10:abc`).
  ANCHOR: "target",
  ANCHOR_END: "end_target",
  URL: "url",
  METHOD: "method",
  KEY: "key",
  CMD: "cmd",
  COMMAND: "command",
  PATCH: "patch",
} as const;

// The tools whose answer the runner reads an edit's changes from, and the
// words each says them in. A Map, so a tool named `toString` reads as unknown.
const EDITS: ReadonlyMap<string, Verbs> = new Map<string, Verbs>([
  [TOOL_NAME.FILE_WRITE, VERBS.WRITE],
  [TOOL_NAME.FILE_APPEND, VERBS.APPEND],
  [TOOL_NAME.FILE_EDIT, VERBS.EDIT],
  [TOOL_NAME.FILE_EDIT_HASHED, VERBS.EDIT],
  [TOOL_NAME.APPLY_PATCH, VERBS.EDIT],
]);

// A Map, not an object literal: a tool named `toString` must read as unknown.
const COPY: ReadonlyMap<string, (args: ToolArgs) => ToolCopy> = new Map([
  [TOOL_NAME.FILE_DELETE, (args) => plainCopy(VERBS.DELETE, pathArg(args))],
  [TOOL_NAME.HTTP_REQUEST, requestCopy],
  [TOOL_NAME.MEMORY_STORE, (args) => plainCopy(VERBS.REMEMBER, clipMarked(stringArg(args, ARG.KEY) ?? ""))],
  [TOOL_NAME.MEMORY_FORGET, (args) => plainCopy(VERBS.FORGET, clipMarked(stringArg(args, ARG.KEY) ?? ""))],
  [TOOL_NAME.EXEC_COMMAND, (args) => commandCopy(stringArg(args, ARG.CMD))],
  [TOOL_NAME.SHELL, (args) => commandCopy(stringArg(args, ARG.COMMAND))],
  ...RUNNER_COPY,
]);

/** A call's verbs, target and body. An edit names the `changes` the runner
 * recorded for it. A tool this map does not know reads as Codex reads any
 * dynamic tool: `Called name(arguments)`. */
export function toolCopy(name: string, args: ToolArgs | undefined, changes: readonly FileChange[] = []): ToolCopy {
  const edit = EDITS.get(name);
  if (edit !== undefined) return editCopy(edit, name, args, changes);
  const known = COPY.get(name);
  if (known !== undefined) {
    if (args !== undefined || TAKES_NO_ARGS.has(name)) return known(args ?? {});
    return plainCopy(known({}).verbs, ARGS_NOT_RECORDED);
  }
  return plainCopy(VERBS.CALL, args === undefined ? name : `${name}${compactArgs(args)}`);
}

/** How a cell reads from its outcome. A call with no outcome that is no
 * longer running was ended by its turn: no call is drawn running after it. */
export function cellState(hasResult: boolean, status: ToolCallStatus | undefined, partRunning: boolean): CellState {
  if (!hasResult) return partRunning ? CELL_STATE.RUNNING : CELL_STATE.INTERRUPTED;
  return status ?? CELL_STATE.SETTLED;
}

/** `cut`: the runner kept only the edges of this output, so more exists than
 * the rows show even when no line was left out (a long single line). */
export type OutputPreview = { rows: readonly string[]; hiddenLines: number; note: string | null; cut: boolean };

/** Codex's preview: the first rows of output, then how many lines it left
 * out. With no rows, a note says whether there was nothing or nothing kept:
 * only a call that returned and said how can claim it printed nothing. */
export function outputPreview(
  head: string | undefined,
  lineCount: number | undefined,
  status: ToolCallStatus | undefined,
  tail?: string,
): OutputPreview {
  const lines = head === undefined ? [] : outputLines(head);
  const rows = lines.slice(0, OUTPUT_PREVIEW_ROWS);
  const hiddenLines = Math.max(0, (lineCount ?? lines.length) - rows.length);
  // The runner sends a tail only when it cut the head.
  const cut = tail !== undefined && hiddenLines === 0;
  if (rows.length > 0) return { rows, hiddenLines, note: null, cut };
  const unknown = status === undefined || status === TOOL_CALL_STATUS.INTERRUPTED || hiddenLines > 0;
  return { rows, hiddenLines, note: unknown ? OUTPUT_UNAVAILABLE : EMPTY_OUTPUT, cut };
}

/** "+5 lines", "+1 line": what a preview left out. */
export function moreLinesLabel(hidden: number): string {
  return `+${hidden} ${hidden === 1 ? "line" : "lines"}`;
}

/** An edit's or a patch's whole text as rows, for a full read, which no leaf
 * cap has clipped; null for any other tool, or one whose arguments were not
 * kept or name no text to compare (a hashed edit by line tag). The rows are
 * what the call asked for; its counts are the recorded changes'. */
export function fullDiff(name: string, args: ToolArgs | undefined): DiffRow[] | null {
  if (args === undefined) return null;
  if (name === TOOL_NAME.APPLY_PATCH) return patchRows(stringArg(args, ARG.PATCH) ?? "");
  if (name !== TOOL_NAME.FILE_EDIT && name !== TOOL_NAME.FILE_EDIT_HASHED) return null;
  const before = stringArg(args, ARG.OLD);
  return before === undefined ? null : lineRows(before, stringArg(args, ARG.NEW) ?? "");
}

function requestCopy(args: ToolArgs): ToolCopy {
  return plainCopy(VERBS.REQUEST, `${stringArg(args, ARG.METHOD) ?? DEFAULT_METHOD} ${clipMarked(stringArg(args, ARG.URL) ?? "")}`.trim());
}

/** An edit names the files it changed and the lines each gained and lost,
 * as the runner recorded them. Until it has (still running, failed, or
 * recorded nothing), it names the file it was asked to change, and nothing
 * for a patch, whose files would be read out of its text. */
function editCopy(verbs: Verbs, name: string, args: ToolArgs | undefined, changes: readonly FileChange[]): ToolCopy {
  if (changes.length > 0) {
    return { verbs, target: changes.map((change) => change.path).join(", "), body: { kind: TOOL_BODY.CHANGES, changes } };
  }
  if (args === undefined) return plainCopy(verbs, ARGS_NOT_RECORDED);
  if (name === TOOL_NAME.APPLY_PATCH) return plainCopy(verbs, "");
  // A hashed edit names its line by tag.
  return plainCopy(verbs, `${pathArg(args)}${anchorOf(args)}`);
}

function anchorOf(args: ToolArgs): string {
  const from = stringArg(args, ARG.ANCHOR);
  if (from === undefined) return "";
  const to = stringArg(args, ARG.ANCHOR_END);
  return to === undefined ? ` at ${from}` : ` at ${from}–${to}`;
}

function commandCopy(command: string | undefined): ToolCopy {
  const [first = "", ...rest] = linesOf(command ?? "");
  const rail = rest.slice(0, COMMAND_RAIL_ROWS);
  const hiddenLines = rest.length - rail.length;
  // A cut command ends early: mark its last line shown, unless a "… +N lines"
  // note already says more follows.
  const clipped = command !== undefined && mayBeClipped(command) && hiddenLines === 0;
  const last = rail.at(-1);
  if (clipped && last === undefined) return { verbs: VERBS.RUN, target: `${first}${CLIP_MARK}`, body: { kind: TOOL_BODY.COMMAND, rail, hiddenLines } };
  const shownRail = clipped && last !== undefined ? [...rail.slice(0, -1), `${last}${CLIP_MARK}`] : rail;
  return { verbs: VERBS.RUN, target: first, body: { kind: TOOL_BODY.COMMAND, rail: shownRail, hiddenLines } };
}
