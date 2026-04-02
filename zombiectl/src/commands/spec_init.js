import { readFileSync, existsSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { walkDir } from "../lib/walk-dir.js";
import { agentLoop } from "../lib/agent-loop.js";

/**
 * Parse Makefile and return list of target names.
 */
export function parseMakeTargets(repoPath) {
  const makefile = join(repoPath, "Makefile");
  if (!existsSync(makefile)) return [];

  let content;
  try {
    content = readFileSync(makefile, "utf8");
  } catch {
    return [];
  }

  const targets = new Set();
  for (const line of content.split("\n")) {
    const m = line.match(/^([a-zA-Z][a-zA-Z0-9_.-]*)\s*:/);
    if (m && !m[1].startsWith(".")) targets.add(m[1]);
  }
  return [...targets];
}

/**
 * Detect test patterns from file paths.
 */
export function detectTestPatterns(files) {
  const patterns = new Set();
  for (const f of files) {
    const base = f.replace(/\\/g, "/");
    if (/(^|\/)tests?\//.test(base)) patterns.add("tests/ directory");
    if (/\.(test|spec)\.[a-z]+$/.test(base)) patterns.add("*.test.* / *.spec.*");
    if (/_test\.[a-z]+$/.test(base)) patterns.add("*_test.*");
    if (/\.test\.[a-z]+$/.test(base)) patterns.add("*.test.*");
  }
  return [...patterns];
}

/**
 * Detect project structure indicators.
 */
export function detectProjectStructure(repoPath) {
  const indicators = [];
  for (const dir of ["src", "tests", "test", "docs", "lib", "pkg", "cmd", "internal"]) {
    if (existsSync(join(repoPath, dir))) indicators.push(`${dir}/`);
  }
  return indicators;
}

/**
 * Scan a repo and return detected context.
 */
export function scanRepo(repoPath) {
  const files = walkDir(repoPath);
  return {
    makeTargets: parseMakeTargets(repoPath),
    testPatterns: detectTestPatterns(files),
    projectStructure: detectProjectStructure(repoPath),
    fileCount: files.length,
  };
}

/**
 * commandSpecInit: implement `zombiectl spec init [--path DIR] [--output PATH] [--describe TEXT]`
 *
 * With --describe: uses agent relay to generate a spec from intent (requires auth).
 * Without --describe: falls back to local template generation (no auth required).
 */
export async function commandSpecInit(args, ctx, deps) {
  const { parseFlags, writeLine, ui } = deps;
  const parsed = parseFlags(args);
  const repoPath = resolve(parsed.options.path || ".");
  const outputPath = parsed.options.output || "docs/spec/new-feature.md";
  const describe = parsed.options.describe;

  if (!existsSync(repoPath)) {
    writeLine(ctx.stderr, ui.err(`path not found: ${repoPath}`));
    return 2;
  }
  if (!statSync(repoPath).isDirectory()) {
    writeLine(ctx.stderr, ui.err(`path is not a directory: ${repoPath}`));
    return 2;
  }

  // Agent-backed generation when --describe is provided
  if (describe && ctx.workspaceId) {
    return agentSpecInit(describe, repoPath, outputPath, ctx, deps);
  }

  // Fallback: local template generation (no auth required)
  return localSpecInit(repoPath, outputPath, ctx, deps);
}

async function agentSpecInit(describe, repoPath, outputPath, ctx, deps) {
  const { writeLine, ui, printJson } = deps;
  const endpoint = `/v1/workspaces/${ctx.workspaceId}/spec/template`;

  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, "");
    writeLine(ctx.stdout, ui.dim("  \u{1F9DF} analyzing your repo..."));
  }

  const result = await agentLoop(endpoint, `Generate a spec template for: ${describe}`, repoPath, ctx, {
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
  });

  if (!result.text) {
    writeLine(ctx.stderr, ui.err("agent returned no content"));
    return 1;
  }

  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, "");
    writeLine(ctx.stdout, ui.dim("  \u{1F9DF} drafting spec..."));
  }

  const outDir = dirname(outputPath);
  try {
    mkdirSync(outDir, { recursive: true });
    writeFileSync(outputPath, result.text, "utf8");
  } catch (err) {
    writeLine(ctx.stderr, ui.err(`failed to write spec: ${err.message}`));
    return 1;
  }

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      output: outputPath,
      tool_calls: result.toolCalls,
      wall_ms: result.wallMs,
      usage: result.usage,
    });
  } else {
    writeLine(ctx.stdout, "");
    writeLine(ctx.stdout, ui.ok(`\u{2713} wrote ${outputPath}`));
    const secs = (result.wallMs / 1000).toFixed(1);
    const tokens = result.usage?.total_tokens ?? "?";
    writeLine(ctx.stdout, ui.dim(`    ${secs}s | ${result.toolCalls} reads | ${tokens} tokens`));
  }

  return 0;
}

