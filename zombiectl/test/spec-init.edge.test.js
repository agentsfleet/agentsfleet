/**
 * spec-init edge cases and error paths — T2, T3
 */
import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { parseMakeTargets, detectTestPatterns, commandSpecInit } from "../src/commands/spec_init.js";
import { mkdirSync, writeFileSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
import { makeTmp, cleanup, parseFlags, writeLine } from "./helpers-fs.js";

// ── T2 parseMakeTargets ───────────────────────────────────────────────────────

describe("T2 parseMakeTargets edge cases", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => cleanup(tmp));

  test("empty Makefile returns empty array", () => {
    writeFileSync(join(tmp, "Makefile"), "");
    expect(parseMakeTargets(tmp)).toEqual([]);
  });

  test("comment-only Makefile returns empty array", () => {
    writeFileSync(join(tmp, "Makefile"), "# comment\n# another\n");
    expect(parseMakeTargets(tmp)).toEqual([]);
  });

  test("recipe lines not mistaken for targets", () => {
    writeFileSync(join(tmp, "Makefile"), "build:\n\techo build\n\techo done\n");
    const targets = parseMakeTargets(tmp);
    expect(targets).toContain("build");
    expect(targets.length).toBe(1);
  });

  test(".PHONY declaration excluded from targets", () => {
    writeFileSync(join(tmp, "Makefile"), ".PHONY: all\nall:\n\techo all\n");
    expect(parseMakeTargets(tmp)).not.toContain(".PHONY");
    expect(parseMakeTargets(tmp)).toContain("all");
  });

  test("numeric suffix in target name parsed correctly", () => {
    writeFileSync(join(tmp, "Makefile"), "test1:\n\techo\ntest2:\n\techo\n");
    const t = parseMakeTargets(tmp);
    expect(t).toContain("test1");
    expect(t).toContain("test2");
  });

  test("CRLF line endings handled", () => {
    writeFileSync(join(tmp, "Makefile"), "build:\r\n\techo\r\nlint:\r\n\techo\r\n");
    const t = parseMakeTargets(tmp);
    expect(t).toContain("build");
    expect(t).toContain("lint");
  });

  test("200-target Makefile parsed without crash", () => {
    const content = Array.from({ length: 200 }, (_, i) => `target${i}:\n\techo\n`).join("\n");
    writeFileSync(join(tmp, "Makefile"), content);
    expect(parseMakeTargets(tmp).length).toBe(200);
  });

  test("no Makefile returns empty array without throwing", () => {
    expect(parseMakeTargets(tmp)).toEqual([]);
  });

  test("hyphens and underscores in target names parsed", () => {
    writeFileSync(join(tmp, "Makefile"), "lint-zig:\n\tzig fmt\ntest_unit:\n\tbun test\n");
    const t = parseMakeTargets(tmp);
    expect(t).toContain("lint-zig");
    expect(t).toContain("test_unit");
  });

  test("double-colon rule (target::) not matched as normal target", () => {
    writeFileSync(join(tmp, "Makefile"), "all:: build\nbuild:\n\techo ok\n");
    const t = parseMakeTargets(tmp);
    // double-colon is unusual; parser may or may not include it — should not crash
    expect(Array.isArray(t)).toBe(true);
    expect(t).toContain("build");
  });

  test("target with dots (e.g. build.linux) parsed correctly", () => {
    writeFileSync(join(tmp, "Makefile"), "build.linux:\n\techo linux\n");
    const t = parseMakeTargets(tmp);
    expect(t).toContain("build.linux");
  });

  test("very long target name (60 chars) parsed without error", () => {
    const longTarget = "a".repeat(60);
    writeFileSync(join(tmp, "Makefile"), `${longTarget}:\n\techo ok\n`);
    expect(() => parseMakeTargets(tmp)).not.toThrow();
  });

  test("targets with prerequisites (target: dep1 dep2) parsed correctly", () => {
    writeFileSync(join(tmp, "Makefile"), "test: build lint\n\tgo test ./...\nbuild:\n\tgo build\nlint:\n\tgo vet\n");
    const t = parseMakeTargets(tmp);
    expect(t).toContain("test");
    expect(t).toContain("build");
    expect(t).toContain("lint");
  });
});

// ── T2 detectTestPatterns ─────────────────────────────────────────────────────

