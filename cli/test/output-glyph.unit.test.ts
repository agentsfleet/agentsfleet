// Unit tests for src/output/glyph.js — status glyphs, their colour-free
// rendering, and the glyph + message helper.

import { describe, test, expect } from "bun:test";
import { glyph, withGlyph } from "../src/output/glyph.ts";
import { ColorMode } from "../src/output/capability.ts";

describe("glyph — same character, different signal", () => {
  test("degraded renders differently from live", () => {
    const g = glyph.degraded({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe(glyph.live({ mode: ColorMode.XTERM256 }).char);
    expect(g.render()).not.toBe(glyph.live({ mode: ColorMode.XTERM256 }).render());
  });
});

describe("glyph — NO_COLOR mode emits the bare character", () => {
  test("every glyph renders as just the char in 'none' mode", () => {
    expect(glyph.live({ mode: ColorMode.NONE }).render()).toBe("●");
    expect(glyph.parked({ mode: ColorMode.NONE }).render()).toBe("○");
    expect(glyph.degraded({ mode: ColorMode.NONE }).render()).toBe("●");
    expect(glyph.failed({ mode: ColorMode.NONE }).render()).toBe("✕");
    expect(glyph.ok({ mode: ColorMode.NONE }).render()).toBe("✓");
    expect(glyph.error({ mode: ColorMode.NONE }).render()).toBe("✕");
    expect(glyph.warn({ mode: ColorMode.NONE }).render()).toBe("⚠");
  });
});

describe("withGlyph — common 'glyph + space + message' helper", () => {
  test("pairs a glyph with the message", () => {
    const out = withGlyph(glyph.ok, "logged in", { mode: ColorMode.NONE });
    expect(out).toBe("✓ logged in");
  });

  test("preserves color when not in 'none' mode", () => {
    const out = withGlyph(glyph.ok, "logged in", { mode: ColorMode.XTERM256 });
    expect(out).toContain("38;5;78");
    // The glyph carries its own escape envelope; the message text
    // follows after the reset. Strip ANSI to assert on layout.
    const stripped = out.replace(/\x1b\[[0-9;]*m/g, "");
    expect(stripped).toBe("✓ logged in");
  });
});
