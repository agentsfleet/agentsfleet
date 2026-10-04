import { diffLines, type Change } from "diff";

import { linesOf } from "./tool-call-copy";

// An edit as Codex draws it: the lines it removed and added, with the lines
// they share between. jsdiff builds it from the edit's own arguments, so a
// cell shows what changed without reading the file.

export const DIFF_ROW = {
  ADDED: "added",
  REMOVED: "removed",
  CONTEXT: "context",
} as const;

export type DiffRow = { kind: (typeof DIFF_ROW)[keyof typeof DIFF_ROW]; text: string };

export type LineDiff = { rows: DiffRow[]; added: number; removed: number };

// Past this many edits jsdiff stops searching and returns nothing; the edit
// then reads as every old line removed and every new one added, still true.
const MAX_EDIT_LENGTH = 1_000;

/** The rows of `before` → `after`, and how many each side changed. */
export function lineDiff(before: string, after: string): LineDiff {
  // An edit's text rarely ends in a line break; its last line is the same line
  // whether or not the other side's does.
  const changes = diffLines(before, after, { maxEditLength: MAX_EDIT_LENGTH, ignoreNewlineAtEof: true })
    ?? wholeReplacement(before, after);
  const rows = changes.flatMap((change) => linesOf(change.value).map((text) => ({ kind: rowKind(change), text })));
  return {
    rows,
    added: rows.filter((row) => row.kind === DIFF_ROW.ADDED).length,
    removed: rows.filter((row) => row.kind === DIFF_ROW.REMOVED).length,
  };
}

function rowKind(change: Change): DiffRow["kind"] {
  if (change.added) return DIFF_ROW.ADDED;
  return change.removed ? DIFF_ROW.REMOVED : DIFF_ROW.CONTEXT;
}

function wholeReplacement(before: string, after: string): Change[] {
  return [
    { value: before, added: false, removed: true, count: linesOf(before).length },
    { value: after, added: true, removed: false, count: linesOf(after).length },
  ];
}
