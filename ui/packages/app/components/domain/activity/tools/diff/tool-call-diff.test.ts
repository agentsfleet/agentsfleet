import { describe, expect, it } from "vitest";

import { DIFF_ROW, lineRows } from "./tool-call-diff";

describe("lineRows", () => {
  it("draws a\\nb → a\\nc\\nd with the shared line kept", () => {
    expect(lineRows("a\nb", "a\nc\nd")).toEqual([
      { kind: DIFF_ROW.CONTEXT, text: "a" },
      { kind: DIFF_ROW.REMOVED, text: "b" },
      { kind: DIFF_ROW.ADDED, text: "c" },
      { kind: DIFF_ROW.ADDED, text: "d" },
    ]);
  });

  it("keeps a shared last line as context whether or not either side ends in a line break", () => {
    expect(lineRows("x\nkeep", "keep\nnew")).toEqual([
      { kind: DIFF_ROW.REMOVED, text: "x" },
      { kind: DIFF_ROW.CONTEXT, text: "keep" },
      { kind: DIFF_ROW.ADDED, text: "new" },
    ]);
    expect(lineRows("a\nb\n", "a\nb").every((row) => row.kind === DIFF_ROW.CONTEXT)).toBe(true);
  });

  it("reads an unchanged edit as context only", () => {
    expect(lineRows("same\n", "same\n")).toEqual([{ kind: DIFF_ROW.CONTEXT, text: "same" }]);
  });

  it("falls back to a whole replacement when the edit is past jsdiff's search bound", () => {
    // Two thousand distinct lines on each side, none shared: more edits than the bound allows.
    const before = Array.from({ length: 2_000 }, (_, i) => `old ${i}`).join("\n");
    const after = Array.from({ length: 2_000 }, (_, i) => `new ${i}`).join("\n");
    const rows = lineRows(before, after);
    expect(rows.filter((row) => row.kind === DIFF_ROW.REMOVED)).toHaveLength(2_000);
    expect(rows[0]).toEqual({ kind: DIFF_ROW.REMOVED, text: "old 0" });
  });
});
