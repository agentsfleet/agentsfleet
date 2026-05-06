import { randomBytes } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

function resolveStatePaths() {
  const baseDir = process.env.ZOMBIE_STATE_DIR || path.join(os.homedir(), ".config", "zombiectl");
  return {
    baseDir,
    credentialsPath: path.join(baseDir, "credentials.json"),
    workspacesPath: path.join(baseDir, "workspaces.json"),
    preferencesPath: path.join(baseDir, "preferences.json"),
  };
}

const PREFERENCES_SCHEMA_VERSION = 1;
const PREFERENCES_SENTINEL = Object.freeze({
  schema_version: PREFERENCES_SCHEMA_VERSION,
  posthog_enabled: null,
  decided_at: null,
});

async function ensureBaseDir() {
  const { baseDir } = resolveStatePaths();
  await fs.mkdir(baseDir, { recursive: true });
}

async function readJson(filePath, fallback) {
  try {
    const raw = await fs.readFile(filePath, "utf8");
    return JSON.parse(raw);
  } catch (err) {
    if (err && (err.code === "ENOENT" || err.name === "SyntaxError")) return fallback;
    throw err;
  }
}

async function writeJson(filePath, value) {
  await ensureBaseDir();
  const body = `${JSON.stringify(value, null, 2)}\n`;
  await fs.writeFile(filePath, body, { mode: 0o600 });
}

export function newIdempotencyKey() {
  return randomBytes(12).toString("hex");
}

export async function loadCredentials() {
  const { credentialsPath } = resolveStatePaths();
  return readJson(credentialsPath, { token: null, saved_at: null, session_id: null, api_url: null });
}

export async function saveCredentials(next) {
  const { credentialsPath } = resolveStatePaths();
  await writeJson(credentialsPath, next);
}

export async function clearCredentials() {
  const { credentialsPath } = resolveStatePaths();
  await writeJson(credentialsPath, { token: null, saved_at: Date.now(), session_id: null, api_url: null });
}

export async function loadWorkspaces() {
  const { workspacesPath } = resolveStatePaths();
  return readJson(workspacesPath, { current_workspace_id: null, items: [] });
}

export async function saveWorkspaces(next) {
  const { workspacesPath } = resolveStatePaths();
  await writeJson(workspacesPath, next);
}

export async function loadPreferences({ stderr } = {}) {
  const { preferencesPath } = resolveStatePaths();
  let raw;
  try {
    raw = await fs.readFile(preferencesPath, "utf8");
  } catch (err) {
    if (err && err.code === "ENOENT") return { ...PREFERENCES_SENTINEL };
    if (stderr) stderr.write("zombiectl: preferences.json unreadable; treating as not-decided\n");
    return { ...PREFERENCES_SENTINEL };
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    if (stderr) stderr.write("zombiectl: preferences.json unreadable; treating as not-decided\n");
    return { ...PREFERENCES_SENTINEL };
  }
  if (!parsed || parsed.schema_version !== PREFERENCES_SCHEMA_VERSION) {
    if (stderr) stderr.write("zombiectl: preferences.json schema_version unsupported; treating as not-decided\n");
    return { ...PREFERENCES_SENTINEL };
  }
  return {
    schema_version: PREFERENCES_SCHEMA_VERSION,
    posthog_enabled: typeof parsed.posthog_enabled === "boolean" ? parsed.posthog_enabled : null,
    decided_at: typeof parsed.decided_at === "number" ? parsed.decided_at : null,
  };
}

export async function savePreferences(next) {
  const { preferencesPath } = resolveStatePaths();
  await writeJson(preferencesPath, {
    schema_version: PREFERENCES_SCHEMA_VERSION,
    posthog_enabled: Boolean(next.posthog_enabled),
    decided_at: typeof next.decided_at === "number" ? next.decided_at : Date.now(),
  });
}

export const stateInternals = {
  resolveStatePaths,
  PREFERENCES_SCHEMA_VERSION,
};
