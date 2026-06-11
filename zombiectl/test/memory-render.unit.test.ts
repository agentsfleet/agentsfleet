// Pure render-helper tests for the memory read verbs, split from
// memory.unit.test.ts for the 350-line file cap. previewText is the
// UTF-8-safety surface; renderUpdatedAt is the isolated wire-timestamp
// helper (the one spot that changes when the wire goes numeric).

import { describe, test, expect } from "bun:test";

import { cleanCell, previewText, renderUpdatedAt } from "../src/commands/memory.ts";

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

describe("renderUpdatedAt — the isolated wire-timestamp helper", () => {
  // pin test: literal is the contract — both wire shapes name the same
  // instant, so both pin to the same ISO string (no re-derived conversion).
  test("epoch-seconds string (today's wire) renders ISO 8601", () => {
    expect(renderUpdatedAt("1765500300")).toBe("2025-12-12T00:45:00.000Z");
  });

  test("numeric epoch millis (the incoming wire shape) renders ISO 8601", () => {
    expect(renderUpdatedAt(1765500300000)).toBe("2025-12-12T00:45:00.000Z");
  });

  test("null, undefined, and non-numeric strings render the dash", () => {
    expect(renderUpdatedAt(null)).toBe("—");
    expect(renderUpdatedAt(undefined)).toBe("—");
    expect(renderUpdatedAt("not-a-timestamp")).toBe("—");
  });

  test("out-of-range wire values render the dash instead of throwing", () => {
    // past Date's ±8.64e15 ms ceiling — a RangeError here would kill the table
    expect(renderUpdatedAt(9.7e15)).toBe("—");
    expect(renderUpdatedAt("99999999999999999999")).toBe("—");
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
  });

  test("null, undefined, and empty content render as empty previews", () => {
    expect(previewText(null)).toBe("");
    expect(previewText(undefined)).toBe("");
    expect(previewText("")).toBe("");
  });
});
