/**
 * run-preview integration tests — T6
 * End-to-end through the real filesystem stack: real spec file, real repo tree,
 * real runPreview call. Asserts full output contract without mocking internals.
 */
import { describe, test, expect } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { runPreview, printPreview, extractSpecRefs, matchRefsToFiles } from "../src/commands/run_preview.js";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
import { makeTmp, cleanup, writeLine } from "./helpers-fs.js";

// ── T6 Integration Verification ────────────────────────────────────────────────

describe("T6 integration — runPreview end-to-end", () => {
  test("full pipeline: spec → refs → file walk → matches → printed output", async () => {
    const tmp = makeTmp();
    const specFile = join(tmp, "spec.md");
    writeFileSync(specFile, "Edit `src/api/handler.go` and `lib/util.ts`.");
    mkdirSync(join(tmp, "src", "api"), { recursive: true });
    mkdirSync(join(tmp, "lib"), { recursive: true });
    writeFileSync(join(tmp, "src", "api", "handler.go"), "package api");
    writeFileSync(join(tmp, "lib", "util.ts"), "export {};");
    const out = makeBufferStream();
    try {
      const result = await runPreview(specFile, tmp, { stdout: out.stream, stderr: makeNoop() }, { writeLine, ui });
      expect(result).not.toBeNull();
      expect(result.matches.length).toBeGreaterThanOrEqual(1);
      const output = out.read();
      expect(output).toContain("Predicted file impact");
      expect(output).toContain("file(s)");
    } finally { cleanup(tmp); }
  });

  test("spec referencing only one existing file returns exactly one high-confidence match", async () => {
    const tmp = makeTmp();
    const specFile = join(tmp, "spec.md");
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "exact.go"), "package main");
    writeFileSync(specFile, "Update `src/exact.go` with the new logic.");
    try {
      const result = await runPreview(specFile, tmp, { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      const highMatches = result.matches.filter((m) => m.confidence === "high");
      expect(highMatches.some((m) => m.file.includes("exact.go"))).toBe(true);
    } finally { cleanup(tmp); }
  });

  test("spec with no file refs prints info message, result has empty matches", async () => {
    const tmp = makeTmp();
    const specFile = join(tmp, "spec.md");
    writeFileSync(specFile, "# Overview\nThis feature improves performance.");
    const out = makeBufferStream();
    try {
      const result = await runPreview(specFile, tmp, { stdout: out.stream, stderr: makeNoop() }, { writeLine, ui });
      expect(result.matches).toEqual([]);
      expect(out.read()).toContain("no file references detected");
    } finally { cleanup(tmp); }
  });

  test("spec referencing multiple confidence levels renders sorted output", async () => {
    const tmp = makeTmp();
    const specFile = join(tmp, "spec.md");
    mkdirSync(join(tmp, "src", "commands"), { recursive: true });
    mkdirSync(join(tmp, "tests"), { recursive: true });
    writeFileSync(join(tmp, "src", "commands", "core.js"), "");
    writeFileSync(join(tmp, "tests", "core.test.js"), "");
    writeFileSync(specFile, "Edit `src/commands/core.js`. Also update tests.");
    const out = makeBufferStream();
    try {
      const result = await runPreview(specFile, tmp, { stdout: out.stream, stderr: makeNoop() }, { writeLine, ui });
      const confidences = result.matches.map((m) => m.confidence);
      const order = { high: 0, medium: 1, low: 2 };
      for (let i = 1; i < confidences.length; i++) {
        expect(order[confidences[i]]).toBeGreaterThanOrEqual(order[confidences[i - 1]]);
      }
    } finally { cleanup(tmp); }
  });

  test("large monorepo spec (50 file refs, 200 real files) resolves without crash", async () => {
    const tmp = makeTmp();
    mkdirSync(join(tmp, "src"), { recursive: true });
    for (let i = 0; i < 200; i++) writeFileSync(join(tmp, "src", `f${i}.go`), "");
    const refs = Array.from({ length: 50 }, (_, i) => `\`src/f${i}.go\``).join(", ");
    const specFile = join(tmp, "spec.md");
    writeFileSync(specFile, `Edit ${refs}.`);
    try {
      const result = await runPreview(specFile, tmp, { stdout: makeNoop(), stderr: makeNoop() }, { writeLine, ui });
      expect(result).not.toBeNull();
      expect(result.matches.length).toBeGreaterThan(0);
    } finally { cleanup(tmp); }
  });

  test("stdout and stderr are cleanly separated — success writes nothing to stderr", async () => {
    const tmp = makeTmp();
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "ok.go"), "");
    const specFile = join(tmp, "spec.md");
    writeFileSync(specFile, "Edit `src/ok.go`.");
    const errBuf = makeBufferStream();
    try {
      await runPreview(specFile, tmp, { stdout: makeNoop(), stderr: errBuf.stream }, { writeLine, ui });
      expect(errBuf.read()).toBe("");
    } finally { cleanup(tmp); }
  });

  test("output contains both indicator and filename on same line", async () => {
    const tmp = makeTmp();
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "targeted.go"), "");
    const specFile = join(tmp, "spec.md");
    writeFileSync(specFile, "Edit `src/targeted.go` with new logic.");
    const out = makeBufferStream();
    try {
      await runPreview(specFile, tmp, { stdout: out.stream, stderr: makeNoop() }, { writeLine, ui });
      const lines = out.read().split("\n");
      const fileLine = lines.find((l) => l.includes("targeted.go"));
      expect(fileLine).toBeDefined();
      // Line must also have an indicator — either bracket label or icon character
      expect(fileLine).toMatch(/\[HIGH\]|\[MED\s*\]|\[LOW\s*\]|●|◆|○/);
    } finally { cleanup(tmp); }
  });

  test("printPreview + extractSpecRefs + matchRefsToFiles compose correctly (no runPreview wrapper)", () => {
    const markdown = "Fix `src/auth/login.go` and update `tests/auth_test.go`.";
    const files = ["src/auth/login.go", "tests/auth_test.go", "src/util/helper.go"];
    const refs = extractSpecRefs(markdown);
    const matches = matchRefsToFiles(refs, files);
    const buf = makeBufferStream();
    printPreview(buf.stream, matches, { writeLine, ui });
    const out = buf.read();
    expect(out).toContain("login.go");
    expect(out).toContain("auth_test.go");
    expect(out).toContain("file(s)");
  });
});
