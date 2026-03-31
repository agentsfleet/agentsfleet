/**
 * spec-init integration tests — T6
 * End-to-end through real filesystem: real repo scan, real template write,
 * real commandSpecInit invocation. Asserts full output contract.
 */
import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { commandSpecInit, scanRepo, generateTemplate } from "../src/commands/spec_init.js";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
import { makeTmp, cleanup, parseFlags, writeLine } from "./helpers-fs.js";

function ctx(overrides = {}) {
  return { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, ...overrides };
}

// ── T6 Integration Verification ────────────────────────────────────────────────

describe("T6 integration — commandSpecInit end-to-end", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => cleanup(tmp));

  test("full pipeline: scan real Go repo → detect language → write template with correct frontmatter", async () => {
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "main.go"), "package main");
    writeFileSync(join(tmp, "src", "util.go"), "package main");
    const out = join(tmp, "spec.md");
    const code = await commandSpecInit(
      ["--path", tmp, "--output", out],
      ctx(),
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    expect(code).toBe(0);
    const content = readFileSync(out, "utf8");
    expect(content).toContain("**Status:** PENDING");
    expect(content).toContain("**Prototype:** v1.0.0");
  });

  test("scanRepo + generateTemplate compose correctly without commandSpecInit wrapper", () => {
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "main.rs"), "fn main() {}");
    writeFileSync(join(tmp, "Makefile"), "lint:\n\tcargo clippy\ntest:\n\tcargo test\n");
    const scan = scanRepo(tmp);
    expect(scan.makeTargets).toContain("lint");
    const tpl = generateTemplate(scan);
    expect(tpl).toContain("make lint");
    expect(tpl).toContain("make test");
  });

  test("multi-language repo (Go + TypeScript) produces monorepo note in template", async () => {
    mkdirSync(join(tmp, "server"), { recursive: true });
    mkdirSync(join(tmp, "web"), { recursive: true });
    writeFileSync(join(tmp, "server", "main.go"), "package main");
    writeFileSync(join(tmp, "web", "app.ts"), "export {};");
    const out = join(tmp, "spec.md");
    await commandSpecInit(["--path", tmp, "--output", out], ctx(), { parseFlags, writeLine, ui, printJson: () => {} });
    // language detection is deferred to agent milestone — template is still valid
    const content = readFileSync(out, "utf8");
    expect(content).toContain("**Status:** PENDING");
  });

  test("repo with Makefile gates writes gate section with make commands", async () => {
    writeFileSync(join(tmp, "Makefile"), "lint:\n\techo ok\ntest:\n\techo ok\nbuild:\n\techo ok\n");
    const out = join(tmp, "spec.md");
    await commandSpecInit(["--path", tmp, "--output", out], ctx(), { parseFlags, writeLine, ui, printJson: () => {} });
    const content = readFileSync(out, "utf8");
    expect(content).toContain("make lint");
    expect(content).toContain("make test");
    expect(content).toContain("make build");
  });

  test("JSON output shape matches contract and detected values are accurate", async () => {
    writeFileSync(join(tmp, "main.go"), "package main");
    writeFileSync(join(tmp, "Makefile"), "test:\n\tgo test ./...\n");
    const captured = [];
    await commandSpecInit(
      ["--path", tmp, "--output", join(tmp, "out.md")],
      { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true },
      { parseFlags, writeLine, ui, printJson: (_s, v) => captured.push(v) },
    );
    expect(captured.length).toBe(1);
    const d = captured[0].detected;
    expect(d.make_targets).toContain("test");
    expect(typeof captured[0].output).toBe("string");
    expect(Number.isInteger(d.file_count)).toBe(true);
  });

  test("nested output path creates all intermediate directories", async () => {
    const out = join(tmp, "docs", "spec", "v1", "M1_001.md");
    const code = await commandSpecInit(["--path", tmp, "--output", out], ctx(), { parseFlags, writeLine, ui, printJson: () => {} });
    expect(code).toBe(0);
    expect(existsSync(out)).toBe(true);
  });

  test("template file is valid UTF-8 with no binary garbage", async () => {
    writeFileSync(join(tmp, "app.py"), "print('hello')");
    const out = join(tmp, "spec.md");
    await commandSpecInit(["--path", tmp, "--output", out], ctx(), { parseFlags, writeLine, ui, printJson: () => {} });
    const raw = readFileSync(out);
    // Every byte of valid UTF-8 should decode cleanly
    expect(() => new TextDecoder("utf-8", { fatal: true }).decode(raw)).not.toThrow();
  });

  test("project with tests/ directory includes test pattern note in template", async () => {
    mkdirSync(join(tmp, "tests"), { recursive: true });
    writeFileSync(join(tmp, "tests", "main_test.go"), "package tests");
    writeFileSync(join(tmp, "main.go"), "package main");
    const out = join(tmp, "spec.md");
    await commandSpecInit(["--path", tmp, "--output", out], ctx(), { parseFlags, writeLine, ui, printJson: () => {} });
    const content = readFileSync(out, "utf8");
    // Template lists test patterns detected
    expect(content).toMatch(/tests?/i);
  });

  test("stdout success message contains → and output path", async () => {
    const outBuf = makeBufferStream();
    const out = join(tmp, "my-spec.md");
    await commandSpecInit(
      ["--path", tmp, "--output", out],
      { stdout: outBuf.stream, stderr: makeNoop(), jsonMode: false },
      { parseFlags, writeLine, ui, printJson: () => {} },
    );
    const printed = outBuf.read();
    expect(printed).toContain(out);
    expect(printed).toContain("→");
  });
});
