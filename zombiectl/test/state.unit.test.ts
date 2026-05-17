import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  SESSION_TIMEOUT_MS,
  appendTrace,
  cleanupTraces,
  loadSession,
  saveSession,
  stateInternals,
} from "../src/lib/state.ts";

async function withTempStateDir(fn: (dir: string) => Promise<void>): Promise<void> {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-state-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

test("resolveStatePaths defaults to XDG-style zombiectl config directory", () => {
  const previous = process.env.ZOMBIE_STATE_DIR;
  delete process.env.ZOMBIE_STATE_DIR;
  try {
    const paths = stateInternals.resolveStatePaths();
    const expectedBase = path.join(os.homedir(), ".config", "zombiectl");
    assert.equal(paths.baseDir, expectedBase);
    assert.equal(paths.credentialsPath, path.join(expectedBase, "credentials.json"));
    assert.equal(paths.workspacesPath, path.join(expectedBase, "workspaces.json"));
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
    assert.equal(paths.sessionPath, "/tmp/zombiectl-state-test/session.json");
    assert.equal(paths.tracesDir, "/tmp/zombiectl-state-test/traces");
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
  }
});

test("loadSession first-run generates UUID device_id and session_id", async () => {
  await withTempStateDir(async () => {
    const s = await loadSession();
    assert.match(s.device_id, UUID_RE);
    assert.match(s.session_id, UUID_RE);
    assert.notEqual(s.device_id, s.session_id);
    assert.equal(s.last_activity, null);
  });
});

test("loadSession keeps device_id and session_id when within TTL", async () => {
  await withTempStateDir(async (dir) => {
    const pinned = {
      device_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      session_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      last_activity: Date.now() - 60_000, // 1 min ago, well inside 30 min TTL
    };
    await fs.writeFile(path.join(dir, "session.json"), JSON.stringify(pinned), { mode: 0o600 });
    const s = await loadSession();
    assert.equal(s.device_id, pinned.device_id);
    assert.equal(s.session_id, pinned.session_id);
    assert.equal(s.last_activity, pinned.last_activity);
  });
});

test("loadSession rotates session_id when last_activity is past TTL, keeps device_id", async () => {
  await withTempStateDir(async (dir) => {
    const keepDevice = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    const oldSession = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    const stale = {
      device_id: keepDevice,
      session_id: oldSession,
      last_activity: Date.now() - SESSION_TIMEOUT_MS - 60_000,
    };
    await fs.writeFile(path.join(dir, "session.json"), JSON.stringify(stale), { mode: 0o600 });
    const s = await loadSession();
    assert.equal(s.device_id, keepDevice);
    assert.notEqual(s.session_id, oldSession);
    assert.match(s.session_id, UUID_RE);
  });
});

test("loadSession recovers from corrupt session.json with a fresh identity", async () => {
  await withTempStateDir(async (dir) => {
    await fs.writeFile(path.join(dir, "session.json"), "{ this is not valid json", { mode: 0o600 });
    const s = await loadSession();
    assert.match(s.device_id, UUID_RE);
    assert.match(s.session_id, UUID_RE);
  });
});

test("loadSession regenerates non-UUID device_id (defensive — poisoned session.json)", async () => {
  await withTempStateDir(async (dir) => {
    const poisoned = { device_id: "not-a-uuid", session_id: "also-bad", last_activity: Date.now() };
    await fs.writeFile(path.join(dir, "session.json"), JSON.stringify(poisoned), { mode: 0o600 });
    const s = await loadSession();
    assert.match(s.device_id, UUID_RE, "device_id should be regenerated to a valid UUID");
    assert.match(s.session_id, UUID_RE, "session_id should be regenerated to a valid UUID");
    assert.notEqual(s.device_id, "not-a-uuid");
  });
});

test("loadSession propagates permission errors (does NOT silently regenerate device_id)", async () => {
  await withTempStateDir(async (dir) => {
    if (process.getuid?.() === 0) return; // root bypasses chmod restrictions
    const sessionPath = path.join(dir, "session.json");
    await fs.writeFile(sessionPath, JSON.stringify({
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      last_activity: Date.now(),
    }), { mode: 0o000 });
    let thrown: unknown = null;
    try { await loadSession(); } catch (e) { thrown = e; }
    assert.ok(thrown, "loadSession must propagate EACCES rather than swallow it and rotate device_id");
    await fs.chmod(sessionPath, 0o600).catch(() => {});
  });
});

test("appendTrace refuses to follow a planted symlink (security guard)", async () => {
  await withTempStateDir(async (dir) => {
    const tracesDir = path.join(dir, "traces");
    await fs.mkdir(tracesDir, { recursive: true });
    const today = new Date().toISOString().slice(0, 10);
    const tracePath = path.join(tracesDir, `${today}.ndjson`);
    const sinkPath = path.join(dir, "sink.txt");
    await fs.writeFile(sinkPath, "pre-existing sink content\n");
    await fs.symlink(sinkPath, tracePath);
    await appendTrace({ ts: "x", command: "y", exit_code: 0, duration_ms: 0 });
    const sinkBody = await fs.readFile(sinkPath, "utf8");
    assert.equal(sinkBody, "pre-existing sink content\n", "appendTrace must not write through the symlink");
  });
});

test("saveSession writes file at 0o600", async () => {
  await withTempStateDir(async (dir) => {
    await saveSession({ device_id: "d", session_id: "s", last_activity: 123 });
    const stat = await fs.stat(path.join(dir, "session.json"));
    assert.equal(stat.mode & 0o777, 0o600);
  });
});

test("appendTrace writes one JSON line per call to today's NDJSON file", async () => {
  await withTempStateDir(async (dir) => {
    await appendTrace({ ts: "2026-05-17T12:00:00Z", command: "first", exit_code: 0, duration_ms: 1 });
    await appendTrace({ ts: "2026-05-17T12:00:01Z", command: "second", exit_code: 1, duration_ms: 2 });
    const today = new Date().toISOString().slice(0, 10);
    const tracePath = path.join(dir, "traces", `${today}.ndjson`);
    const body = await fs.readFile(tracePath, "utf8");
    const lines = body.trim().split("\n");
    assert.equal(lines.length, 2);
    const first = JSON.parse(lines[0]!);
    const second = JSON.parse(lines[1]!);
    assert.equal(first.command, "first");
    assert.equal(second.command, "second");
    assert.equal(second.exit_code, 1);
  });
});

test("appendTrace re-applies 0o600 even when trace file was widened (chmod after append)", async () => {
  await withTempStateDir(async (dir) => {
    const tracesDir = path.join(dir, "traces");
    await fs.mkdir(tracesDir, { recursive: true });
    const today = new Date().toISOString().slice(0, 10);
    const tracePath = path.join(tracesDir, `${today}.ndjson`);
    // Pre-create the file at 0o644 (widened) so the appendFile `mode`
    // option (which only fires on creation) cannot enforce 0o600.
    await fs.writeFile(tracePath, "{\"pre-existing\":true}\n", { mode: 0o644 });
    const before = await fs.stat(tracePath);
    assert.equal(before.mode & 0o777, 0o644);
    await appendTrace({ ts: "x", command: "tighten", exit_code: 0, duration_ms: 1 });
    const after = await fs.stat(tracePath);
    assert.equal(after.mode & 0o777, 0o600, "appendTrace should chmod back to 0o600 after the append");
  });
});

test("appendTrace never throws even if mkdir or write fails (read-only baseDir)", async () => {
  await withTempStateDir(async (dir) => {
    // Pre-create traces as a regular file (not a dir) so mkdir recursive
    // succeeds but appendFile rejects with EISDIR/EEXIST/etc. Either way
    // the call must resolve.
    await fs.writeFile(path.join(dir, "traces"), "not-a-dir");
    await appendTrace({ ts: "x", command: "y", exit_code: 0, duration_ms: 0 }); // no throw
  });
});

test("cleanupTraces removes files older than 7 days, preserves recent ones", async () => {
  await withTempStateDir(async (dir) => {
    const tracesDir = path.join(dir, "traces");
    await fs.mkdir(tracesDir, { recursive: true });
    const today = new Date().toISOString().slice(0, 10);
    const oldDate = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
    const recentDate = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
    await fs.writeFile(path.join(tracesDir, `${oldDate}.ndjson`), "");
    await fs.writeFile(path.join(tracesDir, `${recentDate}.ndjson`), "");
    await fs.writeFile(path.join(tracesDir, `${today}.ndjson`), "");
    // Unparseable name — must be left alone.
    await fs.writeFile(path.join(tracesDir, "not-a-trace.txt"), "");
    await cleanupTraces();
    const remaining = (await fs.readdir(tracesDir)).sort();
    assert.deepEqual(remaining, [`${recentDate}.ndjson`, `${today}.ndjson`, "not-a-trace.txt"].sort());
  });
});

test("cleanupTraces resolves without throwing when traces dir does not exist", async () => {
  await withTempStateDir(async () => {
    await cleanupTraces(); // no throw, no traces dir
  });
});
