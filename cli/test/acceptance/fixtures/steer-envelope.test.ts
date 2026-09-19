import { describe, expect, it } from "bun:test";

import { trailingJson, trailingJsonText } from "./steer-envelope.ts";

// `steer --json` writes prose frames first and the envelope last, so every
// fixture here is "some prose, then one object".
const PROSE = "[claw] thinking about it\n[tool] read(file)\n";

describe("trailingJsonText", () => {
  it("takes the trailing object and leaves the prose behind", () => {
    expect(trailingJsonText(`${PROSE}{"ok":true}`)).toBe('{"ok":true}');
  });

  it("keeps a brace the model printed inside its reply", () => {
    // The reason the scanner tracks strings at all: a `}` in prose is ordinary.
    expect(trailingJsonText(`[claw] use {} for an empty set\n{"ok":true}`))
      .toBe('{"ok":true}');
  });

  it("walks past a nested object to the depth-zero brace", () => {
    const envelope = '{"outer":{"inner":{"deep":1}}}';
    expect(trailingJsonText(`${PROSE}${envelope}`)).toBe(envelope);
  });

  // The regression this file was added for. Reading right-to-left, the scanner
  // meets an escaped quote BEFORE the backslash that escapes it, so a forward
  // scanner's carried flag reads the stream inside-out: the escaped quote ended
  // the string early, the real opening `{` looked like string content, depth
  // never returned to zero, and a valid envelope threw "unbalanced JSON".
  it("does not end a string at a LONE escaped quote", () => {
    // One escaped quote, not a pair. A pair happens to cancel out and lets the
    // inside-out reading limp home, so an even-count fixture proves nothing —
    // odd parity is where the old scanner lost the opening brace entirely.
    const envelope = '{"proposed_action":"approve the \\" grant"}';
    expect(trailingJsonText(`${PROSE}${envelope}`)).toBe(envelope);
    expect(trailingJson(`${PROSE}${envelope}`)).toEqual({
      proposed_action: 'approve the " grant',
    });
  });

  it("does not end a string at a PAIR of escaped quotes", () => {
    const envelope = '{"proposed_action":"approve \\"github\\" for the fleet"}';
    expect(trailingJsonText(`${PROSE}${envelope}`)).toBe(envelope);
    expect(trailingJson(`${PROSE}${envelope}`)).toEqual({
      proposed_action: 'approve "github" for the fleet',
    });
  });

  it("treats an escaped backslash as content, not as an escape", () => {
    // Parity is the whole rule: `\\` before the closing quote escapes itself,
    // so the quote still closes the string. An odd/even slip here swallows the
    // rest of the envelope.
    const envelope = '{"path":"C:\\\\repos\\\\","n":1}';
    expect(trailingJsonText(`${PROSE}${envelope}`)).toBe(envelope);
    expect(trailingJson(`${PROSE}${envelope}`)).toEqual({ path: "C:\\repos\\", n: 1 });
  });

  it("survives a brace that only appears inside an escaped-quote string", () => {
    const envelope = '{"headline":"say \\"{\\" out loud"}';
    expect(trailingJson(`${PROSE}${envelope}`)).toEqual({ headline: 'say "{" out loud' });
  });

  it("refuses a stream carrying no object at all", () => {
    expect(() => trailingJsonText("[claw] nothing structured here")).toThrow();
  });

  it("refuses a stream whose braces never balance", () => {
    expect(() => trailingJsonText('[claw] oops "}')).toThrow();
  });
});
