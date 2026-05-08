// Unit tests for src/output/palette.js — the canonical 256-color → token
// mapping. Asserts the exact ANSI escape per token, the basic16 fallback
// codes, and that NO_COLOR / non-TTY paths emit byte-exact plain text.
//
// The 256-color codes per docs/DESIGN_SYSTEM.md "CLI / zombiectl
// rendering" are pinned in PALETTE_INTERNALS — drift breaks the brand.

import { describe, test, expect, beforeEach } from "bun:test";
import { palette, PALETTE_INTERNALS } from "../src/output/palette.js";
import { ColorMode, resetCapabilityWarning } from "../src/output/capability.js";
import { makeBufferStream } from "./helpers.js";

const ESC = "\u001b";

function ttyStream() {
  const b = makeBufferStream();
  b.stream.isTTY = true;
  return b.stream;
}

function pipeStream() {
  return makeBufferStream().stream;
}

beforeEach(() => {
  resetCapabilityWarning();
});

describe("palette — xterm256 mapping pinned to design-system tokens", () => {
  test("pulse → 256:79 (cyan2)", () => {
    expect(palette.pulse("hi", { mode: ColorMode.XTERM256 })).toBe(`${ESC}[38;5;79mhi${ESC}[0m`);
  });

  test("evidence → 256:220 (gold1)", () => {
    expect(palette.evidence("EVIDENCE", { mode: ColorMode.XTERM256 })).toBe(
      `${ESC}[38;5;220mEVIDENCE${ESC}[0m`,
    );
  });

  test("success → 256:78", () => {
    expect(palette.success("ok", { mode: ColorMode.XTERM256 })).toBe(`${ESC}[38;5;78mok${ESC}[0m`);
  });

  test("warn → 256:214", () => {
    expect(palette.warn("warn", { mode: ColorMode.XTERM256 })).toBe(`${ESC}[38;5;214mwarn${ESC}[0m`);
  });

  test("error → 256:210", () => {
    expect(palette.error("err", { mode: ColorMode.XTERM256 })).toBe(`${ESC}[38;5;210merr${ESC}[0m`);
  });

  test("muted → 256:102 (grey53)", () => {
    expect(palette.muted("dim", { mode: ColorMode.XTERM256 })).toBe(`${ESC}[38;5;102mdim${ESC}[0m`);
  });

  test("subtle → 256:240", () => {
    expect(palette.subtle("subtle", { mode: ColorMode.XTERM256 })).toBe(
      `${ESC}[38;5;240msubtle${ESC}[0m`,
    );
  });

  test("pulseBold → bold + 256:79", () => {
    expect(palette.pulseBold("USAGE", { mode: ColorMode.XTERM256 })).toBe(
      `${ESC}[1;38;5;79mUSAGE${ESC}[0m`,
    );
  });

  test("bold → bold only, no color", () => {
    expect(palette.bold("Section", { mode: ColorMode.XTERM256 })).toBe(
      `${ESC}[1mSection${ESC}[0m`,
    );
  });

  test("text → no escape, no transformation", () => {
    expect(palette.text("plain")).toBe("plain");
  });
});

describe("palette — basic16 fallback codes", () => {
  test("pulse falls back to cyan (36)", () => {
    expect(palette.pulse("x", { mode: ColorMode.BASIC16 })).toContain(`${ESC}[36mx`);
  });

  test("evidence falls back to yellow (33)", () => {
    expect(palette.evidence("x", { mode: ColorMode.BASIC16 })).toContain(`${ESC}[33mx`);
  });

  test("success falls back to green (32)", () => {
    expect(palette.success("x", { mode: ColorMode.BASIC16 })).toContain(`${ESC}[32mx`);
  });

  test("error falls back to red (31)", () => {
    expect(palette.error("x", { mode: ColorMode.BASIC16 })).toContain(`${ESC}[31mx`);
  });

  test("pulseBold in basic16 emits bold + cyan", () => {
    expect(palette.pulseBold("HEAD", { mode: ColorMode.BASIC16 })).toContain(`${ESC}[1;36mHEAD`);
  });
});

describe("palette — NO_COLOR / 'none' mode emits byte-exact plain text", () => {
  test("every color helper returns the input unchanged in 'none' mode", () => {
    for (const fn of [
      palette.pulse,
      palette.evidence,
      palette.success,
      palette.warn,
      palette.error,
      palette.muted,
      palette.subtle,
    ]) {
      expect(fn("hi", { mode: ColorMode.NONE })).toBe("hi");
    }
  });

  test("pulseBold and bold return plain text in 'none' mode", () => {
    expect(palette.pulseBold("HI", { mode: ColorMode.NONE })).toBe("HI");
    expect(palette.bold("HI", { mode: ColorMode.NONE })).toBe("HI");
  });

  test("NO_COLOR=1 env in a TTY produces zero ANSI sequences", () => {
    const env = { NO_COLOR: "1", TERM: "xterm-256color" };
    const result = palette.pulse("live", { env, stream: ttyStream() });
    expect(result).toBe("live");
    expect(result).not.toMatch(/\[/);
  });
});

describe("palette — capability defaults to caller's stream", () => {
  test("non-TTY stream → 'none' mode → plain text", () => {
    const result = palette.pulse("hi", { env: { TERM: "xterm-256color" }, stream: pipeStream() });
    expect(result).toBe("hi");
  });

  test("TTY stream + 256color TERM → xterm256 → escape present", () => {
    const result = palette.pulse("hi", { env: { TERM: "xterm-256color" }, stream: ttyStream() });
    expect(result).toBe(`${ESC}[38;5;79mhi${ESC}[0m`);
  });
});

describe("PALETTE_INTERNALS — design-system invariants pinned", () => {
  test("xterm256 codes match the design-system mapping", () => {
    expect(PALETTE_INTERNALS.XTERM_256).toMatchObject({
      pulse: 79,
      evidence: 220,
      success: 78,
      warn: 214,
      error: 210,
      muted: 102,
      subtle: 240,
    });
  });

  test("basic16 codes are valid SGR ints", () => {
    for (const code of Object.values(PALETTE_INTERNALS.BASIC_16)) {
      expect(code).toMatch(/^[0-9]{1,2}$/);
    }
  });
});
