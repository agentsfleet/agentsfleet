import { readFileSync, existsSync } from "node:fs";
import { join, relative } from "node:path";
import { walkDirForPreview } from "./run_preview_walk.js";

// Confidence display config: icon (TTY) + bracket label (no-TTY) + ANSI code
const CONF_DISPLAY = {
  high:   { ansi: "32", icon: "●", label: "[HIGH]" },  // green
  medium: { ansi: "33", icon: "◆", label: "[MED] " },  // yellow
  low:    { ansi:  "2", icon: "○", label: "[LOW] " },  // dim
};

/**
 * Returns the confidence indicator string appropriate for the output stream.
 * TTY → colored Unicode icon. Non-TTY / NO_COLOR → plain text label.
 * Exported for testing.
 */
export function confIndicator(confidence, stream) {
  const d = CONF_DISPLAY[confidence] ?? CONF_DISPLAY.low;
  // NO_COLOR spec: any non-empty value disables color
  const noColor = Boolean(process.env.NO_COLOR) || !stream?.isTTY;
  if (noColor) return d.label;
  return `\u001b[${d.ansi}m${d.icon}\u001b[0m`;
}

/**
 * Strip ANSI escape sequences from a string.
 * Prevents ANSI injection when filenames contain escape codes.
 */
export function sanitizeDisplay(str) {
  // eslint-disable-next-line no-control-regex
  return str.replace(/\x1b\[[0-9;]*m/g, "").replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, "");
}

/**
 * Extract candidate file path references from spec markdown.
 * Looks for:
 *   - Paths starting with src/, test/, tests/, lib/, pkg/, cmd/, internal/, docs/
 *   - Quoted paths: "some/path/file.ext" or `some/path`
 *   - Known module names (identifiers adjacent to / or .)
 */
export function extractSpecRefs(markdown) {
  const refs = new Set();

  // Quoted or backtick paths: "src/foo/bar.go", `lib/utils`
  const quotedRe = /["`']([a-zA-Z0-9_./-]{3,}[/.][a-zA-Z0-9_/-]+)["`']/g;
  for (const m of markdown.matchAll(quotedRe)) refs.add(m[1]);

  // src/, test/, tests/, lib/, pkg/, cmd/, internal/, docs/ prefix references
  const prefixRe = /\b((?:src|tests?|lib|pkg|cmd|internal|docs|app|web|api|workers?|scripts?)\/[a-zA-Z0-9_./-]+)/g;
  for (const m of markdown.matchAll(prefixRe)) refs.add(m[1]);

  // Filenames with known code extensions inside backtick code spans or inline references
  const fileRe = /\b([a-zA-Z0-9_-]+\.(?:go|rs|ts|tsx|js|mjs|py|zig|rb|java|kt|c|cpp|cs|swift|ex|exs|sh|yaml|yml|toml|json|md))\b/g;
  for (const m of markdown.matchAll(fileRe)) refs.add(m[1]);

  return [...refs];
}

/**
 * Score a file path against a reference term.
 * Returns "high" | "medium" | "low" | null.
 *
 * high   — full path segment matches the ref exactly (e.g. ref is "src/foo/bar.go" and file ends with it)
 * medium — a directory component or filename matches
 * low    — any substring match
 */
function scoreMatch(filePath, ref) {
  const fp = filePath.replace(/\\/g, "/");
  const r = ref.replace(/\\/g, "/");

  if (fp.endsWith(r) || fp === r) return "high";

  // Check if any path segment equals the ref
  const refParts = r.split("/").filter(Boolean);
  const fileParts = fp.split("/").filter(Boolean);

  // All ref parts appear as a contiguous sequence in the file path
  if (refParts.length >= 2) {
    const refStr = refParts.join("/");
    if (fp.includes(refStr)) return "medium";
  }

  // Single-segment ref matches a filename or directory
  if (refParts.length === 1) {
    const name = fileParts[fileParts.length - 1];
    if (name === refParts[0]) return "medium";
    if (name.startsWith(refParts[0]) || fp.includes(`/${refParts[0]}/`)) return "low";
  }

  // Substring match as fallback
  if (fp.includes(r)) return "low";

  return null;
}

const CONFIDENCE_ORDER = { high: 0, medium: 1, low: 2 };

/**
 * Match extracted spec refs against the repo file tree.
 * Returns array of { file, confidence } sorted by confidence then path.
 */
export function matchRefsToFiles(refs, repoFiles) {
  const best = new Map(); // file -> best confidence

  for (const ref of refs) {
    for (const file of repoFiles) {
      const conf = scoreMatch(file, ref);
      if (!conf) continue;
      const existing = best.get(file);
      if (!existing || CONFIDENCE_ORDER[conf] < CONFIDENCE_ORDER[existing]) {
        best.set(file, conf);
      }
    }
  }

  return [...best.entries()]
    .map(([file, confidence]) => ({ file, confidence }))
    .sort((a, b) => {
      const cmp = CONFIDENCE_ORDER[a.confidence] - CONFIDENCE_ORDER[b.confidence];
      return cmp !== 0 ? cmp : a.file.localeCompare(b.file);
    });
}

/**
 * Print the predicted impact list.
 */
const PREVIEW_TITLE = "Predicted file impact";

export function printPreview(stdout, matches, { writeLine, ui }) {
  if (matches.length === 0) {
    writeLine(stdout, ui.info("no file references detected in spec"));
    return;
  }

  writeLine(stdout, ui.head(PREVIEW_TITLE));
  writeLine(stdout, ui.dim("\u2500".repeat(PREVIEW_TITLE.length)));
  writeLine(stdout);

  for (const { file, confidence } of matches) {
    const indicator = confIndicator(confidence, stdout);
    writeLine(stdout, `  ${indicator}  ${sanitizeDisplay(file)}`);
  }
  writeLine(stdout);
  writeLine(stdout, ui.dim(`  ${matches.length} file(s) matched from spec analysis`));
}

/**
 * Run preview: parse spec file, scan repo, print impact.
 * Returns { matches } so callers can decide whether to proceed.
 */
export async function runPreview(specFile, repoPath, ctx, deps) {
  const { writeLine, ui } = deps;

  if (!existsSync(specFile)) {
    writeLine(ctx.stderr, ui.err(`spec file not found: ${specFile}`));
    return null;
  }

  let markdown;
  try {
    markdown = readFileSync(specFile, "utf8");
  } catch (err) {
    writeLine(ctx.stderr, ui.err(`failed to read spec file: ${err.message}`));
    return null;
  }

  const refs = extractSpecRefs(markdown);
  const repoFiles = walkDirForPreview(repoPath);

  // Make file paths relative to repoPath for cleaner display
  const relFiles = repoFiles.map((f) => relative(repoPath, f).replace(/\\/g, "/"));

  const matches = matchRefsToFiles(refs, relFiles);
  printPreview(ctx.stdout, matches, deps);

  return { matches };
}
