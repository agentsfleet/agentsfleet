import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { ARGS_LEAF_MAX_BYTES, linesOf, mayBeClipped, stringArg, workspacePath } from "@/components/domain/activity/tools/presentation/tool-call-text";

type Lines = {added: number; removed: number};
export type PatchChange = {path: string; lines: Lines | undefined};
type Receipt = {paths: string[]; complete: boolean; lines: Lines | undefined};
type Hunk = {path: string; kind: string; lines: Lines; certain: boolean};
const SUCCESS = "Success. Updated the following files:";
const FILE = /^([AMD]) (.+)$/;
const HUNK = /^\*\*\* (Add|Update|Delete) File: (.+)$/;
const MOVE = "*** Move to: ";
const TOTAL = /^\+(\d+) −(\d+)$/;
/** `af_wire::tool_trace::OUTPUT_EDGE_MAX_BYTES`; a byte cut can lose one UTF-8 character. */
const EDGE_BYTES = 1024, UTF8_MAX_BYTES = 4;
const UTF8 = new TextEncoder();
const normalizedPath = (path: string) => workspacePath(path).split("/").filter(part => part !== "." && part !== "").join("/");

/** Native receipts retain paths and aggregate counts even when the argument leaf was cut. */
export function recordedPatchChanges(tool: FleetToolCall, running: boolean): PatchChange[] {
  const receipt = nativeReceipt(tool);
  const [only] = receipt.paths;
  if (!running && receipt.complete && receipt.paths.length === 1 && only !== undefined && receipt.lines) return [{path: only, lines: receipt.lines}];
  const parsed = patchHunks(stringArg(tool.args, "patch"));
  const paths = [...new Set([...receipt.paths, ...parsed.keys()])];
  if (running) return paths.map(path => ({path, lines: undefined}));
  const changes = paths.map(path => ({path, lines: parsed.get(path)}));
  const complete = receipt.complete || parsed.size > 0 && receipt.paths.every(path => parsed.has(path));
  return reconcile(changes, complete ? receipt.lines : undefined);
}

/** Head and tail overlap by line number; neither byte-cut boundary is a complete path. */
function nativeReceipt(tool: FleetToolCall): Receipt {
  const headText = tool.outputHead ?? "", tailText = tool.outputTail ?? "";
  const head = linesOf(headText), tail = linesOf(tailText);
  if (head[0] !== SUCCESS) return {paths: [], complete: false, lines: undefined};
  if (tailText && !headText.endsWith("\n")) head.pop();
  const tailCut = tailText.length >= EDGE_BYTES - UTF8_MAX_BYTES && UTF8.encode(tailText).length >= EDGE_BYTES - UTF8_MAX_BYTES;
  const kept = new Map(head.map((line, index) => [index, line]));
  if (tool.outputLineCount !== undefined) for (const [index, line] of tail.entries()) {
    if (index !== 0 || !tailCut) kept.set(tool.outputLineCount - tail.length + index, line);
  }
  const last = tail.at(-1) ?? head.at(-1);
  const match = last?.match(TOTAL);
  const lines = match ? {added: Number(match[1]), removed: Number(match[2])} : undefined;
  const rows = [...kept.values()];
  const complete = Boolean(lines) && (tail.length === 0 || kept.size === tool.outputLineCount);
  const paths = [...new Set(rows.flatMap(line => {
    const path = line.match(FILE)?.[2];
    return path ? [normalizedPath(path)] : [];
  }))];
  return {paths, complete, lines};
}

/** Only a whole, unclipped argument supports allocating counts to separate files. */
function patchHunks(patch: string | undefined): Map<string, Lines | undefined> {
  const files = new Map<string, Lines | undefined>();
  if (patch === undefined || patch.length > ARGS_LEAF_MAX_BYTES || mayBeClipped(patch)) return files;
  const rows = linesOf(patch.trim());
  if (rows[0]?.trim() !== "*** Begin Patch" || rows.at(-1)?.trim() !== "*** End Patch") return files;
  let hunk: Hunk | undefined;
  for (const line of rows.slice(1, -1)) {
    const header = line.match(HUNK);
    if (header?.[1] && header[2]) {
      if (hunk) retain(files, hunk);
      hunk = {path: normalizedPath(header[2]), kind: header[1], lines: {added: 0, removed: 0}, certain: header[1] !== "Delete"};
    } else if (hunk) {
      if (line.startsWith(MOVE)) hunk.path = normalizedPath(line.slice(MOVE.length));
      else if (line.startsWith("+")) hunk.lines.added += 1;
      else if (line.startsWith("-")) hunk.lines.removed += 1;
      // An empty changed line in an update may be an EOF sentinel the runner removes.
      if (hunk.kind === "Update" && (line === "+" || line === "-")) hunk.certain = false;
    }
  }
  if (hunk) retain(files, hunk);
  return files;
}

function retain(files: Map<string, Lines | undefined>, hunk: Hunk) {
  const previous = files.get(hunk.path);
  const lines = hunk.certain && (!files.has(hunk.path) || previous) ? {
    added: (previous?.added ?? 0) + hunk.lines.added, removed: (previous?.removed ?? 0) + hunk.lines.removed,
  } : undefined;
  files.set(hunk.path, lines);
}

/** One unknown file can use the actual remainder; multiple unknown files cannot share a guess. */
function reconcile(changes: PatchChange[], actual: Lines | undefined): PatchChange[] {
  if (!actual) return changes;
  const unknown = changes.filter(change => !change.lines);
  const known = changes.reduce((sum, change) => ({added: sum.added + (change.lines?.added ?? 0), removed: sum.removed + (change.lines?.removed ?? 0)}), {added: 0, removed: 0});
  const remaining = {added: actual.added - known.added, removed: actual.removed - known.removed};
  if (remaining.added < 0 || remaining.removed < 0 || unknown.length === 0 && (remaining.added !== 0 || remaining.removed !== 0)) return changes.map(change => ({...change, lines: undefined}));
  if (unknown.length === 1) return changes.map(change => change.lines ? change : {...change, lines: remaining});
  return changes;
}
