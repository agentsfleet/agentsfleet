// Zombie CLI commands.
//
// Flat top-level for common ops: install, status, kill, logs.
// Namespaced for less common: credential add, credential list.

import {
  wsZombiesPath,
  wsZombieKillPath,
  wsZombieEventsPath,
} from "../lib/api-paths.js";
import {
  loadSkillFromPath,
  SkillLoadError,
} from "../lib/load-skill-from-path.js";
import { commandCredential } from "./zombie_credential.js";
import { commandList } from "./zombie_list.js";
import { commandEvents } from "./zombie_events.js";
import { commandSteer } from "./zombie_steer.js";

export async function commandZombie(ctx, args, workspaces, deps) {
  const action = args[0];
  const { ui, writeLine, writeError } = deps;

  if (action === "install") return commandInstall(ctx, args.slice(1), workspaces, deps);
  if (action === "list") return commandList(ctx, args.slice(1), workspaces, deps);
  if (action === "status") return commandStatus(ctx, args.slice(1), workspaces, deps);
  if (action === "kill") return commandKill(ctx, args.slice(1), workspaces, deps);
  if (action === "logs") return commandLogs(ctx, args.slice(1), workspaces, deps);
  if (action === "events") return commandEvents(ctx, args.slice(1), workspaces, deps);
  if (action === "steer") return commandSteer(ctx, args.slice(1), workspaces, deps);
  if (action === "credential") return commandCredential(ctx, args.slice(1), workspaces, deps);

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown zombie subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err(`unknown zombie subcommand: ${action ?? "(none)"}`));
    writeLine(ctx.stderr);
    writeLine(ctx.stderr, "usage: zombiectl install --from <path>");
    writeLine(ctx.stderr, "       zombiectl status");
    writeLine(ctx.stderr, "       zombiectl kill");
    writeLine(ctx.stderr, "       zombiectl logs");
    writeLine(ctx.stderr, "       zombiectl steer <id> \"<msg>\"");
    writeLine(ctx.stderr, "       zombiectl events <id> [--actor=glob] [--since=2h]");
    writeLine(ctx.stderr, "       zombiectl credential add|list|delete");
  }
  return 2;
}

// ── install ──────────────────────────────────────────────────────────────

async function commandInstall(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, printJson, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const fromPath = parsed.options.from;

  if (!fromPath || typeof fromPath !== "string") {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl install --from <path>", deps);
    return 2;
  }
  if (parsed.positionals.length > 0) {
    writeError(
      ctx,
      "UNKNOWN_ARGUMENT",
      `unexpected argument: ${parsed.positionals[0]}. usage: zombiectl install --from <path>`,
      deps,
    );
    return 2;
  }

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  let bundle;
  try {
    bundle = loadSkillFromPath(fromPath);
  } catch (err) {
    if (err instanceof SkillLoadError) {
      writeError(ctx, err.code, `${err.code}: ${err.message}`, deps);
      return 1;
    }
    throw err;
  }

  let res;
  try {
    res = await request(ctx, wsZombiesPath(wsId), {
      method: "POST",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      }),
    });
  } catch (err) {
    // Non-ApiError network failures (ECONNREFUSED, DNS, socket close) land here.
    // ApiErrors (409/5xx/timeout) get re-thrown so cli.js's printApiError renders
    // them with the code + request_id the server returned.
    if (err && err.name === "ApiError") throw err;
    writeError(ctx, "IO_ERROR", `IO_ERROR: ${err?.message ?? String(err)}`, deps);
    return 1;
  }

  // Server is the source of truth for the resolved name (parsed from
  // TRIGGER.md frontmatter). Fall back to the directory hint only when the
  // response omits it.
  const displayName = res.name || bundle.fallback_name;

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      status: "installed",
      zombie_id: res.zombie_id,
      webhook_url: res.webhook_url,
      name: displayName,
    });
    return 0;
  }

  writeLine(ctx.stdout, `🎉 ${displayName} is live.`);
  if (res.zombie_id) {
    writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
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
    writeLine(ctx.stdout, ui.info("No zombies running. Install one with: zombiectl install --from <path>"));
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
  const zombieId = parsed.positionals[0];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  if (!zombieId) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl kill <zombie_id>", deps);
    return 2;
  }

  const res = await request(ctx, wsZombieKillPath(wsId, zombieId), {
    method: "POST",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`${zombieId} killed.`));
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

  const zombieId = parsed.options.zombie || parsed.positionals[0];
  if (!zombieId) {
    writeError(ctx, "MISSING_ARGUMENT", "logs requires --zombie <id>", deps);
    return 2;
  }
  let url = `${wsZombieEventsPath(wsId, zombieId)}?limit=${encodeURIComponent(limit)}`;
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
    writeLine(ctx.stdout, ui.info("No events yet."));
    return 0;
  }

  // The events endpoint replaced the activity stream in M42; row shape now
  // carries actor/status/response_text instead of event_type/detail. Render
  // the new shape verbatim — `zombiectl events` is the richer surface.
  printSection(ctx.stdout, "Event Stream");
  for (const evt of events) {
    const ts = evt.created_at ? new Date(evt.created_at).toISOString() : "—";
    const summary = evt.response_text ? evt.response_text.slice(0, 80) : (evt.status ?? "");
    writeLine(ctx.stdout, `  ${ui.dim(ts)}  ${evt.actor}  ${summary}`);
  }

  if (res.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl logs --cursor=${res.next_cursor}`));
  }

  return 0;
}

// commandCredential extracted to ./zombie_credential.js for the 350-line file gate.
