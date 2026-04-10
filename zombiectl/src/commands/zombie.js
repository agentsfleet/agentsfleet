// M1_001 §5.0 — Zombie CLI commands.
//
// Flat top-level for common ops: install, up, status, kill, logs.
// Namespaced for less common: credential add, credential list.

import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { ZOMBIES_PATH } from "../lib/api-paths.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEMPLATES_DIR = join(__dirname, "../../templates");

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

  const templatePath = join(TEMPLATES_DIR, `${templateName}.md`);
  let templateContent;
  try {
    templateContent = readFileSync(templatePath, "utf-8");
  } catch {
    writeError(ctx, "TEMPLATE_NOT_FOUND", `template file not found: ${templateName}.md`, deps);
    return 1;
  }

  const outputDir = process.cwd();
  const outputPath = join(outputDir, `${templateName}.md`);

  try {
    writeFileSync(outputPath, templateContent, "utf-8");
  } catch (err) {
    writeError(ctx, "IO_ERROR", `failed to write ${templateName}.md: ${err.message}`, deps);
    return 1;
  }

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      status: "installed",
      template: templateName,
      path: outputPath,
    });
  } else {
    writeLine(ctx.stdout, ui.ok(`${templateName} installed.`));
    writeLine(ctx.stdout, `  Config: ${outputPath}`);
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

  // Find the zombie config in the current directory
  const configPath = findZombieConfig(process.cwd());
  if (!configPath) {
    if (ctx.jsonMode) {
      writeError(ctx, "NO_CONFIG", "no zombie config found in current directory. Run: zombiectl install <template>", deps);
    } else {
      writeLine(ctx.stderr, ui.err("No zombie config found in current directory."));
      writeLine(ctx.stderr, "Run: zombiectl install <template>");
    }
    return 1;
  }

  let configContent;
  try {
    configContent = readFileSync(configPath, "utf-8");
  } catch (err) {
    writeError(ctx, "IO_ERROR", `failed to read config: ${err.message}`, deps);
    return 1;
  }

  // Parse YAML frontmatter to JSON (simple extraction)
  const config = parseZombieMarkdown(configContent);
  if (!config) {
    writeError(ctx, "INVALID_CONFIG", "failed to parse zombie config. Check YAML frontmatter format.", deps);
    return 1;
  }

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  // Deploy to UseZombie cloud
  const res = await request(ctx, ZOMBIES_PATH, {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({
      workspace_id: wsId,
      name: config.name,
      source_markdown: configContent,
      config_json: config,
    }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`${config.name} is live.`));
    if (res.zombie_id) {
      writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
    }
    if (config.trigger?.source === "agentmail") {
      writeLine(ctx.stdout, `  Send a test email to see it work.`);
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

function findZombieConfig(dir) {
  for (const name of BUNDLED_TEMPLATES) {
    const path = join(dir, `${name}.md`);
    try {
      readFileSync(path, "utf-8");
      return path;
    } catch {
      continue;
    }
  }
  return null;
}

function parseZombieMarkdown(content) {
  // Extract YAML frontmatter between --- delimiters
  const trimmed = content.trim();
  if (!trimmed.startsWith("---")) return null;

  const endIdx = trimmed.indexOf("\n---", 3);
  if (endIdx === -1) return null;

  const yamlBlock = trimmed.slice(3, endIdx).trim();

  // Simple YAML-to-JSON parser for flat/nested config.
  // Full YAML parsing deferred to a proper library when needed.
  try {
    return simpleYamlParse(yamlBlock);
  } catch {
    return null;
  }
}

function simpleYamlParse(yaml) {
  const result = {};
  let currentKey = null;
  let arrayKey = null;

  for (const rawLine of yaml.split("\n")) {
    const line = rawLine.trimEnd();
    if (line.trim() === "" || line.trim().startsWith("#")) continue;

    // Array item: "  - value"
    if (/^\s+-\s+/.test(line)) {
      const value = line.replace(/^\s+-\s+/, "").trim();
      if (currentKey && arrayKey && result[currentKey] && Array.isArray(result[currentKey][arrayKey])) {
        result[currentKey][arrayKey].push(value);
      } else if (arrayKey && Array.isArray(result[arrayKey])) {
        result[arrayKey].push(value);
      }
      continue;
    }

    // Nested key: "  key: value"
    if (/^\s+\w/.test(line) && currentKey) {
      const match = line.match(/^\s+(\w[\w_]*)\s*:\s*(.*)$/);
      if (match) {
        const [, k, v] = match;
        if (!result[currentKey] || Array.isArray(result[currentKey])) result[currentKey] = {};
        if (v.trim() === "") {
          arrayKey = k;
          result[currentKey][k] = [];
        } else {
          result[currentKey][k] = parseYamlValue(v.trim());
          arrayKey = null;
        }
      }
      continue;
    }

    // Top-level key: "key: value"
    const topMatch = line.match(/^(\w[\w_]*)\s*:\s*(.*)$/);
    if (topMatch) {
      const [, k, v] = topMatch;
      if (v.trim() === "") {
        currentKey = k;
        result[k] = [];
        arrayKey = k;
      } else {
        result[k] = parseYamlValue(v.trim());
        currentKey = null;
      }
      continue;
    }
  }

  return result;
}

function parseYamlValue(v) {
  if (v === "true") return true;
  if (v === "false") return false;
  const num = Number(v);
  if (!isNaN(num) && v !== "") return num;
  return v;
}
