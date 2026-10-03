// Pure render-helper tests for the memory read verbs, split from
// memory.unit.test.ts for the 350-line file cap. previewText is the
// UTF-8-safety surface. The wire-timestamp helper this file also covered is
// gone: the age column renders through ago, whose own tests carry every claim
// this file used to make about malformed, null and out-of-range instants.

import { describe, test, expect } from "bun:test";
import { Exit } from "effect";

import { cleanCell, memoryListEffectFromFlags, previewText, sharedBy } from "../src/commands/memory.ts";
import { httpLayerReturning, newCapture, runWith } from "./helpers-memory-layers.ts";

const FLEET_ID = "01900000-0000-7000-8000-0000005e4e72";
const WRITER_ID = "01900000-0000-7000-8000-0000005e4e73";

describe("sharedBy — another fleet's entry names its writer", () => {
  test("an entry this fleet wrote, or one naming no writer, leaves the cell empty", () => {
    expect(sharedBy({ writer_fleet_id: FLEET_ID }, FLEET_ID)).toBe("");
    expect(sharedBy({}, FLEET_ID)).toBe("");
    expect(sharedBy({ writer_fleet_id: null }, FLEET_ID)).toBe("");
    expect(sharedBy({ writer_fleet_id: "" }, FLEET_ID)).toBe("");
  });

  test("another fleet's entry carries the writer, its control bytes stripped", () => {
    expect(sharedBy({ writer_fleet_id: WRITER_ID }, FLEET_ID)).toBe(WRITER_ID);
    expect(sharedBy({ writer_fleet_id: `\u001b[31m${WRITER_ID}` }, FLEET_ID)).toBe(`[31m${WRITER_ID}`);
  });

  test("the list table marks only the shared entry", async () => {
    const cap = newCapture();
    const envelope = {
      items: [
        { key: "own", content: "a", category: "core", updated_at: 1765500300000, writer_fleet_id: FLEET_ID },
        { key: "theirs", content: "b", category: "core", updated_at: 1765500200000, writer_fleet_id: WRITER_ID },
      ],
      total: 2,
      next_cursor: null,
    };
    const exit = await runWith(memoryListEffectFromFlags({ fleetId: FLEET_ID }), {
      http: httpLayerReturning(envelope, []),
      cap,
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.tables[0]?.columns.map((c) => c.label)).toContain("SHARED BY");
    const rows = cap.tables[0]?.rows ?? [];
    expect(rows.map((r) => r["shared_by"])).toEqual(["", WRITER_ID]);
  });
});

describe("cleanCell — server content can't drive the operator's terminal", () => {
  test("strips ESC/BEL/CSI control bytes that carry ANSI and OSC sequences", () => {
    expect(cleanCell("\u001b]52;c;payload\u0007safe")).toBe("]52;c;payloadsafe");
    expect(cleanCell("\u001b[2Jcleared")).toBe("[2Jcleared");
    expect(cleanCell("\u009b31mred")).toBe("31mred"); // C1 CSI introducer
  });

  test("null and undefined render as empty cells; plain text passes through", () => {
    expect(cleanCell(null)).toBe("");
    expect(cleanCell(undefined)).toBe("");
    expect(cleanCell("na\u00efve caf\u00e9 \u{1f989}")).toBe("na\u00efve caf\u00e9 \u{1f989}");
  });
});

describe("test_memory_preview_truncation_utf8_safe", () => {
  test("ASCII content over the cap truncates with an ellipsis", () => {
    const out = previewText("A".repeat(200));
    expect(out.endsWith("…")).toBe(true);
    expect(Array.from(out)).toHaveLength(80);
  });

  test("multibyte content at the boundary never splits a surrogate pair", () => {
    // 100 owls — each is one code point but two UTF-16 units; a naive
    // .slice(0, n) would cut mid-pair and emit a lone surrogate.
    const out = previewText("🦉".repeat(100));
    expect(out.isWellFormed()).toBe(true);
    expect(Array.from(out)).toHaveLength(80);
    expect(out.endsWith("…")).toBe(true);
    // round-trips through UTF-8 byte-identically
    expect(Buffer.from(out, "utf8").toString("utf8")).toBe(out);
  });

  test("short multibyte content passes through untouched", () => {
    expect(previewText("naïve café 🦉")).toBe("naïve café 🦉");
  });

  test("exactly-80 code points pass through; 81 truncates to 80 with ellipsis", () => {
    expect(previewText("A".repeat(80))).toBe("A".repeat(80));
    const out = previewText("A".repeat(81));
    expect(Array.from(out)).toHaveLength(80);
    expect(out.endsWith("…")).toBe(true);
  });

  test("embedded ESC sequences are stripped before measuring", () => {
    expect(previewText("safe\u001b[31mred")).toBe("safe[31mred");
  });

  test("whitespace collapses to single spaces before measuring", () => {
    expect(previewText("a\n\n  b\t c")).toBe("a b c");
    // no adjacent literal spaces — proves the newline itself becomes a
    // space (cleanCell after collapse, not before)
    expect(previewText("line1\nline2")).toBe("line1 line2");
    expect(previewText("col1\tcol2")).toBe("col1 col2");
  });

  test("null, undefined, and empty content render as empty previews", () => {
    expect(previewText(null)).toBe("");
    expect(previewText(undefined)).toBe("");
    expect(previewText("")).toBe("");
  });
});
