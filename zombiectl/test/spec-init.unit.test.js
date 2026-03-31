import { describe, test, expect } from "bun:test";
import { parseMakeTargets, detectTestPatterns, detectProjectStructure, generateTemplate } from "../src/commands/spec_init.js";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import os from "node:os";

// ── parseMakeTargets ──────────────────────────────────────────────────────────

describe("parseMakeTargets", () => {
  let tmpDir;

  function setup(makefileContent) {
    tmpDir = mkdirSync(join(os.tmpdir(), `spec-init-test-${Date.now()}`), { recursive: true }) || join(os.tmpdir(), `spec-init-test-${Date.now()}`);
    // mkdirSync returns undefined on success for existing dirs; use a unique path
    tmpDir = join(os.tmpdir(), `spec-init-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    mkdirSync(tmpDir, { recursive: true });
    if (makefileContent !== null) writeFileSync(join(tmpDir, "Makefile"), makefileContent);
    return tmpDir;
  }

  function cleanup() {
    try { rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }

  test("returns empty array when no Makefile", () => {
    setup(null);
    try {
      expect(parseMakeTargets(tmpDir)).toEqual([]);
    } finally {
      cleanup();
    }
  });

  test("parses standard make targets", () => {
    setup("lint:\n\techo lint\n\ntest:\n\techo test\n\nbuild:\n\techo build\n");
    try {
      const targets = parseMakeTargets(tmpDir);
      expect(targets).toContain("lint");
      expect(targets).toContain("test");
      expect(targets).toContain("build");
    } finally {
      cleanup();
    }
  });

  test("ignores hidden/special targets starting with dot", () => {
    setup(".PHONY: lint\nlint:\n\techo lint\n");
    try {
      const targets = parseMakeTargets(tmpDir);
      expect(targets).not.toContain(".PHONY");
      expect(targets).toContain("lint");
    } finally {
      cleanup();
    }
  });

  test("parses targets with hyphens and underscores", () => {
    setup("lint-zig:\n\tzig fmt\ntest_unit:\n\tbun test\n");
    try {
      const targets = parseMakeTargets(tmpDir);
      expect(targets).toContain("lint-zig");
      expect(targets).toContain("test_unit");
    } finally {
      cleanup();
    }
  });
});

// ── detectTestPatterns ────────────────────────────────────────────────────────

describe("detectTestPatterns", () => {
  test("returns empty for no test files", () => {
    expect(detectTestPatterns(["src/main.go", "src/server.go"])).toEqual([]);
  });

  test("detects tests/ directory", () => {
    const patterns = detectTestPatterns(["tests/foo_test.go"]);
    expect(patterns).toContain("tests/ directory");
  });

  test("detects *.test.* pattern", () => {
    const patterns = detectTestPatterns(["src/foo.test.js"]);
    expect(patterns.some((p) => p.includes("test"))).toBe(true);
  });

  test("detects *_test.* pattern (Go)", () => {
    const patterns = detectTestPatterns(["src/server_test.go"]);
    expect(patterns).toContain("*_test.*");
  });

  test("deduplicates patterns", () => {
    const files = ["a.test.js", "b.test.js", "c.test.ts"];
    const patterns = detectTestPatterns(files);
    const counts = patterns.filter((p) => p.includes("test")).length;
    expect(counts).toBeLessThanOrEqual(2);
  });
});

// ── detectProjectStructure ────────────────────────────────────────────────────

describe("detectProjectStructure", () => {
  let tmpDir;

  function setup(dirs) {
    tmpDir = join(os.tmpdir(), `spec-proj-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    mkdirSync(tmpDir, { recursive: true });
    for (const d of dirs) mkdirSync(join(tmpDir, d), { recursive: true });
    return tmpDir;
  }

  function cleanup() {
    try { rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }

  test("returns empty for bare repo", () => {
    setup([]);
    try {
      expect(detectProjectStructure(tmpDir)).toEqual([]);
    } finally {
      cleanup();
    }
  });

  test("detects src/ and docs/", () => {
    setup(["src", "docs"]);
    try {
      const structure = detectProjectStructure(tmpDir);
      expect(structure).toContain("src/");
      expect(structure).toContain("docs/");
    } finally {
      cleanup();
    }
  });
});

// ── generateTemplate ──────────────────────────────────────────────────────────

describe("generateTemplate", () => {
  test("includes detected make targets in gates section", () => {
    const scan = { makeTargets: ["lint", "test", "build"], testPatterns: [], projectStructure: ["src/"] };
    const tpl = generateTemplate(scan);
    expect(tpl).toContain("make lint");
    expect(tpl).toContain("make test");
  });

  test("produces valid template with empty gates section when no Makefile", () => {
    const scan = { makeTargets: [], testPatterns: [], projectStructure: [] };
    const tpl = generateTemplate(scan);
    expect(tpl).toContain("no Makefile gates detected");
    expect(tpl).toContain("Acceptance Criteria");
    expect(tpl).toContain("PENDING");
  });

  test("includes detected project structure", () => {
    const scan = { makeTargets: [], testPatterns: [], projectStructure: ["src/", "docs/"] };
    const tpl = generateTemplate(scan);
    expect(tpl).toContain("`src/`");
    expect(tpl).toContain("`docs/`");
  });
});
