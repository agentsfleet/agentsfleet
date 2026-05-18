import { randomBytes, randomUUID } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

// On-disk state shapes. All files live under `$ZOMBIE_STATE_DIR` (or
// `~/.config/zombiectl`) at mode 0o600. JSON is parsed permissively —
// missing files return the fallback, corrupt files raise.

export interface StatePaths {
  readonly baseDir: string;
  readonly credentialsPath: string;
  readonly workspacesPath: string;
  readonly sessionPath: string;
}

export interface Session {
  device_id: string;
  session_id: string;
  last_activity: number | null;
}

// Pinned from Supabase's identity.ts. Inactivity past SESSION_TIMEOUT_MS
// rotates session_id (device_id stays permanent).
export const SESSION_TIMEOUT_MS = 30 * 60 * 1000;

// Every file under baseDir is owner-rw-only: credentials, workspaces,
// session.json. Single named const so the policy is enforced from
// one site.
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
  const baseDir = process.env.ZOMBIE_STATE_DIR || path.join(os.homedir(), ".config", "zombiectl");
  return {
    baseDir,
    credentialsPath: path.join(baseDir, "credentials.json"),
    workspacesPath: path.join(baseDir, "workspaces.json"),
    sessionPath: path.join(baseDir, "session.json"),
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

function freshSession(): Session {
  return {
    device_id: randomUUID(),
    session_id: randomUUID(),
    last_activity: null,
  };
}

// Strict UUID validation. randomUUID() produces v4, but we accept any
// canonical UUID variant on read (some test fixtures use v7). Length +
// hex-with-dashes pattern protects PostHog and the trace file from
// arbitrary payloads if session.json is hand-edited or poisoned.
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
function validUuid(v: unknown): string | null {
  return typeof v === "string" && UUID_RE.test(v) ? v : null;
}

function validFiniteNumber(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function isExpiredSession(lastActivity: number | null, nowMs: number): boolean {
  if (lastActivity === null) return false;
  return nowMs - lastActivity > SESSION_TIMEOUT_MS;
}

// loadSession returns a usable Session for ENOENT (first run) and
// SyntaxError (corrupt JSON) — those collapse to a fresh identity via
// readJson's fallback. Other errors (EACCES, EISDIR, etc.) propagate
// to the caller — silently regenerating device_id on a transient
// permission glitch would defeat the "permanent" guarantee. Caller
// (cli.ts) catches and degrades to EMPTY_SESSION but does NOT save
// over the original file when load failed.
export async function loadSession(): Promise<Session> {
  const { sessionPath } = resolveStatePaths();
  const fresh = freshSession();
  const raw = await readJson<Partial<Session>>(sessionPath, fresh);
  const deviceId = validUuid(raw.device_id) ?? fresh.device_id;
  const lastActivity = validFiniteNumber(raw.last_activity);
  const existingSessionId = validUuid(raw.session_id);
  const expired = existingSessionId !== null && isExpiredSession(lastActivity, Date.now());
  const sessionId = existingSessionId === null || expired ? randomUUID() : existingSessionId;
  return { device_id: deviceId, session_id: sessionId, last_activity: lastActivity };
}

export async function saveSession(next: Session): Promise<void> {
  const { sessionPath } = resolveStatePaths();
  await writeJson(sessionPath, next);
}

export const stateInternals = {
  resolveStatePaths,
  freshSession,
  isExpiredSession,
} as const;
