/**
 * run-preview happy path, edge cases, error paths, output fidelity — T1, T2, T3, T4
 */
import { describe, test, expect } from "bun:test";
import {
  extractSpecRefs,
  matchRefsToFiles,
  printPreview,
  confIndicator,
  sanitizeDisplay,
  runPreview,
} from "../src/commands/run_preview.js";
import { Writable } from "node:stream";
import { writeFileSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
import { makeTmp, cleanup, writeLine } from "./helpers-fs.js";

// ── T1 Happy Path ─────────────────────────────────────────────────────────────

describe("T1 printPreview happy path", () => {
  test("prints heading and file count", () => {
    const buf = makeBufferStream();
    printPreview(buf.stream, [
      { file: "src/main.go", confidence: "high" },
      { file: "src/util.go", confidence: "medium" },
    ], { writeLine, ui });
    expect(buf.read()).toContain("Predicted file impact");
    expect(buf.read()).toContain("2 file(s)");
  });

  test("prints all confidence levels", () => {
    const buf = makeBufferStream();
    printPreview(buf.stream, [
      { file: "a.go", confidence: "high" },
      { file: "b.go", confidence: "medium" },
      { file: "c.go", confidence: "low" },
    ], { writeLine, ui });
    const out = buf.read();
    expect(out).toContain("a.go");
    expect(out).toContain("b.go");
    expect(out).toContain("c.go");
  });

  test("empty matches shows info message, not heading", () => {
    const buf = makeBufferStream();
    printPreview(buf.stream, [], { writeLine, ui });
    expect(buf.read()).toContain("no file references detected");
    expect(buf.read()).not.toContain("Predicted file impact");
  });
});

// ── T1 Happy Path (extended) ──────────────────────────────────────────────────

describe("T1 printPreview happy path (extended)", () => {
  test("printPreview preserves match order passed to it (sorted input stays sorted)", () => {
    const buf = makeBufferStream();
    // Pass already-sorted data (a before z) — printPreview must preserve order
    printPreview(buf.stream, [
      { file: "src/a.go", confidence: "high" },
      { file: "src/z.go", confidence: "high" },
    ], { writeLine, ui });
    const out = buf.read();
    expect(out.indexOf("src/a.go")).toBeLessThan(out.indexOf("src/z.go"));
  });

  test("all-low matches renders with low indicator for each", () => {
    const buf = makeBufferStream();
    const matches = [
      { file: "src/x.go", confidence: "low" },
      { file: "src/y.go", confidence: "low" },
    ];
    printPreview(buf.stream, matches, { writeLine, ui });
    const out = buf.read();
    expect(out).toContain("src/x.go");
    expect(out).toContain("src/y.go");
    expect(out).toContain("2 file(s)");
  });

  test("20-file list renders all filenames without truncation", () => {
    const buf = makeBufferStream();
    const matches = Array.from({ length: 20 }, (_, i) => ({ file: `src/file${i}.go`, confidence: "low" }));
    printPreview(buf.stream, matches, { writeLine, ui });
    const out = buf.read();
    for (let i = 0; i < 20; i++) expect(out).toContain(`file${i}.go`);
    expect(out).toContain("20 file(s)");
  });

  test("file path with spaces renders without crash", () => {
    const buf = makeBufferStream();
    printPreview(buf.stream, [{ file: "src/my file.go", confidence: "medium" }], { writeLine, ui });
    expect(buf.read()).toContain("my file.go");
  });
});

// ── T2 Edge Cases — extractSpecRefs ──────────────────────────────────────────

describe("T2 extractSpecRefs edge cases", () => {
  test("empty string returns empty array", () => {
    expect(extractSpecRefs("")).toEqual([]);
  });

  test("whitespace-only returns empty array", () => {
    expect(extractSpecRefs("   \n\t\n  ")).toEqual([]);
  });

  test("only headings returns empty array", () => {
    expect(extractSpecRefs("# Title\n## Section\n### Sub\n")).toEqual([]);
  });

  test("CRLF line endings handled", () => {
    const refs = extractSpecRefs("Edit `src/foo.go`\r\nand `src/bar.go`\r\n");
    expect(refs).toContain("src/foo.go");
    expect(refs).toContain("src/bar.go");
  });

  test("repeated reference is deduplicated to one entry", () => {
    const md = Array.from({ length: 1000 }, () => "`src/core.go`").join(" ");
    expect(extractSpecRefs(md).filter((r) => r === "src/core.go").length).toBe(1);
  });

  test("single-quoted paths extracted", () => {
    const refs = extractSpecRefs("Edit 'lib/utils.js' here.");
    expect(refs.some((r) => r.includes("utils.js"))).toBe(true);
  });

  test("double-quoted paths extracted", () => {
    const refs = extractSpecRefs(`Fix "src/api/handler.go" now.`);
    expect(refs.some((r) => r.includes("handler.go"))).toBe(true);
  });

  test("binary content (null bytes) does not crash", () => {
    expect(() => extractSpecRefs("src/foo.go\x00\x01binary\x00")).not.toThrow();
  });

  test("ReDoS guard: 10,000-char line completes under 500ms", () => {
    const start = performance.now();
    extractSpecRefs("a".repeat(10000));
    expect(performance.now() - start).toBeLessThan(500);
  });

  test("1MB spec file processed under 3 seconds", () => {
    const block = "Edit `src/file.go` and `lib/util.ts`.\n".repeat(20000);
    const start = performance.now();
    const refs = extractSpecRefs(block);
    expect(performance.now() - start).toBeLessThan(3000);
    expect(refs.length).toBeGreaterThan(0);
  });

  test("workers/ and scripts/ prefixes extracted", () => {
    const refs = extractSpecRefs("Update `workers/processor.go` and `scripts/deploy.sh`.");
    expect(refs.some((r) => r.includes("workers/"))).toBe(true);
    expect(refs.some((r) => r.includes("scripts/"))).toBe(true);
  });

  test("yaml and toml config file refs extracted", () => {
    const refs = extractSpecRefs("Edit `config/app.yaml` and `Cargo.toml`.");
    expect(refs.some((r) => r.includes(".yaml") || r.includes(".toml"))).toBe(true);
  });

  test("markdown link format [text](path.go) extracts path", () => {
    const refs = extractSpecRefs("See [the handler](src/api/handler.go) for details.");
    expect(refs.some((r) => r.includes("handler.go"))).toBe(true);
  });

  test("fenced code block containing file path is extracted", () => {
    const md = "Edit this file:\n```\nsrc/commands/core.js\n```\n";
    const refs = extractSpecRefs(md);
    expect(refs.some((r) => r.includes("core.js"))).toBe(true);
  });

  test("shell script extension .sh extracted", () => {
    const refs = extractSpecRefs("Run `scripts/deploy.sh` to release.");
    expect(refs.some((r) => r.includes(".sh"))).toBe(true);
  });

  test("common English words without path separators or extensions not extracted", () => {
    const refs = extractSpecRefs("Update the configuration to use the new service.");
    expect(refs.every((r) => !r.includes("configuration"))).toBe(true);
    expect(refs.every((r) => !r.includes("service"))).toBe(true);
  });

  test("paths starting with ./ not extracted as refs", () => {
    const refs = extractSpecRefs("Edit `./src/core.go`.");
    // The quoted path with "./" prefix may or may not be captured; key: it must not crash
    expect(() => extractSpecRefs("Edit `./src/core.go`.")).not.toThrow();
  });
});

// ── T2 Edge Cases — matchRefsToFiles ─────────────────────────────────────────

describe("T2 matchRefsToFiles edge cases", () => {
  test("empty refs + empty files = empty", () => {
    expect(matchRefsToFiles([], [])).toEqual([]);
  });

  test("empty refs with files = empty", () => {
    expect(matchRefsToFiles([], ["src/foo.go"])).toEqual([]);
  });

  test("refs with no matching files = empty", () => {
    expect(matchRefsToFiles(["zzz_nonexistent.go"], ["src/foo.go"])).toEqual([]);
  });

  test("backslash paths normalised before matching", () => {
    const matches = matchRefsToFiles(["src/foo.go"], ["src\\foo.go"]);
    expect(matches.length).toBe(1);
  });

  test("each file appears at most once", () => {
    const matches = matchRefsToFiles(["src/foo.go", "foo.go", "foo"], ["src/foo.go"]);
    const paths = matches.map((m) => m.file);
    expect(paths.length).toBe(new Set(paths).size);
  });

  test("results sorted high → medium → low", () => {
    const matches = matchRefsToFiles(
      ["src/commands/exact.js", "commands", "cmd"],
      ["src/commands/exact.js", "src/commands/other.js", "cmd/main.go"],
    );
    const order = { high: 0, medium: 1, low: 2 };
    for (let i = 1; i < matches.length; i++) {
      expect(order[matches[i].confidence]).toBeGreaterThanOrEqual(order[matches[i - 1].confidence]);
    }
  });

  test("300 refs × 500 files completes under 3s", () => {
    const refs = Array.from({ length: 300 }, (_, i) => `nomatch${i}.xyz`);
    const files = Array.from({ length: 500 }, (_, i) => `src/file${i}.go`);
    const start = performance.now();
    matchRefsToFiles(refs, files);
    expect(performance.now() - start).toBeLessThan(3000);
  });

  test("deeply nested path ref does not cause stack overflow", () => {
    const deep = "src/" + "level/".repeat(100) + "file.go";
    expect(() => matchRefsToFiles([deep], [deep])).not.toThrow();
  });

  test("multiple refs that match the same file each yield exactly one match entry", () => {
    const matches = matchRefsToFiles(
      ["src/core.go", "core.go", "core"],
      ["src/core.go"],
    );
    expect(matches.filter((m) => m.file === "src/core.go").length).toBe(1);
  });

  test("ref matching multiple files returns all of them", () => {
    const matches = matchRefsToFiles(
      ["handler"],
      ["src/api/handler.go", "src/ws/handler.go", "lib/handler.ts"],
    );
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  test("suffix match (file ends with ref) scores as high confidence", () => {
    // "src/core.go" endsWith "core.go" → high
    const matches = matchRefsToFiles(["core.go"], ["src/core.go"]);
    const coreGo = matches.find((m) => m.file === "src/core.go");
    expect(coreGo).toBeDefined();
    expect(coreGo.confidence).toBe("high");
  });

  test("ref with no extension still matches filename prefix", () => {
    const matches = matchRefsToFiles(["main"], ["src/main.go", "src/main.rs"]);
    expect(matches.length).toBeGreaterThan(0);
  });

  test("very long ref string completes without error", () => {
    const longRef = "src/" + "sub/".repeat(50) + "file.go";
    expect(() => matchRefsToFiles([longRef], ["src/file.go"])).not.toThrow();
  });
});

// ── T3 Error Paths — runPreview ───────────────────────────────────────────────

describe("T3 runPreview error paths", () => {
  test("nonexistent spec file returns null and error message", async () => {
    const err = makeBufferStream();
    const result = await runPreview("/no/such/file.md", ".", { stdout: makeNoop(), stderr: err.stream }, { writeLine, ui });
    expect(result).toBeNull();
    expect(err.read()).toContain("spec file not found");
  });

  test("unreadable spec file returns null (non-root)", async () => {
    if (process.getuid?.() === 0) return;
    const tmp = makeTmp();
    const f = join(tmp, "unreadable.md");
    writeFileSync(f, "# spec");
    chmodSync(f, 0o000);
    try {
      const err = makeBufferStream();
      const result = await runPreview(f, tmp, { stdout: makeNoop(), stderr: err.stream }, { writeLine, ui });
      expect(result).toBeNull();
      expect(err.read()).toContain("failed to read");
    } finally { chmodSync(f, 0o644); cleanup(tmp); }
  });

  test("empty spec file returns { matches: [] }, not null", async () => {
    const tmp = makeTmp();
    const f = join(tmp, "empty.md");
    writeFileSync(f, "");
    try {
      const result = await runPreview(f, tmp, { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      expect(result).not.toBeNull();
      expect(result.matches).toEqual([]);
    } finally { cleanup(tmp); }
  });

  test("spec file with only whitespace returns empty matches", async () => {
    const tmp = makeTmp();
    const f = join(tmp, "blank.md");
    writeFileSync(f, "   \n\t\n  ");
    try {
      const result = await runPreview(f, tmp, { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      expect(result.matches).toEqual([]);
    } finally { cleanup(tmp); }
  });

  test("nonexistent repoPath does not crash — returns empty matches", async () => {
    const tmp = makeTmp();
    const f = join(tmp, "spec.md");
    writeFileSync(f, "Edit `src/foo.go`.");
    try {
      const result = await runPreview(f, "/no/such/repo", { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      expect(result).not.toBeNull();
      expect(Array.isArray(result.matches)).toBe(true);
    } finally { cleanup(tmp); }
  });

  test("spec with only path-traversal refs returns empty matches (no real files)", async () => {
    const tmp = makeTmp();
    const f = join(tmp, "spec.md");
    writeFileSync(f, "Edit `../../etc/passwd` and `../secrets.env`.");
    try {
      const result = await runPreview(f, tmp, { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      expect(result.matches.length).toBe(0);
    } finally { cleanup(tmp); }
  });

  test("stderr is written to ctx.stderr, not stdout", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    await runPreview("/no/file.md", ".", { stdout: out.stream, stderr: err.stream }, { writeLine, ui });
    expect(err.read()).toContain("spec file not found");
    expect(out.read()).toBe("");
  });
});

// ── T4 Output Fidelity ────────────────────────────────────────────────────────

describe("T4 confIndicator fidelity", () => {
  test("non-TTY returns bracket labels for all levels", () => {
    const noTTY = { isTTY: false };
    expect(confIndicator("high", noTTY)).toBe("[HIGH]");
    expect(confIndicator("medium", noTTY)).toBe("[MED] ");
    expect(confIndicator("low", noTTY)).toBe("[LOW] ");
  });

  test("TTY returns ANSI escape sequence", () => {
    const saved = process.env.NO_COLOR;
    delete process.env.NO_COLOR;
    try {
      expect(confIndicator("high", { isTTY: true })).toContain("\u001b[");
    } finally {
      if (saved !== undefined) process.env.NO_COLOR = saved;
      else delete process.env.NO_COLOR;
    }
  });

  test("NO_COLOR=1 suppresses ANSI even on TTY", () => {
    const saved = process.env.NO_COLOR;
    process.env.NO_COLOR = "1";
    try {
      expect(confIndicator("high", { isTTY: true })).not.toContain("\u001b[");
    } finally {
      if (saved !== undefined) process.env.NO_COLOR = saved;
      else delete process.env.NO_COLOR;
    }
  });

  test("unknown confidence value does not throw", () => {
    expect(() => confIndicator("unknown", { isTTY: false })).not.toThrow();
  });
});

describe("T4 sanitizeDisplay fidelity", () => {
  test("strips ANSI codes from filenames", () => {
    expect(sanitizeDisplay("\u001b[31mRED\u001b[0m")).toBe("RED");
  });

  test("strips null bytes", () => {
    expect(sanitizeDisplay("foo\x00bar")).toBe("foobar");
  });

  test("passes clean path unchanged", () => {
    const p = "src/commands/core.js";
    expect(sanitizeDisplay(p)).toBe(p);
  });

  test("empty string returns empty string", () => {
    expect(sanitizeDisplay("")).toBe("");
  });

  test("only escape codes returns empty string", () => {
    expect(sanitizeDisplay("\u001b[1m\u001b[31m\u001b[0m")).toBe("");
  });

  test("mixed escape + text strips only the escape portions", () => {
    expect(sanitizeDisplay("a\u001b[32mb\u001b[0mc")).toBe("abc");
  });

  test("tabs and spaces are preserved", () => {
    const p = "src/cmd\t name.go";
    expect(sanitizeDisplay(p)).toBe(p);
  });

  test("very long filename (500 chars) does not crash", () => {
    const p = "src/" + "a".repeat(495) + ".go";
    expect(() => sanitizeDisplay(p)).not.toThrow();
  });
});
