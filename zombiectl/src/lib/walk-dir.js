import { readdirSync } from "node:fs";
import { join } from "node:path";

// Only universally safe ignores — language-specific build dirs (.zig-cache,
// target, node_modules, vendor, etc.) are deferred to the agent walk milestone.
const IGNORED_DIRS = new Set([".git", ".worktrees"]);

/**
 * Walk a directory (BFS, depth-limited) and collect file paths.
 * @param {string} rootPath
 * @param {number} maxDepth  default 5 — preview needs one extra level vs spec scan
 * @returns {string[]} absolute file paths
 */
export function walkDir(rootPath, maxDepth = 5) {
  const results = [];
  const queue = [{ path: rootPath, depth: 0 }];

  while (queue.length > 0) {
    const { path: current, depth } = queue.shift();
    if (depth > maxDepth) continue;

    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }

    for (const entry of entries) {
      if (IGNORED_DIRS.has(entry.name)) continue;
      const fullPath = join(current, entry.name);
      if (entry.isDirectory()) {
        queue.push({ path: fullPath, depth: depth + 1 });
      } else if (entry.isFile()) {
        results.push(fullPath);
      }
    }
  }

  return results;
}
