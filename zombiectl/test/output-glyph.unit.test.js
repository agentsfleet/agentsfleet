// Unit tests for src/output/glyph.js — status glyph + color pairing.
// Per docs/DESIGN_SYSTEM.md "Status glyphs":
//   Live      → ● in pulse-cyan (256:79)
//   Parked    → ○ in subtle-grey (256:240)
//   Degraded  → ● in warn-amber (256:214)
//   Failed    → ✕ in error-red (256:210)
//
// Operational glyphs (carve-out — functional, not decorative):
//   ok    → ✓ in success-green (256:78)
//   error → ✕ in error-red (256:210)
//   warn  → ⚠ in warn-amber (256:214)

import { describe, test, expect } from "bun:test";
import { glyph, withGlyph } from "../src/output/glyph.js";
import { ColorMode } from "../src/output/capability.js";

const ESC = "\u001b";

describe("glyph — character + color pairing matches the design system", () => {
  test("live → ● in pulse-cyan", () => {
    const g = glyph.live({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("●");
    expect(g.render()).toBe(`${ESC}[38;5;79m●${ESC}[0m`);
  });

  test("parked → ○ in subtle-grey", () => {
    const g = glyph.parked({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("○");
    expect(g.render()).toBe(`${ESC}[38;5;240m○${ESC}[0m`);
  });

  test("degraded → ● in warn-amber (distinct color from live)", () => {
    const g = glyph.degraded({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("●");
    expect(g.render()).toBe(`${ESC}[38;5;214m●${ESC}[0m`);
    // Distinct color from live — same char, different signal.
    expect(g.render()).not.toBe(glyph.live({ mode: ColorMode.XTERM256 }).render());
  });

  test("failed → ✕ in error-red", () => {
    const g = glyph.failed({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("✕");
    expect(g.render()).toBe(`${ESC}[38;5;210m✕${ESC}[0m`);
  });

  test("ok → ✓ in success-green", () => {
    const g = glyph.ok({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("✓");
    expect(g.render()).toBe(`${ESC}[38;5;78m✓${ESC}[0m`);
  });

  test("error → ✕ in error-red", () => {
    const g = glyph.error({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("✕");
    expect(g.render()).toBe(`${ESC}[38;5;210m✕${ESC}[0m`);
  });

  test("warn → ⚠ in warn-amber", () => {
    const g = glyph.warn({ mode: ColorMode.XTERM256 });
    expect(g.char).toBe("⚠");
    expect(g.render()).toBe(`${ESC}[38;5;214m⚠${ESC}[0m`);
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
