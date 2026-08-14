import fs from "node:fs/promises";
import path from "node:path";

import { resolveConfigDir } from "./config-dir.ts";

// On-disk state shapes. All files live under the directory `config-dir.ts`
// resolves from the caller-supplied environment, at mode 0o600. JSON is
// parsed permissively — missing files return the fallback, corrupt files
// raise. No function here reads the process environment: `runCli` resolves
// its io environment (falling back to the process one) exactly once and
// threads it down, so an injected environment reaches the store instead of
// silently losing to a global.
//
// Session identity (`device_id`, `session_id`, `session_last_active`)
// lives in `telemetry.json` under `src/services/telemetry/`, mirroring
// supabase. State here covers credentials + workspaces only.

export interface StatePaths {
  readonly baseDir: string;
  readonly credentialsPath: string;
  readonly workspacesPath: string;
}

// Every file under baseDir is owner-rw-only: credentials, workspaces.
// Single named const so the policy is enforced from one site.
const STATE_FILE_MODE = 0o600;

export interface Credentials {
  token: string | null;
  saved_at: number | null;
  session_id: string | null;
  api_url: string | null;
  // Server-side identifier of the credential in `token`, returned once at
  // mint. Kept so this terminal can revoke its own credential by name
  // without listing every credential the person owns. Null for a record
  // written before the credential exchange shipped, and for a directly
  // supplied tenant key, which this client never minted and cannot revoke.
  credential_id: string | null;
}

export interface WorkspaceItem {
  workspace_id: string;
  // Older persisted rows and list responses may carry a missing name.
  // Display callers tolerate this with `name ?? "—"`.
  name: string | null;
  created_at: number | null;
}

export interface Workspaces {
  readonly tenant_id?: string | null;
  current_workspace_id: string | null;
  items: WorkspaceItem[];
}

function resolveStatePaths(env: NodeJS.ProcessEnv): StatePaths {
  const baseDir = resolveConfigDir(env);
  return {
    baseDir,
    credentialsPath: path.join(baseDir, "credentials.json"),
    workspacesPath: path.join(baseDir, "workspaces.json"),
  };
}

// The logged-out record. Returned fresh on each call rather than shared, so a
// caller that mutates what it read cannot corrupt the next reader's fallback.
// One definition for all three sites — the read fallback, what `clear`
// writes, and the entry point's default — so a field added to `Credentials`
// cannot be forgotten at one of them.
export function emptyCredentials(): Credentials {
  return {
    token: null,
    saved_at: null,
    session_id: null,
    api_url: null,
    credential_id: null,
  };
}

// Same contract as `emptyCredentials`: one definition for every site that
// needs the signed-out workspaces record, so a field added to `Workspaces`
// cannot drift at one of them (the entry-point fallback had already lost
// `tenant_id` before this existed).
export function emptyWorkspaces(): Workspaces {
  return {
    tenant_id: null,
    current_workspace_id: null,
    items: [],
  };
}

async function ensureBaseDir(env: NodeJS.ProcessEnv): Promise<void> {
  const { baseDir } = resolveStatePaths(env);
  await fs.mkdir(baseDir, { recursive: true });
}

async function readJson<T>(filePath: string, fallback: T): Promise<T> {
  try {
    const raw = await fs.readFile(filePath, "utf8");
    return JSON.parse(raw) as T;
  } catch (err) {
    if (err !== null && typeof err === "object") {
      const e = err as { code?: unknown; name?: unknown };
      if (e.code === "ENOENT" || e.name === "SyntaxError") return fallback;
    }
    throw err;
  }
}

async function writeJson(
  env: NodeJS.ProcessEnv,
  filePath: string,
  value: unknown,
): Promise<void> {
  await ensureBaseDir(env);
  const body = `${JSON.stringify(value, null, 2)}\n`;
  await fs.writeFile(filePath, body, { mode: STATE_FILE_MODE });
}

export async function loadCredentials(env: NodeJS.ProcessEnv): Promise<Credentials> {
  const { credentialsPath } = resolveStatePaths(env);
  return readJson<Credentials>(credentialsPath, emptyCredentials());
}

export async function saveCredentials(
  env: NodeJS.ProcessEnv,
  next: Credentials,
): Promise<void> {
  const { credentialsPath } = resolveStatePaths(env);
  await writeJson(env, credentialsPath, next);
}

export async function clearCredentials(env: NodeJS.ProcessEnv): Promise<void> {
  const { credentialsPath } = resolveStatePaths(env);
  // `saved_at` records when the clear happened, so the record is the empty
  // one with that single field stamped.
  await writeJson(env, credentialsPath, {
    ...emptyCredentials(),
    saved_at: Date.now(),
  });
}

export async function loadWorkspaces(env: NodeJS.ProcessEnv): Promise<Workspaces> {
  const { workspacesPath } = resolveStatePaths(env);
  return readJson<Workspaces>(workspacesPath, emptyWorkspaces());
}

export async function saveWorkspaces(
  env: NodeJS.ProcessEnv,
  next: Workspaces,
): Promise<void> {
  const { workspacesPath } = resolveStatePaths(env);
  await writeJson(env, workspacesPath, next);
}

export const stateInternals = {
  resolveStatePaths,
} as const;