function localSpecInit(repoPath, outputPath, ctx, deps) {
  const { writeLine, ui, printJson } = deps;
  const scan = scanRepo(repoPath);
  const template = generateTemplate(scan);

  const outDir = dirname(outputPath);
  try {
    mkdirSync(outDir, { recursive: true });
    writeFileSync(outputPath, template, "utf8");
  } catch (err) {
    writeLine(ctx.stderr, ui.err(`failed to write template: ${err.message}`));
    return 1;
  }

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      output: outputPath,
      detected: {
        make_targets: scan.makeTargets,
        test_patterns: scan.testPatterns,
        project_structure: scan.projectStructure,
        file_count: scan.fileCount,
      },
    });
  } else {
    writeLine(ctx.stdout, ui.ok(`template written \u{2192} ${outputPath}`));
    writeLine(ctx.stdout);
    const rows = [];
    if (scan.makeTargets.length > 0)      rows.push(["make targets", scan.makeTargets.join(", ")]);
    if (scan.testPatterns.length > 0)     rows.push(["test patterns", scan.testPatterns.join(", ")]);
    if (scan.projectStructure.length > 0) rows.push(["structure",     scan.projectStructure.join("  ")]);
    if (rows.length > 0) {
      const w = Math.max(...rows.map(([k]) => k.length));
      const sep = ui.dim("  \u{00B7}  ");
      for (const [k, v] of rows) {
        writeLine(ctx.stdout, `  ${ui.dim(k.padEnd(w))}${sep}${v}`);
      }
      writeLine(ctx.stdout);
    }
    writeLine(ctx.stdout, ui.dim(`${scan.fileCount} file(s) scanned`));
  }

  return 0;
}

function currentDateStr() {
  const d = new Date();
  const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
  return `${months[d.getMonth()]} ${String(d.getDate()).padStart(2,"0")}, ${d.getFullYear()}`;
}

/**
 * Generate a spec template markdown string from scan results.
 */
export function generateTemplate(scan) {
  const { makeTargets, testPatterns, projectStructure } = scan;

  const GATE_TARGETS = new Set(["lint","test","build","check","fmt","format","verify","qa","qa-smoke"]);
  const detectedGates = makeTargets.filter((t) => GATE_TARGETS.has(t));

  const gatesBlock = detectedGates.length > 0
    ? detectedGates.map((t) => `- \`make ${t}\``).join("\n")
    : "- _(no Makefile gates detected)_";

  const structureBlock = projectStructure.length > 0
    ? projectStructure.map((d) => `- \`${d}\``).join("\n")
    : "- _(empty or minimal repo)_";

  return `# M{N}_001: {Feature Title}

**Prototype:** v1.0.0
**Milestone:** M{N}
**Workstream:** 001
**Date:** ${currentDateStr()}
**Status:** PENDING
**Priority:** P1 \u{2014} {one-line description of what this workstream delivers}
**Batch:** B1
**Depends on:** \u{2014}

---

## 1.0 Implementation

**Status:** PENDING

Implement the feature below.

**Detected project structure:**
${structureBlock}

**Dimensions:**
- 1.1 PENDING {First implementation step}
- 1.2 PENDING {Second implementation step}
- 1.3 PENDING {Write or update tests}
- 1.4 PENDING {Handle edge cases}

---

## 2.0 Verification

**Status:** PENDING

**Detected gates:**
${gatesBlock}

**Test patterns detected:** ${testPatterns.length > 0 ? testPatterns.join(", ") : "none"}

**Dimensions:**
- 2.1 PENDING All detected gates pass
- 2.2 PENDING New tests cover the feature path
- 2.3 PENDING No regressions in existing tests

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 {Primary success criterion}
- [ ] 3.2 {Secondary success criterion}
- [ ] 3.3 All detected gates pass with no new failures

---

## 4.0 Out of Scope

- {Item not in scope}
- {Another out of scope item}
`;
}
