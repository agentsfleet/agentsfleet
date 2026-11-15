import { describe, expect, it } from "vitest";

import { DIFF_ROW, lineDiff } from "./tool-call-diff";

describe("lineDiff", () => {
  it("test_edit_diff_counts_lines — a\\nb → a\\nc\\nd counts +2 −1 with the shared line kept", () => {
    const diff = lineDiff("a\nb", "a\nc\nd");
    expect(diff.added).toBe(2);
    expect(diff.removed).toBe(1);
    expect(diff.rows).toEqual([
      { kind: DIFF_ROW.CONTEXT, text: "a" },
      { kind: DIFF_ROW.REMOVED, text: "b" },
      { kind: DIFF_ROW.ADDED, text: "c" },
      { kind: DIFF_ROW.ADDED, text: "d" },
    ]);
  });

  it("keeps a shared last line as context whether or not either side ends in a line break", () => {
    expect(lineDiff("x\nkeep", "keep\nnew").rows).toEqual([
      { kind: DIFF_ROW.REMOVED, text: "x" },
      { kind: DIFF_ROW.CONTEXT, text: "keep" },
      { kind: DIFF_ROW.ADDED, text: "new" },
    ]);
    expect(lineDiff("a\nb\n", "a\nb").added).toBe(0);
  });

  it("reads an unchanged edit as context only", () => {
    expect(lineDiff("same\n", "same\n")).toEqual({ rows: [{ kind: DIFF_ROW.CONTEXT, text: "same" }], added: 0, removed: 0 });
  });

  it("falls back to a whole replacement when the edit is past jsdiff's search bound", () => {
    // Two thousand distinct lines on each side, none shared: more edits than the bound allows.
    const before = Array.from({ length: 2_000 }, (_, i) => `old ${i}`).join("\n");
    const after = Array.from({ length: 2_000 }, (_, i) => `new ${i}`).join("\n");
    const diff = lineDiff(before, after);
    expect(diff.removed).toBe(2_000);
    expect(diff.added).toBe(2_000);
    expect(diff.rows[0]).toEqual({ kind: DIFF_ROW.REMOVED, text: "old 0" });
  });
});
