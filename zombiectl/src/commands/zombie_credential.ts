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
// Default upsert: skip-if-exists. `--force` to overwrite. The backing
// endpoint upserts on (workspace_id, key_name); the client-side guard
// prevents re-runs from silently clobbering a shared secret.

import { wsCredentialsPath, wsCredentialPath } from "../lib/api-paths.ts";
import {
  MISSING_ARGUMENT,
  INVALID_ARGUMENT,
  NO_WORKSPACE,
} from "../constants/cli-errors.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

interface CredentialRow {
  name?: string;
  created_at?: string | number | null;
  [key: string]: unknown;
}

interface CredentialsListResponse {
  credentials?: CredentialRow[];
}

type ParsedData =
  | { value: Record<string, unknown>; error?: undefined }
  | { error: string; value?: undefined };

function requireWorkspace(
  ctx: CommandCtx,
  workspaces: Workspaces,
  deps: CommandDeps,
): string | null | undefined {
  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    deps.writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
  }
  return wsId;
}

function parseDataObject(raw: string): ParsedData {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { error: `--data is not valid JSON: ${message}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { error: "--data must be a JSON object (not a string, array, or scalar)" };
  }
  const obj = parsed as Record<string, unknown>;
  if (Object.keys(obj).length === 0) {
    return { error: "--data must be a non-empty JSON object — at least one field is required" };
  }
  return { value: obj };
}

// Read all of stdin as UTF-8 — used when --data is `@-` so secret JSON
// never appears in shell history or process argv.
async function readStdinJson(ctx: CommandCtx): Promise<string> {
  if (typeof ctx.stdin === "string") return ctx.stdin;
  if (
    ctx.stdin &&
    typeof (ctx.stdin as AsyncIterable<unknown>)[Symbol.asyncIterator] ===
      "function"
  ) {
    const chunks: unknown[] = [];
    for await (const chunk of ctx.stdin as AsyncIterable<unknown>) {
      chunks.push(chunk);
    }
    return chunks
      .map((c) =>
        typeof c === "string"
          ? c
          : c instanceof Uint8Array
            ? new TextDecoder().decode(c)
            : String(c),
      )
      .join("");
  }
  const bunRef = (globalThis as { Bun?: { stdin?: { text?: () => Promise<string> } } }).Bun;
  if (typeof bunRef?.stdin?.text === "function") {
    return await bunRef.stdin.text();
  }
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function findCredentialByName(
  ctx: CommandCtx,
  wsId: string,
  name: string,
  deps: CommandDeps,
): Promise<CredentialRow | null> {
  const { request, apiHeaders } = deps;
  const res = (await request(ctx, wsCredentialsPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as CredentialsListResponse | null;
  const list = Array.isArray(res?.credentials) ? res.credentials : [];
  return list.find((c) => c.name === name) ?? null;
}

export async function commandCredentialAdd(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const credName = parsed.positionals[0];
  if (!credName) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl credential add <name> --data='<json-object>' [--force]", deps);
    return 2;
  }

  const force = Boolean(parsed.options["force"]);
  const dataFlag = parsed.options["data"];
  if (!dataFlag || typeof dataFlag !== "string") {
    writeError(
      ctx,
      MISSING_ARGUMENT,
      "missing --data flag. Pipe JSON on stdin with --data=@- or pass --data='{...}'. Stdin form keeps secrets out of shell history.",
      deps,
    );
    return 2;
  }

  let raw: string;
  if (dataFlag === "@-") {
    raw = await readStdinJson(ctx);
    if (!raw || raw.trim().length === 0) {
      writeError(ctx, INVALID_ARGUMENT, "--data=@- but stdin was empty", deps);
      return 2;
    }
  } else {
    raw = dataFlag;
  }

  const validated = parseDataObject(raw);
  if (validated.error || !validated.value) {
    writeError(ctx, INVALID_ARGUMENT, validated.error ?? "invalid --data", deps);
    return 2;
  }

  if (!force) {
    const existing = await findCredentialByName(ctx, wsId, credName, deps);
    if (existing) {
      if (ctx.jsonMode && ctx.stdout) {
        printJson(ctx.stdout, { status: "skipped", name: credName, reason: "already_exists" });
      } else if (ctx.stdout) {
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

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, { status: force ? "overwritten" : "stored", name: credName });
  } else if (ctx.stdout) {
    writeLine(ctx.stdout, ui.ok(`Credential '${credName}' ${force ? "overwritten" : "stored"} in vault.`));
  }
  return 0;
}

export async function commandCredentialShow(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { ui, printJson, writeLine, writeError } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const credName = parsed.positionals[0];
  if (!credName) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl credential show <name>", deps);
    return 2;
  }

  const found = await findCredentialByName(ctx, wsId, credName, deps);
  if (!found) {
    if (ctx.jsonMode && ctx.stdout) {
      printJson(ctx.stdout, { name: credName, exists: false });
    } else if (ctx.stderr) {
      writeLine(ctx.stderr, ui.err(`Credential '${credName}' not found in vault.`));
    }
    return 1;
  }

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, { name: found.name, exists: true, created_at: found.created_at ?? null });
  } else if (ctx.stdout) {
    writeLine(ctx.stdout, ui.ok(`Credential '${found.name}' exists.`));
    if (found.created_at) writeLine(ctx.stdout, ui.dim(`  created_at: ${found.created_at}`));
  }
  return 0;
}

export async function commandCredentialList(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const res = (await request(ctx, wsCredentialsPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as CredentialsListResponse | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }
  if (!ctx.stdout) return 0;
  const creds = res?.credentials ?? [];
  if (creds.length === 0) {
    writeLine(ctx.stdout, ui.info("No credentials stored. Add one with: zombiectl credential add <name> --data=@- (pipe JSON on stdin)"));
  } else {
    for (const c of creds) {
      writeLine(ctx.stdout, `  ${c.name ?? ""}  ${ui.dim(String(c.created_at || ""))}`);
    }
  }
  return 0;
}

export async function commandCredentialDelete(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const credName = parsed.positionals[0];
  if (!credName) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl credential delete <name>", deps);
    return 2;
  }
  await request(ctx, wsCredentialPath(wsId, credName), {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, { status: "deleted", name: credName });
  } else if (ctx.stdout) {
    writeLine(ctx.stdout, ui.ok(`Credential '${credName}' removed from vault.`));
  }
  return 0;
}
