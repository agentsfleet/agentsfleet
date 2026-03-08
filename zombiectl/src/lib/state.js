import { randomBytes } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

function resolveStatePaths() {
  const baseDir = process.env.ZOMBIE_STATE_DIR || path.join(os.homedir(), ".zombie");
  return {
    baseDir,
    credentialsPath: path.join(baseDir, "credentials.json"),
    workspacesPath: path.join(baseDir, "workspaces.json"),
    runsPath: path.join(baseDir, "runs.json"),
  };
}

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

export async function loadRuns() {
  const { runsPath } = resolveStatePaths();
  return readJson(runsPath, { items: [] });
}

export async function appendRun(run) {
  const { runsPath } = resolveStatePaths();
  const state = await loadRuns();
  state.items = [run, ...(state.items || [])].slice(0, 200);
  await writeJson(runsPath, state);
}
