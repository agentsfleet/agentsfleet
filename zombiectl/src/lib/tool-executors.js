import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, sep, relative } from "node:path";

const MAX_GLOB_RESULTS = 500;
const MAX_FILE_SIZE = 256 * 1024; // 256KB

/**
 * Validate that a resolved path is within the repo root.
 * Returns { resolved } or { error }.
 */
export function validatePath(inputPath, repoRoot) {
  const resolved = resolve(repoRoot, inputPath);
  if (resolved !== repoRoot && !resolved.startsWith(repoRoot + sep)) {
    return { error: "path outside repo root" };
  }
  return { resolved };
}

/**
 * Execute a tool call locally. Returns the result string.
 */
export function executeTool(name, input, repoRoot) {
  switch (name) {
    case "read_file":
      return executeReadFile(input.path, repoRoot);
    case "list_dir":
      return executeListDir(input.path, repoRoot);
    case "glob":
      return executeGlob(input.pattern, repoRoot);
    default:
      return `error: unknown tool "${name}"`;
  }
}

function executeReadFile(path, repoRoot) {
  const v = validatePath(path, repoRoot);
  if (v.error) return `error: ${v.error}`;

  try {
    const stat = statSync(v.resolved);
    if (stat.isDirectory()) return "error: path is a directory, use list_dir instead";
    if (stat.size > MAX_FILE_SIZE) return `error: file too large (${stat.size} bytes, max ${MAX_FILE_SIZE})`;
    return readFileSync(v.resolved, "utf8");
  } catch (err) {
    if (err.code === "ENOENT") return `error: file not found: ${path}`;
    return `error: ${err.message}`;
  }
}

function executeListDir(path, repoRoot) {
  const v = validatePath(path || ".", repoRoot);
  if (v.error) return `error: ${v.error}`;

  try {
    const entries = readdirSync(v.resolved, { withFileTypes: true });
    return entries
      .filter((e) => e.name !== ".git")
      .map((e) => (e.isDirectory() ? `${e.name}/` : e.name))
      .sort()
      .join("\n");
  } catch (err) {
    if (err.code === "ENOENT") return `error: directory not found: ${path}`;
    return `error: ${err.message}`;
  }
}

function micromatch(filePath, pattern) {
  const regex = pattern
    .replace(/\./g, "\\.")
    .replace(/\*\*/g, "\0")
    .replace(/\*/g, "[^/]*")
    .replace(/\0/g, ".*")
    .replace(/\?/g, "[^/]");
  return new RegExp(`^${regex}$`).test(filePath);
}

function walkSync(dir, root, results, limit) {
  let entries;
  try { entries = readdirSync(dir, { withFileTypes: true }); } catch { return; }
  for (const entry of entries) {
    if (results.length >= limit) return;
    if (entry.name === ".git") continue;
    const full = resolve(dir, entry.name);
    if (entry.isDirectory()) {
      walkSync(full, root, results, limit);
    } else {
      results.push(relative(root, full));
    }
  }
}

function executeGlob(pattern, repoRoot) {
  try {
    const allFiles = [];
    walkSync(repoRoot, repoRoot, allFiles, MAX_GLOB_RESULTS * 10);
    const results = [];
    for (const f of allFiles) {
      if (micromatch(f, pattern)) {
        results.push(f);
        if (results.length >= MAX_GLOB_RESULTS) break;
      }
    }
    if (results.length === 0) return "(no matches)";
    return results.sort().join("\n");
  } catch (err) {
    return `error: ${err.message}`;
  }
}
