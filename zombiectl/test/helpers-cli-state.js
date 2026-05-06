// Shared test scaffolding for CLI integration tests.
//
// Five sibling *.integration.test.js files were each carrying their own
// copy of (a) a Writable buffer that captures stdout/stderr, (b) a
// mkdtemp-based ZOMBIE_STATE_DIR scope guard, and (c) an authed variant
// of the same that pre-seeds credentials.json + workspaces.json so
// auth-required commands don't bounce off the auth guard. Hoisting them
// here cuts ~150 lines of duplication and makes the per-test surface
// uniform.
//
// IMPORTANT — serial-execution assumption:
//
// `withFreshStateDir` and `withAuthedStateDir` mutate
// `process.env.ZOMBIE_STATE_DIR` during the body of `fn` and restore in
// `finally`. This is safe only because `bun test` runs all files in a
// single worker process serially within a file, and (as of Bun 1.3.x)
// does not parallelize across files within a single `bun test` run.
//
// If that assumption ever changes — e.g., a `--parallel` flag is enabled,
// or a shard runner forks — two tests could trample each other's
// ZOMBIE_STATE_DIR mid-flight and one test would see the other's
// pre-seeded credentials. The clean fix at that point is to thread
// ZOMBIE_STATE_DIR through `runCli`'s `io` param (a one-line change in
// state.js to read from caller-provided env first) instead of relying
// on the process-global. Until then this comment is the warning sign.

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { saveCredentials, saveWorkspaces } from "../src/lib/state.js";

/** Discard-all writable stream — handy when a test only cares about return code or stderr. */
export function makeNoop() {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/**
 * Writable that buffers everything into a string. Use one per test to
 * avoid leaking output between cases.
 */
export function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

/**
 * Run `fn` inside an isolated, fresh ZOMBIE_STATE_DIR. The directory is
 * created empty (no credentials, no workspaces). Restores the previous
 * value of process.env.ZOMBIE_STATE_DIR + removes the temp dir on exit,
 * regardless of whether `fn` threw.
 *
 * @param {(stateDir: string) => Promise<T>} fn
 * @returns {Promise<T>}
 */
export async function withFreshStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-test-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

/**
 * Like withFreshStateDir, but pre-seeds the dir so the auth guard passes
 * and workspace-scoped commands have a workspace context. Intended for
 * tests that want to drive an authed CLI invocation without going
 * through the login flow.
 *
 * @param {{ workspaceId: string, workspaceName?: string, sessionId?: string, token?: string, apiUrl?: string | null }} opts
 * @param {(stateDir: string) => Promise<T>} fn
 * @returns {Promise<T>}
 */
export async function withAuthedStateDir(opts, fn) {
  const {
    workspaceId,
    workspaceName = "test-ws",
    sessionId = "sess_test",
    token = "header.payload.sig",
    apiUrl = null,
  } = opts;
  return withFreshStateDir(async (dir) => {
    await saveCredentials({
      token,
      saved_at: Date.now(),
      session_id: sessionId,
      api_url: apiUrl,
    });
    await saveWorkspaces({
      current_workspace_id: workspaceId,
      items: [{ workspace_id: workspaceId, name: workspaceName, created_at: Date.now() }],
    });
    return await fn(dir);
  });
}
