import { test, expect } from "bun:test";
import os from "node:os";
import path from "node:path";
import fs from "node:fs/promises";

import { newIdempotencyKey, loadCredentials, saveCredentials, clearCredentials, loadWorkspaces, saveWorkspaces } from "../src/lib/state.js";
import { printSection, printKeyValue, printTable } from "../src/output/index.ts";
import {
  wsZombiesPath,
  wsZombiePath,
  wsZombieMessagesPath,
  wsZombieEventsPath,
  wsZombieEventsStreamPath,
  wsEventsPath,
  wsCredentialsPath,
  wsCredentialPath,
  wsGrantRequestPath,
  wsGrantsListPath,
  wsGrantPath,
} from "../src/lib/api-paths.js";

function tmpDir() {
  return path.join(os.tmpdir(), `zctl-cov-${Date.now()}-${Math.random().toString(16).slice(2)}`);
}

function captureStream() {
  const chunks = [];
  return { write(s) { chunks.push(s); }, get text() { return chunks.join(""); } };
}

// ── state.js ────────────────────────────────────────────────────────────

test("newIdempotencyKey returns a 24-char hex string", () => {
  const k = newIdempotencyKey();
  expect(k).toMatch(/^[0-9a-f]{24}$/);
  expect(newIdempotencyKey()).not.toBe(k);
});

test("save/load/clear Credentials roundtrip writes mode 0600 and clears to nulls", async () => {
  const dir = tmpDir();
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await saveCredentials({ token: "tok_abc", saved_at: 1, session_id: "sess", api_url: "https://api.example" });
    const after = await loadCredentials();
    expect(after.token).toBe("tok_abc");
    const stat = await fs.stat(path.join(dir, "credentials.json"));
    expect((stat.mode & 0o777).toString(8)).toBe("600");
    await clearCredentials();
    const cleared = await loadCredentials();
    expect(cleared.token).toBeNull();
    expect(cleared.saved_at).toEqual(expect.any(Number));
  } finally {
    delete process.env.ZOMBIE_STATE_DIR;
    await fs.rm(dir, { recursive: true, force: true });
  }
});

test("save/load Workspaces roundtrip persists current_workspace_id + items[]", async () => {
  const dir = tmpDir();
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await saveWorkspaces({ current_workspace_id: "ws_1", items: [{ id: "ws_1", name: "main" }] });
    const after = await loadWorkspaces();
    expect(after.current_workspace_id).toBe("ws_1");
    expect(after.items).toHaveLength(1);
  } finally {
    delete process.env.ZOMBIE_STATE_DIR;
    await fs.rm(dir, { recursive: true, force: true });
  }
});

test("loadCredentials returns the default shape when no file exists", async () => {
  const dir = tmpDir();
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    const c = await loadCredentials();
    expect(c).toEqual({ token: null, saved_at: null, session_id: null, api_url: null });
  } finally {
    delete process.env.ZOMBIE_STATE_DIR;
  }
});

// ── output/index.js ─────────────────────────────────────────────────────

test("printSection writes the section header to the stream", () => {
  const s = captureStream();
  printSection(s, "AGENTS");
  expect(s.text.length).toBeGreaterThan(0);
  expect(s.text).toContain("AGENTS");
});

test("printKeyValue writes each row as a labelled pair", () => {
  const s = captureStream();
  printKeyValue(s, [
    ["zombie_id", "zmb_1"],
    ["status", "active"],
  ]);
  expect(s.text).toContain("zombie_id");
  expect(s.text).toContain("zmb_1");
  expect(s.text).toContain("status");
});

test("printTable writes columns and rows", () => {
  const s = captureStream();
  printTable(
    s,
    [{ label: "name", key: "name" }, { label: "status", key: "status" }],
    [{ name: "alpha", status: "live" }, { name: "beta", status: "stopped" }],
  );
  expect(s.text).toContain("alpha");
  expect(s.text).toContain("stopped");
});

// ── api-paths.js (URL-encoding contract) ────────────────────────────────

test("path helpers URL-encode workspace id and zombie id components", () => {
  const ws = "ws/with slash";
  const z = "zmb spaces";
  expect(wsZombiesPath(ws)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies");
  expect(wsZombiePath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces");
  expect(wsZombieMessagesPath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/messages");
  expect(wsZombieEventsPath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/events");
  expect(wsZombieEventsStreamPath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/events/stream");
  expect(wsEventsPath(ws)).toBe("/v1/workspaces/ws%2Fwith%20slash/events");
  expect(wsCredentialsPath(ws)).toBe("/v1/workspaces/ws%2Fwith%20slash/credentials");
  expect(wsCredentialPath(ws, "github_token")).toBe("/v1/workspaces/ws%2Fwith%20slash/credentials/github_token");
  expect(wsGrantRequestPath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/integration-requests");
  expect(wsGrantsListPath(ws, z)).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/integration-grants");
  expect(wsGrantPath(ws, z, "grant_x")).toBe("/v1/workspaces/ws%2Fwith%20slash/zombies/zmb%20spaces/integration-grants/grant_x");
});
