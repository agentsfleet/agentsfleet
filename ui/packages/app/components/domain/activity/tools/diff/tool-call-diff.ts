import { diffLines, type Change } from "diff";

import { linesOf } from "../presentation/tool-call-text";

// An edit's text as a diff: the lines it removed and added, with the
// lines they share between, for a full read to show what the call asked for.
// jsdiff builds the rows from the edit's own arguments; how many lines the
// edit changed is the runner's recorded count, never these rows'.

export const DIFF_ROW = {
  ADDED: "added",
  REMOVED: "removed",
  CONTEXT: "context",
} as const;

export type DiffRow = { kind: (typeof DIFF_ROW)[keyof typeof DIFF_ROW]; text: string };

/** The rows of `before` → `after`. */
export function lineRows(before: string, after: string): DiffRow[] {
  // An edit's text rarely ends in a line break; its last line is the same line
  // whether or not the other side's does.
  const changes = diffLines(before, after, { ignoreNewlineAtEof: true });
  return changes.flatMap((change) => linesOf(change.value).map((text) => ({ kind: rowKind(change), text })));
}

function rowKind(change: Change): DiffRow["kind"] {
  if (change.added) return DIFF_ROW.ADDED;
  return change.removed ? DIFF_ROW.REMOVED : DIFF_ROW.CONTEXT;
}
