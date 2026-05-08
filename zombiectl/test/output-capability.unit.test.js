// Unit tests for src/output/capability.js — color-mode detection.
//
// Decision order (per docs/DESIGN_SYSTEM.md and the spec): NO_COLOR →
// !isTTY → FORCE_COLOR → TERM/COLORTERM. JSON-mode short-circuit lives
// in the command branching, not in the detector.

import { describe, test, expect, beforeEach } from "bun:test";
import {
  ColorMode,
  detectColorMode,
  isTty,
  noteBasic16IfFirst,
  resetCapabilityWarning,
} from "../src/output/capability.js";
import { makeBufferStream } from "./helpers.js";

function makeTtyStream() {
  const b = makeBufferStream();
  b.stream.isTTY = true;
  return b;
}

function makeNonTtyStream() {
  return makeBufferStream();
}

beforeEach(() => {
  resetCapabilityWarning();
});

describe("detectColorMode — NO_COLOR honoured per no-color.org", () => {
  test("NO_COLOR=1 returns 'none' even in a 256-color TTY", () => {
    const tty = makeTtyStream();
    const env = { NO_COLOR: "1", TERM: "xterm-256color", COLORTERM: "truecolor" };
    expect(detectColorMode(env, tty.stream)).toBe(ColorMode.NONE);
  });

  test("NO_COLOR=true (non-empty) returns 'none'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ NO_COLOR: "true" }, tty.stream)).toBe(ColorMode.NONE);
  });

  test("NO_COLOR='' (empty string) is treated as unset — does not disable", () => {
    const tty = makeTtyStream();
    const env = { NO_COLOR: "", TERM: "xterm-256color" };
    expect(detectColorMode(env, tty.stream)).toBe(ColorMode.XTERM256);
  });
});

describe("detectColorMode — !isTTY returns 'none' regardless of TERM", () => {
  test("non-TTY pipe with TERM=xterm-256color returns 'none'", () => {
    const pipe = makeNonTtyStream();
    expect(detectColorMode({ TERM: "xterm-256color" }, pipe.stream)).toBe(ColorMode.NONE);
  });

  test("non-TTY pipe with COLORTERM=truecolor returns 'none'", () => {
    const pipe = makeNonTtyStream();
    expect(detectColorMode({ COLORTERM: "truecolor" }, pipe.stream)).toBe(ColorMode.NONE);
  });
});

describe("detectColorMode — FORCE_COLOR overrides TTY check", () => {
  test("FORCE_COLOR=2 + non-TTY returns 'xterm256'", () => {
    const pipe = makeNonTtyStream();
    expect(detectColorMode({ FORCE_COLOR: "2" }, pipe.stream)).toBe(ColorMode.XTERM256);
  });

  test("FORCE_COLOR=1 + non-TTY returns 'basic16'", () => {
    const pipe = makeNonTtyStream();
    expect(detectColorMode({ FORCE_COLOR: "1" }, pipe.stream)).toBe(ColorMode.BASIC16);
  });

  test("FORCE_COLOR=0 forces 'none' even in a TTY", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ FORCE_COLOR: "0", TERM: "xterm-256color" }, tty.stream)).toBe(
      ColorMode.NONE,
    );
  });

  test("FORCE_COLOR=3 returns 'xterm256' (we don't track truecolor distinctly)", () => {
    const pipe = makeNonTtyStream();
    expect(detectColorMode({ FORCE_COLOR: "3" }, pipe.stream)).toBe(ColorMode.XTERM256);
  });
});

describe("detectColorMode — TERM-based fallback", () => {
  test("TERM=xterm-256color in TTY returns 'xterm256'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "xterm-256color" }, tty.stream)).toBe(ColorMode.XTERM256);
  });

  test("TERM=screen-256color returns 'xterm256'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "screen-256color" }, tty.stream)).toBe(ColorMode.XTERM256);
  });

  test("TERM=xterm (no 256color) returns 'basic16'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "xterm" }, tty.stream)).toBe(ColorMode.BASIC16);
  });

  test("TERM=dumb returns 'none'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "dumb" }, tty.stream)).toBe(ColorMode.NONE);
  });

  test("TERM='' (empty) returns 'none'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "" }, tty.stream)).toBe(ColorMode.NONE);
  });

  test("COLORTERM=truecolor wins over TERM=xterm", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ TERM: "xterm", COLORTERM: "truecolor" }, tty.stream)).toBe(
      ColorMode.XTERM256,
    );
  });

  test("COLORTERM=24bit returns 'xterm256'", () => {
    const tty = makeTtyStream();
    expect(detectColorMode({ COLORTERM: "24bit" }, tty.stream)).toBe(ColorMode.XTERM256);
  });
});

describe("isTty", () => {
  test("returns true when stream.isTTY is true", () => {
    const tty = makeTtyStream();
    expect(isTty(tty.stream)).toBe(true);
  });

  test("returns false when stream.isTTY is undefined", () => {
    const pipe = makeNonTtyStream();
    expect(isTty(pipe.stream)).toBe(false);
  });

  test("returns false when stream is null", () => {
    expect(isTty(null)).toBe(false);
  });
});

describe("noteBasic16IfFirst — fires once per process", () => {
  test("first call writes one notice line; second call is silent", () => {
    const stderr = makeTtyStream();
    noteBasic16IfFirst(stderr.stream);
    const first = stderr.read();
    expect(first).toContain("note: terminal advertises <256 colors");

    noteBasic16IfFirst(stderr.stream);
    const second = stderr.read();
    expect(second).toBe(first);
  });

  test("does not write to a non-TTY stderr", () => {
    const stderr = makeNonTtyStream();
    noteBasic16IfFirst(stderr.stream);
    expect(stderr.read()).toBe("");
  });
});
