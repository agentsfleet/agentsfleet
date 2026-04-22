// M1_001 §5.0 — Zombie CLI commands.
//
// Flat top-level for common ops: install, up, status, kill, logs.
// Namespaced for less common: credential add, credential list.

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";
import {
  wsZombiesPath,
  wsZombiePath,
  wsZombieActivityPath,
} from "../lib/api-paths.js";
import { commandCredential } from "./zombie_credential.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEMPLATES_DIR = join(__dirname, "../../templates");

const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";

const BUNDLED_TEMPLATES = ["lead-collector", "slack-bug-fixer"];

export async function commandZombie(ctx, args, workspaces, deps) {
  const action = args[0];
  const { ui, writeLine, writeError } = deps;

  if (action === "install") return commandInstall(ctx, args.slice(1), deps);
  if (action === "up") return commandUp(ctx, args.slice(1), workspaces, deps);
  if (action === "status") return commandStatus(ctx, args.slice(1), workspaces, deps);
  if (action === "kill") return commandKill(ctx, args.slice(1), workspaces, deps);
  if (action === "logs") return commandLogs(ctx, args.slice(1), workspaces, deps);
  if (action === "credential") return commandCredential(ctx, args.slice(1), workspaces, deps);

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown zombie subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err(`unknown zombie subcommand: ${action ?? "(none)"}`));
    writeLine(ctx.stderr);
    // non-JSON: preserve multi-line usage text not expressible as a single message
    writeLine(ctx.stderr, "usage: zombiectl install <template>");
    writeLine(ctx.stderr, "       zombiectl up");
    writeLine(ctx.stderr, "       zombiectl status");
    writeLine(ctx.stderr, "       zombiectl kill");
    writeLine(ctx.stderr, "       zombiectl logs");
    writeLine(ctx.stderr, "       zombiectl credential add|list");
  }
  return 2;
}

// ── install ──────────────────────────────────────────────────────────────

async function commandInstall(ctx, args, deps) {
  const { ui, writeLine, writeError, printJson } = deps;

  const templateName = args[0];
  if (!templateName) {
    if (ctx.jsonMode) {
      writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl install <template>", deps);
    } else {
      writeLine(ctx.stderr, ui.err("usage: zombiectl install <template>"));
      writeLine(ctx.stderr);
      writeLine(ctx.stderr, "Available templates:");
      for (const t of BUNDLED_TEMPLATES) {
        writeLine(ctx.stderr, `  ${t}`);
      }
    }
    return 2;
  }

  if (!BUNDLED_TEMPLATES.includes(templateName)) {
    if (ctx.jsonMode) {
      writeError(ctx, "UNKNOWN_TEMPLATE", `unknown template: ${templateName}. Available: ${BUNDLED_TEMPLATES.join(", ")}`, deps);
    } else {
      writeLine(ctx.stderr, ui.err(`unknown template: ${templateName}`));
      writeLine(ctx.stderr, `Available templates: ${BUNDLED_TEMPLATES.join(", ")}`);
    }
    return 2;
  }

  const templateDir = join(TEMPLATES_DIR, templateName);
  let skillContent, triggerContent;
  try {
    skillContent = readFileSync(join(templateDir, SKILL_FILENAME), "utf-8");
    triggerContent = readFileSync(join(templateDir, TRIGGER_FILENAME), "utf-8");
  } catch {
    writeError(ctx, "TEMPLATE_NOT_FOUND", `template directory not found: ${templateName}/`, deps);
    return 1;
  }

  const outputDir = join(process.cwd(), templateName);
  try {
    mkdirSync(outputDir, { recursive: true });
    writeFileSync(join(outputDir, SKILL_FILENAME), skillContent, "utf-8");
    writeFileSync(join(outputDir, TRIGGER_FILENAME), triggerContent, "utf-8");
  } catch (err) {
    writeError(ctx, "IO_ERROR", `failed to write ${templateName}/: ${err.message}`, deps);
    return 1;
  }

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      status: "installed",
      template: templateName,
      path: outputDir,
    });
  } else {
    writeLine(ctx.stdout, ui.ok(`${templateName} installed.`));
    writeLine(ctx.stdout, `  Created ${templateName}/`);
    writeLine(ctx.stdout, `    ${SKILL_FILENAME}   — agent instructions (edit this)`);
    writeLine(ctx.stdout, `    ${TRIGGER_FILENAME} — deployment config`);
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, "Next steps:");
    writeLine(ctx.stdout, `  zombiectl up              Start the zombie`);
    writeLine(ctx.stdout, `  zombiectl credential add  Add credentials (optional)`);
  }

  return 0;
}

// ── up ───────────────────────────────────────────────────────────────────

