import {expect, it} from "vitest";
import {ansiColor, parseAnsi} from "./terminal-ansi";

const ESC = "\u001b";
const text = (value: string) => parseAnsi(value).map(segment => segment.text).join("");

it("preserves readable output while removing cursor, hyperlink and terminal control instructions", () => {
  expect(text(`start${ESC}[2Kend`)).toBe("startend");
  expect(text(`a\u009b2Kb`)).toBe("ab");
  expect(text(`${ESC}]8;;https://example.test\u0007label${ESC}]8;;${ESC}\\`)).toBe("label");
  expect(text(`\u009dignored${ESC}\\visible`)).toBe("visible");
  for (const kind of ["P", "^", "_"]) expect(text(`a${ESC}${kind}private${ESC}\\b`)).toBe("ab");
  expect(text(`a${ESC}(Bb${ESC}7c`)).toBe("abc");
  expect(text(`a${ESC}[unfinished`)).toBe("anfinished");
  expect(text(`a${ESC}[12`)).toBe("a");
  expect(text(`a${ESC}]unterminated`)).toBe("a");
  expect(text(`a${ESC}Punterminated`)).toBe("a");
  expect(text(`a${ESC}`)).toBe("a");
  expect(text("")).toBe("");
});

it("tracks supported emphasis and resets without leaking unsupported extended color instructions", () => {
  const segments = parseAnsi(`${ESC}[1;2;31mstrong${ESC}[22;39mnormal${ESC}[mreset`);
  expect(segments[0]).toMatchObject({text: "strong", color: 31, bold: true, dim: true});
  expect(segments[1]).toMatchObject({text: "normalreset", color: undefined, bold: false, dim: false});
  for (const code of ["38;2;1;2;3", "48;5;10", "38;7", "48", "999", ";"]) {
    expect(text(`${ESC}[${code}mvisible`)).toBe("visible");
  }
  expect(text(`a${ESC}[0mb${ESC}[0mc`)).toBe("abc");
  expect(ansiColor(31, false)).toContain("text-red");
  expect(ansiColor(31, true)).toContain("text-red");
});
