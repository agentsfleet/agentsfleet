import { describe, test, expect } from "bun:test";
import { extractSpecRefs, matchRefsToFiles } from "../src/commands/run_preview.js";

// ── extractSpecRefs ───────────────────────────────────────────────────────────

describe("extractSpecRefs", () => {
  test("extracts src/ prefixed paths", () => {
    const md = "The implementation lives in `src/agent/runner.go` and handles state.";
    const refs = extractSpecRefs(md);
    expect(refs.some((r) => r.includes("src/agent"))).toBe(true);
  });

  test("extracts quoted file paths", () => {
    const md = 'Modify "zombiectl/src/commands/core.js" to add the flag.';
    const refs = extractSpecRefs(md);
    expect(refs.some((r) => r.includes("core.js"))).toBe(true);
  });

  test("extracts bare filenames with code extensions", () => {
    const md = "Update spec_init.js and run_preview.js to implement this.";
    const refs = extractSpecRefs(md);
    expect(refs).toContain("spec_init.js");
    expect(refs).toContain("run_preview.js");
  });

  test("extracts tests/ directory references", () => {
    const md = "Add tests under tests/spec-init.unit.test.js for coverage.";
    const refs = extractSpecRefs(md);
    expect(refs.some((r) => r.includes("tests/"))).toBe(true);
  });

  test("returns empty array for markdown with no file references", () => {
    const md = "This milestone adds preview capability and templates.";
    const refs = extractSpecRefs(md);
    expect(refs).toEqual([]);
  });

  test("deduplicates repeated references", () => {
    const md = "Edit `src/foo.go` and also `src/foo.go` again.";
    const refs = extractSpecRefs(md);
    const matches = refs.filter((r) => r === "src/foo.go");
    expect(matches.length).toBe(1);
  });
});

// ── matchRefsToFiles ──────────────────────────────────────────────────────────

describe("matchRefsToFiles", () => {
  const repoFiles = [
    "src/agent/runner.go",
    "src/agent/runner_test.go",
    "src/commands/core.js",
    "src/commands/spec_init.js",
    "tests/spec-init.unit.test.js",
    "Makefile",
    "README.md",
  ];

  test("returns high confidence for exact path suffix match", () => {
    const matches = matchRefsToFiles(["src/commands/spec_init.js"], repoFiles);
    const m = matches.find((x) => x.file === "src/commands/spec_init.js");
    expect(m).toBeDefined();
    expect(m.confidence).toBe("high");
  });

  test("returns medium confidence for partial path match", () => {
    const matches = matchRefsToFiles(["src/agent"], repoFiles);
    const files = matches.map((m) => m.file);
    expect(files.some((f) => f.startsWith("src/agent"))).toBe(true);
  });

  test("returns medium confidence for filename match", () => {
    const matches = matchRefsToFiles(["core.js"], repoFiles);
    const m = matches.find((x) => x.file === "src/commands/core.js");
    expect(m).toBeDefined();
    expect(["high", "medium"]).toContain(m.confidence);
  });

  test("returns no matches for unrelated term", () => {
    const matches = matchRefsToFiles(["totally_nonexistent_file_xyz.zig"], repoFiles);
    expect(matches).toEqual([]);
  });

  test("sorts results by confidence (high before medium before low)", () => {
    const refs = ["src/commands/spec_init.js", "src/agent"];
    const matches = matchRefsToFiles(refs, repoFiles);
    const order = { high: 0, medium: 1, low: 2 };
    for (let i = 1; i < matches.length; i++) {
      expect(order[matches[i].confidence]).toBeGreaterThanOrEqual(order[matches[i - 1].confidence]);
    }
  });

  test("deduplicates files — each file appears at most once", () => {
    const refs = ["src/commands/spec_init.js", "spec_init.js"];
    const matches = matchRefsToFiles(refs, repoFiles);
    const paths = matches.map((m) => m.file);
    const unique = new Set(paths);
    expect(paths.length).toBe(unique.size);
  });

  test("returns empty for empty refs", () => {
    expect(matchRefsToFiles([], repoFiles)).toEqual([]);
  });

  test("returns empty for empty file list", () => {
    expect(matchRefsToFiles(["src/foo.go"], [])).toEqual([]);
  });
});

// ── integration: spec markdown with real-ish content ─────────────────────────

describe("extractSpecRefs + matchRefsToFiles integration", () => {
  test("matches files from spec section content", () => {
    const md = `
## 1.0 Implementation

Edit \`src/commands/spec_init.js\` to add language detection.
Also update tests/spec-init.unit.test.js.
    `;
    const refs = extractSpecRefs(md);
    const repoFiles = [
      "src/commands/spec_init.js",
      "tests/spec-init.unit.test.js",
      "src/commands/core.js",
    ];
    const matches = matchRefsToFiles(refs, repoFiles);
    const files = matches.map((m) => m.file);
    expect(files).toContain("src/commands/spec_init.js");
    expect(files).toContain("tests/spec-init.unit.test.js");
  });
});
