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
  readonly tracesDir: string;
}

export interface Session {
  device_id: string;
  session_id: string;
  last_activity: number | null;
}

// Pinned from Supabase's identity.ts / tracing.layer.ts. Inactivity past
// SESSION_TIMEOUT_MS rotates session_id (device_id stays permanent).
// Traces older than TRACE_RETENTION_DAYS are swept at CLI startup.
export const SESSION_TIMEOUT_MS = 30 * 60 * 1000;
export const TRACE_RETENTION_DAYS = 7;

// Every file under baseDir is owner-rw-only: credentials, workspaces,
// session.json, and the rolling traces/*.ndjson. Single named const so
// the policy is enforced from one site.
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
    tracesDir: path.join(baseDir, "traces"),
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

function validString(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function validFiniteNumber(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function isExpiredSession(lastActivity: number | null, nowMs: number): boolean {
  if (lastActivity === null) return false;
  return nowMs - lastActivity > SESSION_TIMEOUT_MS;
}

// loadSession is best-effort: corrupt or unreadable session.json falls
// back to a fresh identity (and the next saveSession re-creates the file
// at 0o600). Caller decides when to persist; run-command bumps
// last_activity per invocation by saving an updated session.
export async function loadSession(): Promise<Session> {
  const { sessionPath } = resolveStatePaths();
  const fresh = freshSession();
  let raw: Partial<Session>;
  try {
    raw = await readJson<Partial<Session>>(sessionPath, fresh);
  } catch {
    raw = fresh;
  }
  const deviceId = validString(raw.device_id) ?? fresh.device_id;
  const lastActivity = validFiniteNumber(raw.last_activity);
  const existingSessionId = validString(raw.session_id);
  const expired = existingSessionId !== null && isExpiredSession(lastActivity, Date.now());
  const sessionId = existingSessionId === null || expired ? randomUUID() : existingSessionId;
  return { device_id: deviceId, session_id: sessionId, last_activity: lastActivity };
}

export async function saveSession(next: Session): Promise<void> {
  const { sessionPath } = resolveStatePaths();
  await writeJson(sessionPath, next);
}

// Append one JSON line to today's trace file. Best-effort: silently
// drops the record on disk-full / permission-denied / EROFS so the CLI
// boundary path never throws on telemetry. Caller passes a fully formed
// record (no shape coercion happens here). `fs.appendFile`'s `mode`
// option is only honored on file creation; chmod after the append
// guarantees STATE_FILE_MODE even when the trace file was previously
// widened by an operator or another tool.
export async function appendTrace(record: Record<string, unknown>): Promise<void> {
  const { tracesDir } = resolveStatePaths();
  const today = new Date().toISOString().slice(0, 10);
  const tracePath = path.join(tracesDir, `${today}.ndjson`);
  try {
    await fs.mkdir(tracesDir, { recursive: true });
    await fs.appendFile(tracePath, `${JSON.stringify(record)}\n`, { mode: STATE_FILE_MODE });
    await fs.chmod(tracePath, STATE_FILE_MODE);
  } catch {
    // Telemetry never breaks the CLI boundary.
  }
}

// Sweep trace files older than TRACE_RETENTION_DAYS. Best-effort and
// silent — every failure (missing dir, unparseable name, unlink racing
// with a concurrent CLI) is swallowed so telemetry never blocks UX.
export async function cleanupTraces(tracesDir?: string): Promise<void> {
  const dir = tracesDir ?? resolveStatePaths().tracesDir;
  let entries: string[];
  try {
    entries = await fs.readdir(dir);
  } catch {
    return;
  }
  const cutoffMs = Date.now() - TRACE_RETENTION_DAYS * 24 * 60 * 60 * 1000;
  await Promise.all(
    entries.map(async (entry) => {
      const match = /^(\d{4}-\d{2}-\d{2})\.ndjson$/.exec(entry);
      const dateStr = match?.[1];
      if (typeof dateStr !== "string") return;
      const fileMs = Date.parse(`${dateStr}T00:00:00Z`);
      if (!Number.isFinite(fileMs) || fileMs >= cutoffMs) return;
      try {
        await fs.unlink(path.join(dir, entry));
      } catch {
        // Concurrent CLI may have already removed it.
      }
    }),
  );
}

export const stateInternals = {
  resolveStatePaths,
  freshSession,
  isExpiredSession,
} as const;
