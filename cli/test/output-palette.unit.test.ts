// Unit tests for src/output/palette.js — the escape format per capability
// mode, the basic16 fallback codes, and that NO_COLOR / non-TTY paths emit
// byte-exact plain text.

import { describe, test, expect, beforeEach } from "bun:test";
import { palette } from "../src/output/palette.ts";
import { ColorMode, resetCapabilityWarning } from "../src/output/capability.ts";
import { makeBufferStream } from "./helpers.ts";

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

describe("palette — bold and plain text in xterm256", () => {
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

  test("bold returns plain text in 'none' mode", () => {
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
