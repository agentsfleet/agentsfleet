import { readFileSync, existsSync } from "node:fs";
import { resolve, relative } from "node:path";
import { agentLoop } from "../lib/agent-loop.js";

// Confidence display config: icon (TTY) + bracket label (no-TTY) + ANSI code
const CONF_DISPLAY = {
  high:   { ansi: "32", icon: "\u{25CF}", label: "[HIGH]" },  // green
  medium: { ansi: "33", icon: "\u{25C6}", label: "[MED] " },  // yellow
  low:    { ansi:  "2", icon: "\u{25CB}", label: "[LOW] " },  // dim
};

/**
 * Returns the confidence indicator string appropriate for the output stream.
 * TTY -> colored Unicode icon. Non-TTY / NO_COLOR -> plain text label.
 */
export function confIndicator(confidence, stream) {
  const d = CONF_DISPLAY[confidence] ?? CONF_DISPLAY.low;
  const noColor = Boolean(process.env.NO_COLOR) || !stream?.isTTY;
  if (noColor) return d.label;
  return `\u001b[${d.ansi}m${d.icon}\u001b[0m`;
}

/**
 * Strip ANSI escape sequences from a string.
 */
export function sanitizeDisplay(str) {
  // eslint-disable-next-line no-control-regex
  return str.replace(/\x1b\[[0-9;]*m/g, "").replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, "");
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
  writeLine(stdout, ui.dim(`  ${matches.length} file(s) in blast radius`));
}

/**
 * Parse agent text output into structured matches.
 * Expected format: "● src/file.go  high  — reason" or similar.
 */
function parseAgentMatches(text) {
  const matches = [];
  const lineRe = /^[●◆○\s]*(\S+)\s+(high|medium|low)/gm;
  let m;
  while ((m = lineRe.exec(text)) !== null) {
    matches.push({ file: m[1], confidence: m[2] });
  }
  return matches;
}

/**
 * Run preview using agent relay: agent reads spec + explores repo.
 * Returns { matches } or null on error.
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

  const absRepoPath = resolve(repoPath);

  // If workspace is available, use agent-backed preview
  if (ctx.workspaceId) {
    return agentPreview(markdown, absRepoPath, ctx, deps);
  }

  // Fallback: local heuristic preview (legacy, kept for offline use)
  return localPreview(markdown, absRepoPath, ctx, deps);
}

async function agentPreview(markdown, repoPath, ctx, deps) {
  const { writeLine, ui } = deps;
  const endpoint = `/v1/workspaces/${ctx.workspaceId}/spec/preview`;

  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, "");
    writeLine(ctx.stdout, ui.dim("  \u{1F9DF} analyzing your repo against spec..."));
  }

  const result = await agentLoop(
    endpoint,
    `Which files will this spec touch?\n\n${markdown}`,
    repoPath,
    ctx,
    {
      onToolCall: (tc) => {
        if (!ctx.jsonMode) {
          const label = tc.name === "read_file" ? `read ${tc.input.path}`
            : tc.name === "list_dir" ? `listed ${tc.input.path || "./"}`
            : `glob ${tc.input.pattern}`;
          writeLine(ctx.stdout, ui.dim(`  \u{2192} ${label}`));
        }
      },
      onError: (msg) => {
        writeLine(ctx.stderr, ui.err(msg));
      },
    },
  );

  if (!result.text) {
    writeLine(ctx.stderr, ui.err("agent returned no content"));
    return null;
  }

  const matches = parseAgentMatches(result.text);
  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, "");
    printPreview(ctx.stdout, matches, deps);
    const secs = (result.wallMs / 1000).toFixed(1);
    const tokens = result.usage?.total_tokens ?? "?";
    writeLine(ctx.stdout, ui.dim(`    ${secs}s | ${result.toolCalls} reads | ${tokens} tokens`));
  }

  return { matches };
}

/**
 * Local heuristic preview — kept for offline/no-auth scenarios.
 * Uses regex to extract file refs from spec and match against repo tree.
 */
async function localPreview(markdown, repoPath, ctx, deps) {
  const { walkDirForPreview } = await import("./run_preview_walk.js");

  const refs = extractSpecRefs(markdown);
  const repoFiles = walkDirForPreview(repoPath);
  const relFiles = repoFiles.map((f) => relative(repoPath, f).replace(/\\/g, "/"));
  const matches = matchRefsToFiles(refs, relFiles);
  printPreview(ctx.stdout, matches, deps);
  return { matches };
}

// ── Legacy heuristic functions (used by localPreview fallback) ──────────────

export function extractSpecRefs(markdown) {
  const refs = new Set();
  const quotedRe = /["`']([a-zA-Z0-9_./-]{3,}[/.][a-zA-Z0-9_/-]+)["`']/g;
  for (const m of markdown.matchAll(quotedRe)) refs.add(m[1]);
  const prefixRe = /\b((?:src|tests?|lib|pkg|cmd|internal|docs|app|web|api|workers?|scripts?)\/[a-zA-Z0-9_./-]+)/g;
  for (const m of markdown.matchAll(prefixRe)) refs.add(m[1]);
  const fileRe = /\b([a-zA-Z0-9_-]+\.(?:go|rs|ts|tsx|js|mjs|py|zig|rb|java|kt|c|cpp|cs|swift|ex|exs|sh|yaml|yml|toml|json|md))\b/g;
  for (const m of markdown.matchAll(fileRe)) refs.add(m[1]);
  return [...refs];
}

function scoreMatch(filePath, ref) {
  const fp = filePath.replace(/\\/g, "/");
  const r = ref.replace(/\\/g, "/");
  if (fp.endsWith(r) || fp === r) return "high";
  const refParts = r.split("/").filter(Boolean);
  const fileParts = fp.split("/").filter(Boolean);
  if (refParts.length >= 2) {
    const refStr = refParts.join("/");
    if (fp.includes(refStr)) return "medium";
  }
  if (refParts.length === 1) {
    const name = fileParts[fileParts.length - 1];
    if (name === refParts[0]) return "medium";
    if (name.startsWith(refParts[0]) || fp.includes(`/${refParts[0]}/`)) return "low";
  }
  if (fp.includes(r)) return "low";
  return null;
}

const CONFIDENCE_ORDER = { high: 0, medium: 1, low: 2 };

export function matchRefsToFiles(refs, repoFiles) {
  const best = new Map();
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
