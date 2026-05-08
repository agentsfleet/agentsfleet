// Unit tests for src/output/format.js — table, key-value, section,
// help-heading, evidence-line rendering. Width-aware, alignment-aware.
// Pulse-cyan currency: only formatHelpHeading consumes pulse here.

import { describe, test, expect } from "bun:test";
import {
  formatTable,
  formatKeyValue,
  formatSection,
  formatHelpHeading,
  formatEvidence,
} from "../src/output/format.js";
import { ColorMode } from "../src/output/capability.js";

const ESC = "\u001b";

const PLAIN = { mode: ColorMode.NONE };
const STYLED = { mode: ColorMode.XTERM256 };

function stripAnsi(str) {
  return str.replace(/\x1b\[[0-9;]*m/g, "");
}

describe("formatHelpHeading — the only pulse-currency callsite in format.js", () => {
  test("emits pulse-cyan + bold in xterm256", () => {
    expect(formatHelpHeading("USAGE", STYLED)).toBe(`${ESC}[1;38;5;79mUSAGE${ESC}[0m`);
  });

  test("emits plain text in 'none' mode", () => {
    expect(formatHelpHeading("USAGE", PLAIN)).toBe("USAGE");
  });
});

describe("formatSection — bold default, NEVER pulse", () => {
  test("section title is bold default text — chrome, not currency", () => {
    const out = formatSection("Workspace", STYLED);
    expect(out).toContain(`${ESC}[1mWorkspace${ESC}[0m`);
    // Pulse-currency invariant: section titles do not use pulse-cyan.
    expect(out).not.toContain("38;5;79");
  });

  test("rule below the title uses subtle-grey (256:240)", () => {
    const out = formatSection("Hi", STYLED);
    expect(out).toContain("38;5;240");
  });

  test("plain mode: title + rule on consecutive lines, no escapes", () => {
    expect(formatSection("Hi", PLAIN)).toBe("\nHi\n──\n");
  });
});

describe("formatEvidence — three-token split", () => {
  test("EVIDENCE label in evidence-amber, ref in default text, quote in muted", () => {
    const out = formatEvidence("cd_logs:281–294", "ENOSPC", STYLED);
    // Evidence label
    expect(out).toContain(`${ESC}[38;5;220mEVIDENCE`);
    // Source ref preserved verbatim
    expect(stripAnsi(out)).toContain("cd_logs:281–294");
    // Quote in muted (256:102)
    expect(out).toContain("38;5;102");
    expect(stripAnsi(out)).toContain('— "ENOSPC"');
  });

  test("plain mode renders the canonical string", () => {
    expect(formatEvidence("cd_logs:1", "boom", PLAIN)).toBe('EVIDENCE cd_logs:1 — "boom"');
  });
});

describe("formatKeyValue", () => {
  test("aligns labels to the longest key", () => {
    const out = formatKeyValue([["short", "1"], ["longer-key", "2"]], PLAIN);
    expect(out).toContain("short     ");  // padded to 10 chars
    expect(out).toContain("longer-key");
  });

  test("empty rows return empty string", () => {
    expect(formatKeyValue([], PLAIN)).toBe("");
  });
});

describe("formatTable — wide terminal renders horizontal layout", () => {
  const COLUMNS = [
    { key: "name", label: "NAME" },
    { key: "status", label: "STATUS" },
  ];
  const ROWS = [
    { name: "platform-ops", status: "LIVE" },
    { name: "db-backup", status: "PARKED" },
  ];

  test("wide terminal (widthHint=120) renders header + rule + rows", () => {
    const out = formatTable(COLUMNS, ROWS, { ...PLAIN, widthHint: 120 });
    const lines = out.split("\n").filter((l) => l !== "");
    expect(lines[0]).toContain("NAME");
    expect(lines[0]).toContain("STATUS");
    expect(lines[1]).toMatch(/^─+/); // rule
    expect(lines[2]).toContain("platform-ops");
    expect(lines[3]).toContain("db-backup");
  });

  test("table header is bold, NOT pulse-cyan (chrome rule)", () => {
    const out = formatTable(COLUMNS, ROWS, { ...STYLED, widthHint: 120 });
    expect(out).toContain(`${ESC}[1m`);   // bold open
    expect(out).not.toContain("38;5;79"); // no pulse-cyan in headers
  });

  test("empty rows → '(none)' in subtle-grey", () => {
    const out = formatTable(COLUMNS, [], PLAIN);
    expect(out).toBe("(none)\n");
  });
});

describe("formatTable — narrow terminal collapses to vertical layout", () => {
  const COLUMNS = [
    { key: "name", label: "NAME" },
    { key: "status", label: "STATUS" },
    { key: "events", label: "EVENTS" },
  ];
  const ROWS = [
    { name: "platform-ops", status: "LIVE", events: "142" },
  ];

  test("widthHint=40 switches to vertical key:value layout", () => {
    const out = formatTable(COLUMNS, ROWS, { ...PLAIN, widthHint: 40 });
    // Each column rendered as a "  LABEL    value" line; no horizontal rule.
    expect(out).toContain("NAME  ");
    expect(out).toContain("platform-ops");
    expect(out).not.toMatch(/^─/m);
  });
});

describe("formatTable — alignment", () => {
  test("numeric columns right-align", () => {
    const COLUMNS = [{ key: "count", label: "COUNT" }];
    const ROWS = [{ count: "1" }, { count: "100" }];
    const out = formatTable(COLUMNS, ROWS, { ...PLAIN, widthHint: 100 });
    // The "1" row should have leading whitespace to right-align under the
    // wider "100".
    const lines = out.split("\n").filter((l) => l !== "");
    const oneRow = lines.find((l) => l.includes("1") && !l.includes("100"));
    expect(oneRow).toMatch(/^ +1$/);
  });

  test("text columns left-align by default", () => {
    const COLUMNS = [{ key: "name", label: "NAME" }];
    const ROWS = [{ name: "alpha" }, { name: "longer-name" }];
    const out = formatTable(COLUMNS, ROWS, { ...PLAIN, widthHint: 100 });
    const lines = out.split("\n").filter((l) => l !== "");
    const alphaRow = lines.find((l) => l.startsWith("alpha"));
    expect(alphaRow).toMatch(/^alpha +$/);
  });

  test("explicit align='right' override", () => {
    const COLUMNS = [{ key: "v", label: "V", align: "right" }];
    const ROWS = [{ v: "abc" }, { v: "longer" }];
    const out = formatTable(COLUMNS, ROWS, { ...PLAIN, widthHint: 100 });
    const lines = out.split("\n").filter((l) => l !== "");
    const abcRow = lines.find((l) => l.includes("abc") && !l.includes("longer"));
    expect(abcRow).toMatch(/^ +abc$/);
  });
});