async function commandUp(ctx, args, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;

  // Find a zombie directory in the current directory
  const zombieDir = findZombieDir(process.cwd());
  if (!zombieDir) {
    if (ctx.jsonMode) {
      writeError(ctx, "NO_CONFIG", "no zombie directory found. Run: zombiectl install <template>", deps);
    } else {
      writeLine(ctx.stderr, ui.err("No zombie directory found in current directory."));
      writeLine(ctx.stderr, "Run: zombiectl install <template>");
    }
    return 1;
  }

  let skillMd, triggerMd;
  try {
    skillMd = readFileSync(join(zombieDir, SKILL_FILENAME), "utf-8");
    triggerMd = readFileSync(join(zombieDir, TRIGGER_FILENAME), "utf-8");
  } catch (err) {
    writeError(ctx, "IO_ERROR", `failed to read zombie files: ${err.message}`, deps);
    return 1;
  }

  // Extract name from TRIGGER.md frontmatter (minimal: just the name line)
  const nameMatch = triggerMd.match(/^name:\s*(.+)$/m);
  const zombieName = nameMatch ? nameMatch[1].trim() : basename(zombieDir);

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  // Deploy to UseZombie cloud — server parses TRIGGER.md, no client-side YAML parsing.
  // M24_001: workspace_id moved to URL path (RULE RAD §4); body carries content only.
  const res = await request(ctx, wsZombiesPath(wsId), {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({
      source_markdown: skillMd,
      trigger_markdown: triggerMd,
    }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, "🎉 Woohoo! Your zombie is installed and ready to run.");
    if (res.webhook_url) {
      writeLine(ctx.stdout, res.webhook_url);
    }
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.ok(`${zombieName} is live.`));
    if (res.zombie_id) {
      writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
    }
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, "Commands:");
    writeLine(ctx.stdout, `  zombiectl status          Check status`);
    writeLine(ctx.stdout, `  zombiectl logs            View activity`);
    writeLine(ctx.stdout, `  zombiectl kill            Stop the zombie`);
  }

  return 0;
}

// ── status ───────────────────────────────────────────────────────────────

async function commandStatus(ctx, args, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError } = deps;

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  const res = await request(ctx, wsZombiesPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const zombies = res.items ?? [];
  if (zombies.length === 0) {
    writeLine(ctx.stdout, ui.info("No zombies running. Install one with: zombiectl install <template>"));
    return 0;
  }

  printSection(ctx.stdout, "Zombies");
  for (const z of zombies) {
    const budget = z.budget_used_dollars != null ? `$${z.budget_used_dollars.toFixed(2)}` : "—";
    printKeyValue(ctx.stdout, {
      Name: z.name,
      Status: z.status,
      Events: String(z.events_processed ?? 0),
      Budget: budget,
    });
    writeLine(ctx.stdout);
  }

  return 0;
}

// ── kill ─────────────────────────────────────────────────────────────────

async function commandKill(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const zombieName = parsed.positionals[0];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  // M24_001: per-zombie DELETE only; the collection-DELETE (kill-all) was never
  // implemented server-side — falls through to a 405 if attempted.
  const endpoint = zombieName
    ? wsZombiePath(wsId, zombieName)
    : wsZombiesPath(wsId);

  const res = await request(ctx, endpoint, {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    const name = zombieName || "all zombies";
    writeLine(ctx.stdout, ui.ok(`${name} killed.`));
  }

  return 0;
}

// ── logs ─────────────────────────────────────────────────────────────────

async function commandLogs(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, printSection, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const limit = parsed.options.limit || "20";

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  // M24_001: activity is now per-zombie — require --zombie <id>.
  const zombieId = parsed.options.zombie || parsed.positionals[0];
  if (!zombieId) {
    writeError(ctx, "MISSING_ARGUMENT", "logs requires --zombie <id>", deps);
    return 2;
  }
  let url = `${wsZombieActivityPath(wsId, zombieId)}?limit=${encodeURIComponent(limit)}`;
  if (parsed.options.cursor) {
    url += `&cursor=${encodeURIComponent(parsed.options.cursor)}`;
  }

  const res = await request(ctx, url, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const events = res.items ?? [];
  if (events.length === 0) {
    writeLine(ctx.stdout, ui.info("No activity yet."));
    return 0;
  }

  printSection(ctx.stdout, "Activity Stream");
  for (const evt of events) {
    const ts = evt.created_at ? new Date(evt.created_at).toISOString() : "—";
    writeLine(ctx.stdout, `  ${ui.dim(ts)}  ${evt.event_type}  ${evt.detail || ""}`);
  }

  if (res.cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl logs --cursor=${res.cursor}`));
  }

  return 0;
}

// commandCredential extracted to ./zombie_credential.js (M26_001, RULE FLL).

// ── helpers ──────────────────────────────────────────────────────────────

// M2: only bundled template names are searched.
// M3 will add support for arbitrary directory names via TRIGGER.md discovery.
function findZombieDir(dir) {
  for (const name of BUNDLED_TEMPLATES) {
    const candidate = join(dir, name);
    if (existsSync(join(candidate, SKILL_FILENAME)) && existsSync(join(candidate, TRIGGER_FILENAME))) {
      return candidate;
    }
  }
  return null;
}
