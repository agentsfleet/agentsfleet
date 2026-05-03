// Zombie credential CLI commands (extracted from zombie.js to keep it
// under the 350-line file gate).
//
// zombiectl credential add    <name> --data='<json-object>'   (or --data=@- for stdin)
// zombiectl credential add    <name> --data=@- --force        (overwrite existing)
// zombiectl credential show   <name>                          (existence + created_at; never secret bytes)
// zombiectl credential list
// zombiectl credential delete <name>
//
// Credentials are workspace-scoped opaque JSON objects. The skill that
// consumes the secret addresses fields as ${secrets.<name>.<field>}; this
// CLI does not enforce a schema — that's the consumer's contract.
//
// Default upsert: skip-if-exists. If a credential with `<name>` already
// exists, `add` returns 0 with status="skipped" and does NOT mutate the
// vault. Pass `--force` to overwrite. The backing endpoint upserts on
// (workspace_id, key_name); the CLI's skip-if-exists is a client-side
// guard so installation flows can re-run without clobbering a
// workspace-shared secret like `github.webhook_secret`.

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

// Read all of stdin as UTF-8. Used when --data is `@-` so secret JSON
// never appears in shell history or process argv. Returns the raw body
// (caller pipes through parseDataObject).
async function readStdinJson(ctx) {
  // Allow tests to inject a fake stdin via ctx.stdin (string or async iterable).
  if (typeof ctx?.stdin === "string") return ctx.stdin;
  if (ctx?.stdin && typeof ctx.stdin[Symbol.asyncIterator] === "function") {
    const chunks = [];
    for await (const chunk of ctx.stdin) chunks.push(chunk);
    return chunks.map((c) => (typeof c === "string" ? c : new TextDecoder().decode(c))).join("");
  }
  // Bun runtime path.
  if (typeof globalThis.Bun?.stdin?.text === "function") {
    return await globalThis.Bun.stdin.text();
  }
  // Node fallback.
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

async function findCredentialByName(ctx, wsId, name, deps) {
  const { request, apiHeaders } = deps;
  const res = await request(ctx, wsCredentialsPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  });
  const list = Array.isArray(res?.credentials) ? res.credentials : [];
  return list.find((c) => c.name === name) || null;
}

async function addCredential(ctx, args, wsId, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const credName = args[1];
  if (!credName) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl credential add <name> --data='<json-object>' [--force]", deps);
    return 2;
  }

  const parsed = parseFlags(args.slice(2));
  const force = Boolean(parsed.options.force);
  const dataFlag = parsed.options.data;
  if (!dataFlag) {
    writeError(
      ctx,
      "MISSING_ARGUMENT",
      "missing --data flag. Pipe JSON on stdin with --data=@- or pass --data='{...}'. Stdin form keeps secrets out of shell history.",
      deps,
    );
    return 2;
  }

  let raw;
  if (dataFlag === "@-") {
    raw = await readStdinJson(ctx);
    if (!raw || raw.trim().length === 0) {
      writeError(ctx, "INVALID_ARGUMENT", "--data=@- but stdin was empty", deps);
      return 2;
    }
  } else {
    raw = dataFlag;
  }

  const validated = parseDataObject(raw);
  if (validated.error) {
    writeError(ctx, "INVALID_ARGUMENT", validated.error, deps);
    return 2;
  }

  // Default skip-if-exists. The backend upserts on (workspace_id, key_name);
  // the client-side guard prevents re-runs from silently clobbering a shared
  // secret (e.g. github.webhook_secret across multiple zombies in a workspace).
  if (!force) {
    const existing = await findCredentialByName(ctx, wsId, credName, deps);
    if (existing) {
      if (ctx.jsonMode) {
        printJson(ctx.stdout, { status: "skipped", name: credName, reason: "already_exists" });
      } else {
        writeLine(ctx.stdout, ui.info(`Credential '${credName}' already exists — skipped. Pass --force to overwrite.`));
      }
      return 0;
    }
  }

  await request(ctx, wsCredentialsPath(wsId), {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ name: credName, data: validated.value }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { status: force ? "overwritten" : "stored", name: credName });
  } else {
    writeLine(ctx.stdout, ui.ok(`Credential '${credName}' ${force ? "overwritten" : "stored"} in vault.`));
  }
  return 0;
}

async function showCredential(ctx, args, wsId, deps) {
  const { ui, printJson, writeLine, writeError } = deps;
  const credName = args[1];
  if (!credName) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl credential show <name>", deps);
    return 2;
  }

  const found = await findCredentialByName(ctx, wsId, credName, deps);
  if (!found) {
    if (ctx.jsonMode) {
      printJson(ctx.stdout, { name: credName, exists: false });
    } else {
      writeLine(ctx.stderr, ui.err(`Credential '${credName}' not found in vault.`));
    }
    return 1;
  }

  // Never echo secret bytes — show only existence + metadata. Field-level
  // presence requires a backend GET which doesn't yet exist; M49 only needs
  // the existence check (skill prompts reuse-vs-scope on second install
  // when the github credential is present at all).
  if (ctx.jsonMode) {
    printJson(ctx.stdout, { name: found.name, exists: true, created_at: found.created_at ?? null });
  } else {
    writeLine(ctx.stdout, ui.ok(`Credential '${found.name}' exists.`));
    if (found.created_at) writeLine(ctx.stdout, ui.dim(`  created_at: ${found.created_at}`));
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
    writeLine(ctx.stdout, ui.info("No credentials stored. Add one with: zombiectl credential add <name> --data=@- (pipe JSON on stdin)"));
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
    writeLine(ctx.stderr, "usage: zombiectl credential add    <name> --data=@- [--force]   (pipe JSON on stdin)");
    writeLine(ctx.stderr, "       zombiectl credential show   <name>");
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
    case "show":
      return showCredential(ctx, args, wsId, deps);
    case "list":
      return listCredentials(ctx, wsId, deps);
    case "delete":
      return deleteCredential(ctx, args, wsId, deps);
    default:
      return unknownAction(ctx, action, deps);
  }
}
