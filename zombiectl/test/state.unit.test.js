import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import {
  clearCredentials,
  loadPreferences,
  savePreferences,
  stateInternals,
} from "../src/lib/state.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

async function withStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-state-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test("resolveStatePaths defaults to XDG-style zombiectl config directory", () => {
  const previous = process.env.ZOMBIE_STATE_DIR;
  delete process.env.ZOMBIE_STATE_DIR;
  try {
    const paths = stateInternals.resolveStatePaths();
    const expectedBase = path.join(os.homedir(), ".config", "zombiectl");
    assert.equal(paths.baseDir, expectedBase);
    assert.equal(paths.credentialsPath, path.join(expectedBase, "credentials.json"));
    assert.equal(paths.workspacesPath, path.join(expectedBase, "workspaces.json"));
    assert.equal(paths.preferencesPath, path.join(expectedBase, "preferences.json"));
  } finally {
    if (previous !== undefined) process.env.ZOMBIE_STATE_DIR = previous;
  }
});

test("resolveStatePaths honors ZOMBIE_STATE_DIR override", () => {
  const previous = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = "/tmp/zombiectl-state-test";
  try {
    const paths = stateInternals.resolveStatePaths();
    assert.equal(paths.baseDir, "/tmp/zombiectl-state-test");
    assert.equal(paths.credentialsPath, "/tmp/zombiectl-state-test/credentials.json");
    assert.equal(paths.workspacesPath, "/tmp/zombiectl-state-test/workspaces.json");
    assert.equal(paths.preferencesPath, "/tmp/zombiectl-state-test/preferences.json");
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
  }
});

test("loadPreferences returns sentinel on missing file and does not create it", async () => {
  await withStateDir(async (dir) => {
    const prefs = await loadPreferences();
    assert.equal(prefs.posthog_enabled, null);
    assert.equal(prefs.decided_at, null);
    assert.equal(prefs.schema_version, stateInternals.PREFERENCES_SCHEMA_VERSION);

    const exists = await fs.stat(path.join(dir, "preferences.json")).then(() => true).catch(() => false);
    assert.equal(exists, false);
  });
});

test("savePreferences writes mode 0600 and round-trips through loadPreferences", async () => {
  await withStateDir(async (dir) => {
    await savePreferences({ posthog_enabled: true });
    const stat = await fs.stat(path.join(dir, "preferences.json"));
    assert.equal(stat.mode & 0o777, 0o600);
    const prefs = await loadPreferences();
    assert.equal(prefs.posthog_enabled, true);
    assert.equal(typeof prefs.decided_at, "number");
    assert.equal(prefs.schema_version, stateInternals.PREFERENCES_SCHEMA_VERSION);
  });
});

test("savePreferences honors caller-supplied decided_at", async () => {
  await withStateDir(async () => {
    const ts = 1700000000000;
    await savePreferences({ posthog_enabled: false, decided_at: ts });
    const prefs = await loadPreferences();
    assert.equal(prefs.posthog_enabled, false);
    assert.equal(prefs.decided_at, ts);
  });
});

test("loadPreferences returns sentinel and warns on corrupt JSON without overwriting the file", async () => {
  await withStateDir(async (dir) => {
    const filePath = path.join(dir, "preferences.json");
    await fs.writeFile(filePath, "{not valid json", { mode: 0o600 });
    const err = bufferStream();
    const prefs = await loadPreferences({ stderr: err.stream });
    assert.equal(prefs.posthog_enabled, null);
    assert.match(err.read(), /preferences\.json unreadable/);
    const stillBroken = await fs.readFile(filePath, "utf8");
    assert.equal(stillBroken, "{not valid json");
  });
});

test("loadPreferences returns sentinel and warns on unsupported schema_version", async () => {
  await withStateDir(async (dir) => {
    const filePath = path.join(dir, "preferences.json");
    await fs.writeFile(filePath, JSON.stringify({ schema_version: 99, posthog_enabled: true }), { mode: 0o600 });
    const err = bufferStream();
    const prefs = await loadPreferences({ stderr: err.stream });
    assert.equal(prefs.posthog_enabled, null);
    assert.match(err.read(), /schema_version unsupported/);
  });
});

test("clearCredentials does not touch preferences.json", async () => {
  await withStateDir(async (dir) => {
    await savePreferences({ posthog_enabled: true });
    const before = await fs.readFile(path.join(dir, "preferences.json"), "utf8");
    await clearCredentials();
    const after = await fs.readFile(path.join(dir, "preferences.json"), "utf8");
    assert.equal(after, before);
    const prefs = await loadPreferences();
    assert.equal(prefs.posthog_enabled, true);
  });
});
