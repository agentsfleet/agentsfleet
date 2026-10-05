import type { JsonValue, ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { truncate } from "@/lib/utils";

// The text a tool cell draws from a call's arguments and output, read the way
// the runner wrote it: argument strings it may have cut at its leaf cap,
// sandbox paths, lines, and terminal output. Split out of `tool-call-copy` so
// every copy map shares one reading.

/** `afd_wire::tool_trace::ARGS_LEAF_MAX_BYTES`: the runner cuts any longer
 * argument string to this many bytes, on a character boundary, unmarked. */
export const ARGS_LEAF_MAX_BYTES = 256;
/** Marks text the runner may have cut short at its argument leaf cap. */
export const CLIP_MARK = "…";
// A cut lands on the last whole character, at most one character short.
const UTF8_MAX_CHAR_BYTES = 4;
const WORKSPACE_ROOT = "/workspace/";
const PATH_ARG = "path";
const LINE_BREAK = /\r?\n/;
// Terminal control sequences a command's output carries: CSI (colour, cursor),
// OSC (titles, links) ended by BEL or ST, and lone two-byte escapes (keypad
// modes `=` and `>` among them).
const TERMINAL_CONTROL = /\u001B\[[0-?]*[ -/]*[@-~]|\u001B\][^\u0007\u001B]*(?:\u0007|\u001B\\)|\u001B[=>@-Z\\-_]/g;
const CARRIAGE_RETURN = "\r";
const UTF8 = new TextEncoder();
const COMPACT_ARGS_MAX_CHARS = 120;

/** Whether a string argument may have been cut by the runner. */
export function mayBeClipped(text: string): boolean {
  return UTF8.encode(text).length > ARGS_LEAF_MAX_BYTES - UTF8_MAX_CHAR_BYTES;
}

/** Text marked where the runner may have cut it short. */
export function clipMarked(text: string): string {
  return mayBeClipped(text) ? `${text}${CLIP_MARK}` : text;
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

/** Output as a terminal would have shown it: control sequences dropped, and a
 * line a carriage return overwrote (a progress bar) left at its last state. */
export function outputLines(text: string): string[] {
  return linesOf(text.replace(TERMINAL_CONTROL, "")).map((line) => line.slice(line.lastIndexOf(CARRIAGE_RETURN) + 1));
}

/** A string argument, or undefined when it is absent or not a string. */
export function stringArg(args: ToolArgs | undefined, key: string): string | undefined {
  const value: JsonValue | undefined = args?.[key];
  return typeof value === "string" ? value : undefined;
}

/** The first of several spellings a runner may use for one argument. */
export function firstString(args: ToolArgs | undefined, keys: readonly string[]): string | undefined {
  for (const key of keys) {
    const value = stringArg(args, key);
    if (value !== undefined) return value;
  }
  return undefined;
}

/** A string or a number argument, as text: ids arrive as either. */
export function scalarArg(args: ToolArgs | undefined, key: string): string | undefined {
  const value: JsonValue | undefined = args?.[key];
  return typeof value === "string" || typeof value === "number" ? String(value) : undefined;
}

/** The path a call names, relative to the workspace and marked where the
 * runner may have cut it. */
export function pathArg(args: ToolArgs | undefined): string {
  return clipMarked(workspacePath(stringArg(args, PATH_ARG) ?? ""));
}

/** A text argument's first line, marked if the line or the text was cut. */
export function firstLine(text: string): string {
  const [first = ""] = linesOf(text);
  return mayBeClipped(text) || first.length < text.trimEnd().length ? `${first}${CLIP_MARK}` : first;
}

/** Arguments as they came, compact and bounded: Codex's `name(arguments)`. */
export function compactArgs(args: ToolArgs): string {
  return `(${truncate(JSON.stringify(args), COMPACT_ARGS_MAX_CHARS)})`;
}
