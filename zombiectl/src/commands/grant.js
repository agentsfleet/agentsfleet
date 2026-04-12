// M9_001 §6.0 — Integration Grant CLI commands.
//
// zombiectl grant list   --zombie <id>              → list grants for a zombie
// zombiectl grant revoke --zombie <id> <grant_id>   → revoke a grant immediately

import { ZOMBIES_PATH } from "../lib/api-paths.js";
import { writeError } from "../program/io.js";

export async function commandGrant(ctx, args, _workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const action = args[0];
  const parsed = parseFlags(args.slice(1));

  if (action === "list") return commandGrantList(ctx, parsed, { request, apiHeaders, ui, printJson, printTable, writeLine });
  if (action === "revoke") return commandGrantRevoke(ctx, parsed, { request, apiHeaders, ui, printJson, writeLine, writeError, deps });

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown grant subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl grant list   --zombie <id>"));
    writeLine(ctx.stderr, ui.err("       zombiectl grant revoke --zombie <id> <grant_id>"));
  }
  return 2;
}

// ── grant list ───────────────────────────────────────────────────────────────

async function commandGrantList(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const zombieId = parsed.options["zombie"] || parsed.positionals[0];
  if (!zombieId) {
    writeLine(ctx.stderr, ui.err("grant list requires --zombie <id>"));
    return 2;
  }

  const url = `${ZOMBIES_PATH}${encodeURIComponent(zombieId)}/integration-grants`;
  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });
  const grants = Array.isArray(res.grants) ? res.grants : [];

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (grants.length === 0) {
    writeLine(ctx.stdout, ui.info("no integration grants found"));
    return 0;
  }

  printTable(ctx.stdout, [
    { key: "service",      label: "SERVICE" },
    { key: "status",       label: "STATUS" },
    { key: "requested_at", label: "REQUESTED_AT" },
    { key: "approved_at",  label: "APPROVED_AT" },
    { key: "grant_id",     label: "GRANT_ID" },
  ], grants.map((g) => ({
    ...g,
    requested_at: g.requested_at ? new Date(g.requested_at).toISOString() : "-",
    approved_at:  g.approved_at  ? new Date(g.approved_at).toISOString()  : "-",
  })));
  return 0;
}

// ── grant revoke ─────────────────────────────────────────────────────────────

async function commandGrantRevoke(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;

  const zombieId = parsed.options["zombie"];
  const grantId = parsed.positionals[0];

  if (!zombieId || !grantId) {
    writeLine(ctx.stderr, ui.err("grant revoke requires --zombie <id> <grant_id>"));
    return 2;
  }

  const url = `${ZOMBIES_PATH}${encodeURIComponent(zombieId)}/integration-grants/${encodeURIComponent(grantId)}`;
  await request(ctx, url, { method: "DELETE", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { revoked: true, grant_id: grantId });
  } else {
    writeLine(ctx.stdout, ui.ok(`Grant ${grantId} revoked. Zombie will receive UZ-GRANT-003 on next execute attempt.`));
  }
  return 0;
}
