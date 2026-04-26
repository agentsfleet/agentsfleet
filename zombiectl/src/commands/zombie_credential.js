// Zombie credential CLI commands (extracted from zombie.js to keep it
// under the 350-line file gate).
//
// zombiectl credential add    <name> --data='<json-object>'
// zombiectl credential list
// zombiectl credential delete <name>
//
// Credentials are workspace-scoped opaque JSON objects. The skill that
// consumes the secret addresses fields as ${secrets.<name>.<field>}; this
// CLI does not enforce a schema — that's the consumer's contract.

import { wsCredentialsPath, wsCredentialPath } from "../lib/api-paths.js";

function ensureWorkspace(ctx, workspaces, deps) {
  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    deps.writeError(
      ctx,
      "NO_WORKSPACE",
      "no workspace selected. Run: zombiectl workspace add",
      deps,
    );
  }
  return wsId;
}

function parseDataObject(raw) {
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    return { error: `--data is not valid JSON: ${err.message}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { error: "--data must be a JSON object (not a string, array, or scalar)" };
  }
  if (Object.keys(parsed).length === 0) {
    return { error: "--data must be a non-empty JSON object — at least one field is required" };
  }
  return { value: parsed };
}

async function addCredential(ctx, args, wsId, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const credName = args[1];
  if (!credName) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl credential add <name> --data='<json-object>'", deps);
    return 2;
  }

  const parsed = parseFlags(args.slice(2));
  const raw = parsed.options.data;
  if (!raw) {
    writeError(
      ctx,
      "MISSING_ARGUMENT",
      "missing --data flag. Usage: zombiectl credential add <name> --data='{\"host\":\"...\",\"api_token\":\"...\"}'",
      deps,
    );
    return 2;
  }

  const validated = parseDataObject(raw);
  if (validated.error) {
    writeError(ctx, "INVALID_ARGUMENT", validated.error, deps);
    return 2;
  }

  await request(ctx, wsCredentialsPath(wsId), {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ name: credName, data: validated.value }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { status: "stored", name: credName });
  } else {
    writeLine(ctx.stdout, ui.ok(`Credential '${credName}' stored in vault.`));
  }
  return 0;
}

async function listCredentials(ctx, wsId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;
  const res = await request(ctx, wsCredentialsPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }
  const creds = res.credentials ?? [];
  if (creds.length === 0) {
    writeLine(ctx.stdout, ui.info("No credentials stored. Add one with: zombiectl credential add <name> --data='{...}'"));
  } else {
    for (const c of creds) {
      writeLine(ctx.stdout, `  ${c.name}  ${ui.dim(c.created_at || "")}`);
    }
  }
  return 0;
}

async function deleteCredential(ctx, args, wsId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const credName = args[1];
  if (!credName) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl credential delete <name>", deps);
    return 2;
  }
  await request(ctx, wsCredentialPath(wsId, credName), {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { status: "deleted", name: credName });
  } else {
    writeLine(ctx.stdout, ui.ok(`Credential '${credName}' removed from vault.`));
  }
  return 0;
}

function unknownAction(ctx, action, deps) {
  const { ui, writeLine, writeError } = deps;
  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown credential action: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err(`unknown credential action: ${action ?? "(none)"}`));
    writeLine(ctx.stderr, "usage: zombiectl credential add <name> --data='<json-object>'");
    writeLine(ctx.stderr, "       zombiectl credential list");
    writeLine(ctx.stderr, "       zombiectl credential delete <name>");
  }
  return 2;
}

export async function commandCredential(ctx, args, workspaces, deps) {
  const action = args[0];
  const wsId = ensureWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  switch (action) {
    case "add":
      return addCredential(ctx, args, wsId, deps);
    case "list":
      return listCredentials(ctx, wsId, deps);
    case "delete":
      return deleteCredential(ctx, args, wsId, deps);
    default:
      return unknownAction(ctx, action, deps);
  }
}
