import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

// On-disk state shapes. All files live under `$AGENTSFLEET_STATE_DIR` (or
// `~/.config/agentsfleet`) at mode 0o600. JSON is parsed permissively —
// missing files return the fallback, corrupt files raise.
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

function resolveStatePaths(): StatePaths {
  const baseDir =
    process.env.AGENTSFLEET_STATE_DIR ||
    path.join(os.homedir(), ".config", "agentsfleet");
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

async function ensureBaseDir(): Promise<void> {
  const { baseDir } = resolveStatePaths();
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

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await ensureBaseDir();
  const body = `${JSON.stringify(value, null, 2)}\n`;
  await fs.writeFile(filePath, body, { mode: STATE_FILE_MODE });
}

export async function loadCredentials(): Promise<Credentials> {
  const { credentialsPath } = resolveStatePaths();
  return readJson<Credentials>(credentialsPath, emptyCredentials());
}

export async function saveCredentials(next: Credentials): Promise<void> {
  const { credentialsPath } = resolveStatePaths();
  await writeJson(credentialsPath, next);
}

export async function clearCredentials(): Promise<void> {
  const { credentialsPath } = resolveStatePaths();
  // `saved_at` records when the clear happened, so the record is the empty
  // one with that single field stamped.
  await writeJson(credentialsPath, {
    ...emptyCredentials(),
    saved_at: Date.now(),
  });
}

export async function loadWorkspaces(): Promise<Workspaces> {
  const { workspacesPath } = resolveStatePaths();
  return readJson<Workspaces>(workspacesPath, emptyWorkspaces());
}

export async function saveWorkspaces(next: Workspaces): Promise<void> {
  const { workspacesPath } = resolveStatePaths();
  await writeJson(workspacesPath, next);
}

export const stateInternals = {
  resolveStatePaths,
} as const;
