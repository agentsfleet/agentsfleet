import { randomBytes } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

// On-disk state shapes. Both files live under `$ZOMBIE_STATE_DIR` (or
// `~/.config/zombiectl`) at mode 0o600. JSON is parsed permissively —
// missing files return the fallback, corrupt files raise.

export interface StatePaths {
  readonly baseDir: string;
  readonly credentialsPath: string;
  readonly workspacesPath: string;
}

export interface Credentials {
  token: string | null;
  saved_at: number | null;
  session_id: string | null;
  api_url: string | null;
}

export interface WorkspaceItem {
  workspace_id: string;
  name: string;
  created_at: number | null;
}

export interface Workspaces {
  current_workspace_id: string | null;
  items: WorkspaceItem[];
}

function resolveStatePaths(): StatePaths {
  const baseDir = process.env.ZOMBIE_STATE_DIR || path.join(os.homedir(), ".config", "zombiectl");
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
  await fs.writeFile(filePath, body, { mode: 0o600 });
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
