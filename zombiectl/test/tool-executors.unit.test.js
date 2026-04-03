import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { validatePath, executeTool } from "../src/lib/tool-executors.js";

function makeTmp() {
  const dir = join(import.meta.dir, ".tmp-tool-exec-" + Date.now());
  mkdirSync(dir, { recursive: true });
  return dir;
}

describe("validatePath", () => {
  test("allows path within repo root", () => {
    const result = validatePath("src/main.js", "/repo");
    expect(result.resolved).toBe("/repo/src/main.js");
    expect(result.error).toBeUndefined();
  });

  test("allows repo root itself", () => {
    const result = validatePath(".", "/repo");
    expect(result.resolved).toBe("/repo");
    expect(result.error).toBeUndefined();
  });

  test("rejects path traversal with ../", () => {
    const result = validatePath("../../.ssh/id_rsa", "/repo");
    expect(result.error).toBe("path outside repo root");
    expect(result.resolved).toBeUndefined();
  });

  test("rejects absolute path outside repo", () => {
    const result = validatePath("/etc/passwd", "/repo");
    expect(result.error).toBe("path outside repo root");
  });
});

describe("executeTool — read_file", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("reads file content", () => {
    writeFileSync(join(tmp, "hello.txt"), "world");
    const result = executeTool("read_file", { path: "hello.txt" }, tmp);
    expect(result).toBe("world");
  });

  test("returns error for missing file", () => {
    const result = executeTool("read_file", { path: "nope.txt" }, tmp);
    expect(result).toContain("error:");
    expect(result).toContain("not found");
  });

  test("rejects path traversal", () => {
    const result = executeTool("read_file", { path: "../../.ssh/id_rsa" }, tmp);
    expect(result).toContain("error: path outside repo root");
  });
});

describe("executeTool — list_dir", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("lists directory entries", () => {
    mkdirSync(join(tmp, "subdir"));
    writeFileSync(join(tmp, "file.txt"), "");
    const result = executeTool("list_dir", { path: "." }, tmp);
    expect(result).toContain("file.txt");
    expect(result).toContain("subdir/");
  });

  test("filters out .git", () => {
    mkdirSync(join(tmp, ".git"));
    writeFileSync(join(tmp, "file.txt"), "");
    const result = executeTool("list_dir", { path: "." }, tmp);
    expect(result).not.toContain(".git");
  });

  test("rejects path traversal", () => {
    const result = executeTool("list_dir", { path: "../.." }, tmp);
    expect(result).toContain("error: path outside repo root");
  });
});

describe("executeTool — glob", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("matches files by pattern", () => {
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "main.js"), "");
    writeFileSync(join(tmp, "src", "util.js"), "");
    const result = executeTool("glob", { pattern: "src/*.js" }, tmp);
    expect(result).toContain("src/main.js");
    expect(result).toContain("src/util.js");
  });

  test("returns no matches message", () => {
    const result = executeTool("glob", { pattern: "*.nonexistent" }, tmp);
    expect(result).toBe("(no matches)");
  });

  test("** matches deeply nested files", () => {
    mkdirSync(join(tmp, "a", "b", "c"), { recursive: true });
    writeFileSync(join(tmp, "a", "b", "c", "deep.js"), "");
    writeFileSync(join(tmp, "a", "top.js"), "");
    const result = executeTool("glob", { pattern: "**/*.js" }, tmp);
    expect(result).toContain("a/b/c/deep.js");
    expect(result).toContain("a/top.js");
  });

  test("? matches single character", () => {
    writeFileSync(join(tmp, "a.js"), "");
    writeFileSync(join(tmp, "ab.js"), "");
    const result = executeTool("glob", { pattern: "?.js" }, tmp);
    expect(result).toContain("a.js");
    expect(result).not.toContain("ab.js");
  });

  test("excludes .git directory", () => {
    mkdirSync(join(tmp, ".git", "objects"), { recursive: true });
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, ".git", "config"), "");
    writeFileSync(join(tmp, "src", "real.js"), "");
    const result = executeTool("glob", { pattern: "**/*.js" }, tmp);
    expect(result).toContain("src/real.js");
    expect(result).not.toContain(".git");
    expect(result).not.toContain("config");
  });

  test("* does not cross directory boundaries", () => {
    mkdirSync(join(tmp, "src"), { recursive: true });
    writeFileSync(join(tmp, "src", "nested.js"), "");
    writeFileSync(join(tmp, "root.js"), "");
    const result = executeTool("glob", { pattern: "*.js" }, tmp);
    expect(result).toContain("root.js");
    expect(result).not.toContain("src/nested.js");
  });

  test("matches dotfiles in pattern", () => {
    writeFileSync(join(tmp, ".env"), "");
    writeFileSync(join(tmp, ".gitignore"), "");
    const result = executeTool("glob", { pattern: ".*" }, tmp);
    expect(result).toContain(".env");
    expect(result).toContain(".gitignore");
  });
});

describe("executeTool — unknown tool", () => {
  test("returns error for unknown tool name", () => {
    const result = executeTool("write_file", { path: "x" }, "/tmp");
    expect(result).toContain('error: unknown tool "write_file"');
  });
});
