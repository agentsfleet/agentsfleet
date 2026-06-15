import { randomBytes } from "node:crypto";
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
}

export interface WorkspaceItem {
  workspace_id: string;
  // Server can return name=null on the create-response path
  // (workspaceShow / workspaceList tolerate this with `name ?? "—"`).
  // Tightening to non-null here would force every caller to coerce.
  name: string | null;
  created_at: number | null;
}

export interface Workspaces {
  current_workspace_id: string | null;
  items: WorkspaceItem[];
}

function resolveStatePaths(): StatePaths {
  const baseDir = process.env.AGENTSFLEET_STATE_DIR || path.join(os.homedir(), ".config", "agentsfleet");
  return {
    baseDir,
    credentialsPath: path.join(baseDir, "credentials.json"),
    workspacesPath: path.join(baseDir, "workspaces.json"),
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

export function newIdempotencyKey(): string {
  return randomBytes(12).toString("hex");
}

export async function loadCredentials(): Promise<Credentials> {
  const { credentialsPath } = resolveStatePaths();
  return readJson<Credentials>(credentialsPath, {
    token: null,
    saved_at: null,
    session_id: null,
    api_url: null,
  });
}

export async function saveCredentials(next: Credentials): Promise<void> {
  const { credentialsPath } = resolveStatePaths();
  await writeJson(credentialsPath, next);
}

export async function clearCredentials(): Promise<void> {
  const { credentialsPath } = resolveStatePaths();
  await writeJson(credentialsPath, {
    token: null,
    saved_at: Date.now(),
    session_id: null,
    api_url: null,
  });
}

export async function loadWorkspaces(): Promise<Workspaces> {
  const { workspacesPath } = resolveStatePaths();
  return readJson<Workspaces>(workspacesPath, { current_workspace_id: null, items: [] });
}

export async function saveWorkspaces(next: Workspaces): Promise<void> {
  const { workspacesPath } = resolveStatePaths();
  await writeJson(workspacesPath, next);
}

export const stateInternals = {
  resolveStatePaths,
} as const;
