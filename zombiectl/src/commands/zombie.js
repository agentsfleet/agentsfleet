// Zombie CLI commands.
//
// Flat top-level for common ops: install, status, kill, logs.
// Namespaced for less common: credential add, credential list.

import {
  wsZombiesPath,
  wsZombiePath,
} from "../lib/api-paths.js";
import {
  loadSkillFromPath,
  SkillLoadError,
} from "../lib/load-skill-from-path.js";
import { validateRequiredId } from "../program/validate.js";
import {
  IO_ERROR,
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  UNKNOWN_ARGUMENT,
  UNKNOWN_COMMAND,
  VALIDATION_ERROR,
} from "../constants/cli-errors.js";
import {
  ACTION_CREDENTIAL,
  ACTION_DELETE,
  ACTION_EVENTS,
  ACTION_INSTALL,
  ACTION_KILL,
  ACTION_LIST,
  ACTION_LOGS,
  ACTION_RESUME,
  ACTION_STATUS,
  ACTION_STEER,
  ACTION_STOP,
} from "../constants/cli-actions.js";
import { OPT_FROM } from "../constants/cli-flags.js";
import {
  ERR_CREDENTIAL_NAME_INVALID,
  ERR_CREDENTIAL_NOT_FOUND,
  ERR_VAULT_INVALID,
  ERR_ZOMBIE_RUNNER_FAILED,
} from "../constants/error-codes.js";
import { commandCredential } from "./zombie_credential.js";
import { commandList } from "./zombie_list.js";
import { commandLogs } from "./zombie_logs.js";
import { commandEvents } from "./zombie_events.js";
import { commandSteer } from "./zombie_steer.js";
import {
  AUTH_PRESET,
  WORKSPACE_PRESET,
  ZOMBIE_PRESET,
  compose,
} from "../lib/error-map-presets.js";

// Single map shared by every `zombie.*` route. The dispatcher in
// commandZombie routes to install/list/status/kill/stop/resume/delete/
// logs/steer/events/credential — all hit the same workspace + zombie
// auth path, so the union map is the right grain. Vault and execution
// codes go in here too because credential and events surface them.
export const errorMap = compose(AUTH_PRESET, WORKSPACE_PRESET, ZOMBIE_PRESET, {
  [ERR_VAULT_INVALID]: {
    code: "CREDENTIAL_INVALID",
    message: "Credential JSON is invalid — must be a non-empty object ≤ 4 KiB.",
  },
  [ERR_CREDENTIAL_NOT_FOUND]: {
    code: "CREDENTIAL_NOT_FOUND",
    message: "Credential not found in this workspace.",
  },
  [ERR_CREDENTIAL_NAME_INVALID]: {
    code: "CREDENTIAL_NAME_INVALID",
    message: "Credential name is invalid — use lowercase letters, digits, and dashes.",
  },
  [ERR_ZOMBIE_RUNNER_FAILED]: {
    code: "ZOMBIE_RUNNER_FAILED",
    message: "Zombie runner exited with an error — see `zombiectl logs <zombie_id>` for details.",
  },
});

export async function commandZombie(ctx, args, workspaces, deps) {
  const action = args[0];
  const { ui, writeLine, writeError } = deps;

  if (action === ACTION_INSTALL) return commandInstall(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_LIST) return commandList(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_STATUS) return commandStatus(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_STOP) return commandSetStatus(ctx, args.slice(1), workspaces, deps, "stopped");
  if (action === ACTION_RESUME) return commandSetStatus(ctx, args.slice(1), workspaces, deps, "active");
  if (action === ACTION_KILL) return commandSetStatus(ctx, args.slice(1), workspaces, deps, "killed");
  if (action === ACTION_DELETE) return commandDelete(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_LOGS) return commandLogs(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_EVENTS) return commandEvents(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_STEER) return commandSteer(ctx, args.slice(1), workspaces, deps);
  if (action === ACTION_CREDENTIAL) return commandCredential(ctx, args.slice(1), workspaces, deps);

  if (ctx.jsonMode) {
    writeError(ctx, UNKNOWN_COMMAND, `unknown zombie subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err(`unknown zombie subcommand: ${action ?? "(none)"}`));
    writeLine(ctx.stderr);
    writeLine(ctx.stderr, "usage: zombiectl install --from <path>");
    writeLine(ctx.stderr, "       zombiectl status");
    writeLine(ctx.stderr, "       zombiectl stop <id>     # halt the running session (resumable)");
    writeLine(ctx.stderr, "       zombiectl resume <id>   # resume from stopped or auto-paused");
    writeLine(ctx.stderr, "       zombiectl kill <id>     # mark terminal (irreversible)");
    writeLine(ctx.stderr, "       zombiectl delete <id>   # hard-purge (must kill first)");
    writeLine(ctx.stderr, "       zombiectl logs");
    writeLine(ctx.stderr, "       zombiectl steer <id> \"<msg>\"");
    writeLine(ctx.stderr, "       zombiectl events <id> [--actor=glob] [--since=2h]");
    writeLine(ctx.stderr, "       zombiectl credential add|list|delete");
  }
  return 2;
}

// ── install ──────────────────────────────────────────────────────────────

async function commandInstall(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const fromPath = parsed.options[OPT_FROM];

  if (!fromPath || typeof fromPath !== "string") {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl install --from <path>", deps);
    return 2;
  }
  if (parsed.positionals.length > 0) {
    writeError(
      ctx,
      UNKNOWN_ARGUMENT,
      `unexpected argument: ${parsed.positionals[0]}. usage: zombiectl install --from <path>`,
      deps,
    );
    return 2;
  }

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
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
    writeError(ctx, IO_ERROR, `IO_ERROR: ${err?.message ?? String(err)}`, deps);
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

  writeLine(ctx.stdout, ui.ok(`${displayName} is live.`));
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
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
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

// ── status transitions: stop / resume / kill ──────────────────────────────
// Drives PATCH /v1/workspaces/{ws}/zombies/{id} {status: ...}.
//   stopped — halt the running session, resumable
//   active  — resume from stopped or auto-paused
//   killed  — terminal mark (irreversible)
// `paused` is gate-only and intentionally not exposed here.

const STATUS_PAST_TENSE = {
  stopped: "stopped",
  active: "resumed",
  killed: "killed",
};

const STATUS_VERB = {
  stopped: "stop",
  active: "resume",
  killed: "kill",
};

async function commandSetStatus(ctx, args, workspaces, deps, status) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const zombieId = parsed.positionals[0];
  const verb = STATUS_VERB[status] ?? "patch";

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, `usage: zombiectl ${verb} <zombie_id>`, deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, "zombie_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  const res = await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "PATCH",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ status }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`${zombieId} ${STATUS_PAST_TENSE[status]}.`));
  }
  return 0;
}

// ── delete (hard-purge) ───────────────────────────────────────────────────
// DELETE /v1/workspaces/{ws}/zombies/{id}. Must kill first; server returns
// 409 (UZ-ZMB-010) if the zombie isn't terminal yet.

async function commandDelete(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const zombieId = parsed.positionals[0];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl delete <zombie_id>", deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, "zombie_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { zombie_id: zombieId, deleted: true });
  } else {
    writeLine(ctx.stdout, ui.ok(`${zombieId} deleted.`));
  }
  return 0;
}

// commandLogs extracted to ./zombie_logs.js; commandCredential extracted to
// ./zombie_credential.js — both for the 350-line file gate.
