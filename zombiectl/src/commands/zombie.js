// M1_001 §5.0 — Zombie CLI commands.
//
// Flat top-level for common ops: install, up, status, kill, logs.
// Namespaced for less common: credential add, credential list.

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { ZOMBIES_PATH } from "../lib/api-paths.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEMPLATES_DIR = join(__dirname, "../../templates");

const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";

const BUNDLED_TEMPLATES = ["lead-collector"];

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
  const zombieName = nameMatch ? nameMatch[1].trim() : dirname(zombieDir).split("/").pop();

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  // Deploy to UseZombie cloud — server parses TRIGGER.md, no client-side YAML parsing
  const res = await request(ctx, ZOMBIES_PATH, {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({
      workspace_id: wsId,
      source_markdown: skillMd,
      trigger_markdown: triggerMd,
    }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`${zombieName} is live.`));
    if (res.zombie_id) {
      writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
    }
    if (res.webhook_url) {
      writeLine(ctx.stdout, `  Webhook URL: ${res.webhook_url}`);
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

  const res = await request(ctx, `${ZOMBIES_PATH}?workspace_id=${encodeURIComponent(wsId)}`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const zombies = res.zombies || res.data || [];
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

  // If no name given, kill all zombies in the workspace
  const endpoint = zombieName
    ? `${ZOMBIES_PATH}${encodeURIComponent(zombieName)}?workspace_id=${encodeURIComponent(wsId)}`
    : `${ZOMBIES_PATH}?workspace_id=${encodeURIComponent(wsId)}`;

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

  let url = `${ZOMBIES_PATH}activity?workspace_id=${encodeURIComponent(wsId)}&limit=${limit}`;
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

  const events = res.events || res.data || [];
  if (events.length === 0) {
    writeLine(ctx.stdout, ui.info("No activity yet."));
    return 0;
  }

  printSection(ctx.stdout, "Activity Stream");
  for (const evt of events) {
    const ts = evt.created_at ? new Date(evt.created_at).toISOString() : "—";
    writeLine(ctx.stdout, `  ${ui.dim(ts)}  ${evt.event_type}  ${evt.detail || ""}`);
  }

  if (res.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl logs --cursor=${res.next_cursor}`));
  }

  return 0;
}

// ── credential ───────────────────────────────────────────────────────────

async function commandCredential(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const action = args[0];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  if (action === "add") {
    const credName = args[1];
    if (!credName) {
      writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl credential add <name>", deps);
      return 2;
    }

    // Read credential value from stdin or --value flag
    const parsed = parseFlags(args.slice(2));
    let credValue = parsed.options.value;

    if (!credValue) {
      // Prompt for value (non-interactive mode returns error)
      if (ctx.noInput) {
        writeError(ctx, "NO_INPUT", "credential value required. Use: zombiectl credential add <name> --value=<value>", deps);
        return 1;
      }
      // In production, this would use readline. For now, require --value.
      writeError(ctx, "NO_INPUT", "interactive credential prompt not yet implemented. Use: zombiectl credential add <name> --value=<value>", deps);
      return 1;
    }

    await request(ctx, `${ZOMBIES_PATH}credentials`, {
      method: "POST",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({
        workspace_id: wsId,
        name: credName,
        value: credValue,
      }),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, { status: "stored", name: credName });
    } else {
      writeLine(ctx.stdout, ui.ok(`Credential '${credName}' stored in vault.`));
    }
    return 0;
  }

  if (action === "list") {
    const res = await request(ctx, `${ZOMBIES_PATH}credentials?workspace_id=${encodeURIComponent(wsId)}`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      const creds = res.credentials || res.data || [];
      if (creds.length === 0) {
        writeLine(ctx.stdout, ui.info("No credentials stored. Add one with: zombiectl credential add <name>"));
      } else {
        for (const c of creds) {
          writeLine(ctx.stdout, `  ${c.name}  ${ui.dim(c.created_at || "")}`);
        }
      }
    }
    return 0;
  }

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown credential action: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err(`unknown credential action: ${action ?? "(none)"}`));
    writeLine(ctx.stderr, "usage: zombiectl credential add <name> --value=<value>");
    writeLine(ctx.stderr, "       zombiectl credential list");
  }
  return 2;
}

// ── helpers ──────────────────────────────────────────────────────────────

function findZombieDir(dir) {
  for (const name of BUNDLED_TEMPLATES) {
    const candidate = join(dir, name);
    if (existsSync(join(candidate, SKILL_FILENAME)) && existsSync(join(candidate, TRIGGER_FILENAME))) {
      return candidate;
    }
  }
  return null;
}
