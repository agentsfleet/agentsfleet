// M1_001 §5.0 — Zombie credential CLI commands (extracted from zombie.js in M26_001
// to keep zombie.js under the RULE FLL 350-line gate).
//
// zombiectl credential add  <name> --value=<value>
// zombiectl credential list

import { wsCredentialsPath } from "../lib/api-paths.js";

export async function commandCredential(ctx, args, workspaces, deps) {
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

    const parsed = parseFlags(args.slice(2));
    let credValue = parsed.options.value;

    if (!credValue) {
      if (ctx.noInput) {
        writeError(ctx, "NO_INPUT", "credential value required. Use: zombiectl credential add <name> --value=<value>", deps);
        return 1;
      }
      writeError(ctx, "NO_INPUT", "interactive credential prompt not yet implemented. Use: zombiectl credential add <name> --value=<value>", deps);
      return 1;
    }

    await request(ctx, wsCredentialsPath(wsId), {
      method: "POST",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({
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
    const res = await request(ctx, wsCredentialsPath(wsId), {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      const creds = res.credentials ?? [];
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
