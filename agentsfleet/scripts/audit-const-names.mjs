#!/usr/bin/env node

// Trap codemod-generated cryptic const names that oxlint can't see.
//
// Two patterns are rejected at lint time:
//   1. K_PUNCT_<hex6>   — hash-suffix names auto-emitted for punctuation
//                         literals. Hex disambiguates collisions but tells
//                         the reader nothing about intent.
//   2. K_<40+ uppercase chars>  — auto-derived from the literal's words
//                                  and silently truncated; the name often
//                                  drops the final clause of the sentence
//                                  it represents.
//
// Override (rare): suffix the const declaration line with `// audit-const-names: keep`.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { extname, join, resolve } from "node:path";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const SOURCE_DIRS = ["src", "bin"];
const OVERRIDE_TAG = "audit-const-names: keep";

const PATTERNS = [
  {
    name: "hash-suffix const",
    re: /\bconst\s+(K_PUNCT_[0-9A-F]{6,})\b/g,
    hint: "Rename to a semantic identifier (e.g. K_EM_DASH, K_COLUMN_GAP).",
  },
  {
    name: "over-long const (≥40 chars)",
    re: /\bconst\s+(K_[A-Z0-9_]{40,})\s*=/g,
    hint: "Truncated codemod output — pick a shorter semantic name (e.g. K_MSG_SERVER_DEGRADED).",
  },
];

const failures = [];

for (const file of listJsFiles(SOURCE_DIRS.map((d) => join(ROOT, d)))) {
  const source = readFileSync(file, "utf8");
  const lines = source.split("\n");
  for (const { name, re, hint } of PATTERNS) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(source))) {
      const line = source.slice(0, m.index).split("\n").length;
      if (lines[line - 1].includes(OVERRIDE_TAG)) continue;
      failures.push({ file: file.slice(ROOT.length + 1), line, ident: m[1], pattern: name, hint });
    }
  }
}

if (failures.length > 0) {
  console.error("Const-name audit failed:");
  for (const f of failures) {
    console.error(`  ${f.file}:${f.line}  ${f.ident}  [${f.pattern}]`);
    console.error(`    → ${f.hint}`);
  }
  console.error("\nOverride (rare): append `// audit-const-names: keep` to the declaration line.");
  process.exit(1);
}

console.log("Const-name audit passed");

function listJsFiles(roots) {
  const out = [];
  for (const root of roots) {
    if (!safeIsDir(root)) continue;
    walk(root, out);
  }
  return out;
}

function walk(dir, out) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) walk(full, out);
    else if (stat.isFile() && [".js", ".mjs"].includes(extname(full))) out.push(full);
  }
}

function safeIsDir(p) {
  try { return statSync(p).isDirectory(); } catch { return false; }
}
