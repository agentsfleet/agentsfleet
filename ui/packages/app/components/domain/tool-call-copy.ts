import type { ToolArgs, ToolCallStatus } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { RUNNER_COPY, TAKES_NO_ARGS, patchDiff } from "./tool-call-copy-runner";
import { lineDiff, type LineDiff } from "./tool-call-diff";
import { ARGS_NOT_RECORDED, TOOL_BODY, TOOL_NAME, plainCopy, type ToolBody, type ToolCopy, type Verbs } from "./tool-call-shape";
import { CLIP_MARK, clipMarked, compactArgs, linesOf, mayBeClipped, outputLines, pathArg, stringArg } from "./tool-call-text";

// What a tool call says in the thread, the way Codex's transcript says it: a
// verb that is `-ing` while the call runs and past tense once it returns, the
// thing it touched, and what came back. Verbs come from the tool's name
// because hosted tools take structured arguments; Codex parses a shell line.
// Pure: a call's name, arguments and outcome in, words out. Files, requests,
// memory and commands are here; every other runner tool reads in
// `tool-call-copy-runner`.

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
  CONTENT: "content",
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

// A Map, not an object literal: a tool named `toString` must read as unknown.
const COPY: ReadonlyMap<string, (args: ToolArgs) => ToolCopy> = new Map([
  [TOOL_NAME.FILE_WRITE, (args) => writeCopy(VERBS.WRITE, args)],
  [TOOL_NAME.FILE_APPEND, (args) => writeCopy(VERBS.APPEND, args)],
  [TOOL_NAME.FILE_DELETE, (args) => plainCopy(VERBS.DELETE, pathArg(args))],
  [TOOL_NAME.FILE_EDIT, editCopy],
  [TOOL_NAME.FILE_EDIT_HASHED, editCopy],
  [TOOL_NAME.HTTP_REQUEST, requestCopy],
  [TOOL_NAME.MEMORY_STORE, (args) => plainCopy(VERBS.REMEMBER, clipMarked(stringArg(args, ARG.KEY) ?? ""))],
  [TOOL_NAME.MEMORY_FORGET, (args) => plainCopy(VERBS.FORGET, clipMarked(stringArg(args, ARG.KEY) ?? ""))],
  [TOOL_NAME.EXEC_COMMAND, (args) => commandCopy(stringArg(args, ARG.CMD))],
  [TOOL_NAME.SHELL, (args) => commandCopy(stringArg(args, ARG.COMMAND))],
  ...RUNNER_COPY,
]);

/** A call's verbs, target and body. A tool this map does not know reads as
 * Codex reads any dynamic tool: `Called name(arguments)`. */
export function toolCopy(name: string, args: ToolArgs | undefined): ToolCopy {
  const known = COPY.get(name);
  if (known !== undefined) {
    if (args !== undefined || TAKES_NO_ARGS.has(name)) return known(args ?? {});
    return plainCopy(known({}).verbs, ARGS_NOT_RECORDED);
  }
  return plainCopy(VERBS.CALL, args === undefined ? name : `${name}${compactArgs(args)}`);
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

/** An edit's or a patch's whole diff, for a full read, which no leaf cap has
 * clipped; null for any other tool, or one whose arguments were not kept or
 * name no text to compare (a hashed edit by line tag). */
export function fullDiff(name: string, args: ToolArgs | undefined): LineDiff | null {
  if (args === undefined) return null;
  if (name === TOOL_NAME.APPLY_PATCH) return patchDiff(stringArg(args, ARG.PATCH) ?? "").diff;
  if (name !== TOOL_NAME.FILE_EDIT && name !== TOOL_NAME.FILE_EDIT_HASHED) return null;
  const before = stringArg(args, ARG.OLD);
  return before === undefined ? null : lineDiff(before, stringArg(args, ARG.NEW) ?? "");
}

function requestCopy(args: ToolArgs): ToolCopy {
  return plainCopy(VERBS.REQUEST, `${stringArg(args, ARG.METHOD) ?? DEFAULT_METHOD} ${clipMarked(stringArg(args, ARG.URL) ?? "")}`.trim());
}

function writeCopy(verbs: Verbs, args: ToolArgs): ToolCopy {
  const content = stringArg(args, ARG.CONTENT);
  const copy = plainCopy(verbs, pathArg(args));
  return content === undefined || mayBeClipped(content) ? copy : { ...copy, addedLines: linesOf(content).length };
}

function editCopy(args: ToolArgs): ToolCopy {
  const before = stringArg(args, ARG.OLD);
  const after = stringArg(args, ARG.NEW) ?? "";
  // A hashed edit names its line by tag and carries no old text: there is
  // nothing to diff against, so the cell names the line instead.
  if (before === undefined) return plainCopy(VERBS.EDIT, `${pathArg(args)}${anchorOf(args)}`);
  const body: ToolBody = mayBeClipped(before) || mayBeClipped(after)
    ? { kind: TOOL_BODY.CLIPPED_EDIT }
    : { kind: TOOL_BODY.DIFF, diff: lineDiff(before, after) };
  return { verbs: VERBS.EDIT, target: pathArg(args), body };
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
