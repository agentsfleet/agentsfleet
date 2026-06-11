// Pure render-helper tests for the memory read verbs, split from
// memory.unit.test.ts for the 350-line file cap. previewText is the
// UTF-8-safety surface; renderUpdatedAt is the isolated wire-timestamp
// helper (the one spot that changes when the wire goes numeric).

import { describe, test, expect } from "bun:test";

import { previewText, renderUpdatedAt } from "../src/commands/memory.ts";

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

  test("whitespace collapses to single spaces before measuring", () => {
    expect(previewText("a\n\n  b\t c")).toBe("a b c");
  });

  test("null, undefined, and empty content render as empty previews", () => {
    expect(previewText(null)).toBe("");
    expect(previewText(undefined)).toBe("");
    expect(previewText("")).toBe("");
  });
});