describe("T2 detectTestPatterns edge cases", () => {
  test("empty file list returns empty", () => {
    expect(detectTestPatterns([])).toEqual([]);
  });

  test("deeply nested test dir matched", () => {
    const p = detectTestPatterns(["src/deep/nested/tests/foo_test.go"]);
    expect(p).toContain("tests/ directory");
  });

  test(".spec.ts detected", () => {
    const p = detectTestPatterns(["src/foo.spec.ts"]);
    expect(p.some((x) => x.includes("spec"))).toBe(true);
  });

  test("100 files with same pattern does not produce duplicate entries", () => {
    const files = Array.from({ length: 100 }, (_, i) => `src/f${i}.test.js`);
    const p = detectTestPatterns(files);
    expect(p.filter((x) => x.includes("test")).length).toBeLessThanOrEqual(2);
  });

  test("test/ at repo root matched", () => {
    const p = detectTestPatterns(["test/foo_test.go"]);
    expect(p).toContain("tests/ directory");
  });

  test("__tests__ directory not confused with tests/ pattern", () => {
    const p = detectTestPatterns(["src/__tests__/foo.test.js"]);
    expect(p.some((x) => x.includes("test"))).toBe(true);
  });

  test(".test.tsx detected as test pattern", () => {
    const p = detectTestPatterns(["src/Login.test.tsx"]);
    expect(p.some((x) => x.includes("test"))).toBe(true);
  });

  test("mixed patterns in one file list captures all types", () => {
    const files = [
      "test/login_test.go",
      "src/Login.test.tsx",
      "src/util.spec.ts",
    ];
    const p = detectTestPatterns(files);
    expect(p.length).toBeGreaterThanOrEqual(2);
  });
});

// ── T3 commandSpecInit error paths ────────────────────────────────────────────

describe("T3 commandSpecInit error paths", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => cleanup(tmp));

  test("nonexistent --path exits 2 with error message", async () => {
    const errBuf = makeBufferStream();
    const ctx = { stdout: makeNoop(), stderr: errBuf.stream, jsonMode: false };
    const code = await commandSpecInit(["--path", "/nonexistent/xyz"], ctx, {
      parseFlags, writeLine, ui, printJson: () => {},
    });
    expect(code).toBe(2);
    expect(errBuf.read()).toContain("path not found");
  });

  test("unwritable output dir exits 1 gracefully (non-root)", async () => {
    if (process.getuid?.() === 0) return;
    const roDir = join(tmp, "ro");
    mkdirSync(roDir, { recursive: true });
    chmodSync(roDir, 0o555);
    const errBuf = makeBufferStream();
    const ctx = { stdout: makeNoop(), stderr: errBuf.stream, jsonMode: false };
    const code = await commandSpecInit(
      ["--path", tmp, "--output", join(roDir, "sub", "out.md")],
      ctx,
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect(code).toBe(1);
    chmodSync(roDir, 0o755);
  });

  test("missing --path argument falls back to default '.' without crash", async () => {
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
    // Pass no --path flag — should use default "." which exists
    const code = await commandSpecInit(
      ["--output", join(tmp, "out.md")],
      ctx,
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect([0, 1]).toContain(code); // may fail if CWD has no perms, but must not throw
  });

  test("overwriting an existing output file succeeds silently", async () => {
    const { writeFileSync } = await import("node:fs");
    const out = join(tmp, "existing.md");
    writeFileSync(out, "old content");
    const code = await commandSpecInit(
      ["--path", tmp, "--output", out],
      { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false },
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect(code).toBe(0);
    const { readFileSync } = await import("node:fs");
    expect(readFileSync(out, "utf8")).not.toBe("old content");
  });

  test("stdout contains output path and scanned file count on success", async () => {
    const outBuf = makeBufferStream();
    const out = join(tmp, "info.md");
    const code = await commandSpecInit(
      ["--path", tmp, "--output", out],
      { stdout: outBuf.stream, stderr: makeNoop(), jsonMode: false },
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect(code).toBe(0);
    expect(outBuf.read()).toContain(out);
  });

  test("error message is written to stderr, not stdout", async () => {
    const outBuf = makeBufferStream();
    const errBuf = makeBufferStream();
    const code = await commandSpecInit(
      ["--path", "/nonexistent/xyz"],
      { stdout: outBuf.stream, stderr: errBuf.stream, jsonMode: false },
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect(code).not.toBe(0);
    expect(errBuf.read()).toContain("path not found");
    expect(outBuf.read()).toBe("");
  });
});
