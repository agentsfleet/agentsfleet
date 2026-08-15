// Shared test scaffolding for CLI integration tests.
//
// Five sibling *.integration.test.{js,ts} files were each carrying their
// own copy of (a) a Writable buffer that captures stdout/stderr, (b) a
// mkdtemp-based AGENTSFLEET_STATE_DIR scope guard, and (c) an authed variant
// of the same that pre-seeds credentials.json + workspaces.json so
// auth-required commands don't bounce off the auth guard. Hoisting them
// here cuts ~150 lines of duplication and makes the per-test surface
// uniform.
//
// IMPORTANT — serial-execution assumption:
//
// `withFreshStateDir` and `withAuthedStateDir` mutate
// `process.env.AGENTSFLEET_STATE_DIR` during the body of `fn` and restore in
// `finally`. This is safe only because `bun test` runs all files in a
// single worker process serially within a file, and (as of Bun 1.3.x)
// does not parallelize across files within a single `bun test` run.
//
// If that assumption ever changes — e.g., a `--parallel` flag is enabled,
// or a shard runner forks — two tests could trample each other's
// AGENTSFLEET_STATE_DIR mid-flight and one test would see the other's
// pre-seeded credentials. The clean fix exists: the store reads the
// environment `runCli` is handed, so a test that injects
// `{ ...stateDirEnv(), … }` (or its own directory) is immune to the
// process-global entirely. These mutation-scoped helpers remain for suites
// still seeding through the process environment; converting them wholesale
// belongs to the `useFreshStateDir` conversion stream.

import { afterEach, beforeEach } from "bun:test";
import fs from "node:fs/promises";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { saveCredentials, saveWorkspaces } from "../src/lib/state.ts";
import { STATE_DIR_ENV } from "../src/lib/config-dir.ts";
import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
} from "../src/constants/cli-credential.ts";

// A credential shaped the way services/credentials.ts validates on load: the
// afc_ prefix and a 64-character lower-case hex body. Built by repetition
// rather than written out, so this file carries no high-entropy literal for a
// secret scanner to flag, and so nobody mistakes it for a real credential.
// A seeded value that fails the load check would read as logged-out and every
// authed fixture would bounce off the auth guard.
const FIXTURE_BODY_CHAR = "a";
export const FIXTURE_CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${FIXTURE_BODY_CHAR.repeat(CLI_CREDENTIAL_BODY_LEN)}`;
export const FIXTURE_CREDENTIAL_ID = "cli_cred_fixture";

const TMP_PREFIX = "agentsfleet-test-";

/**
 * The current process state-dir as an injectable env fragment, read at call
 * time because withFreshStateDir / withAuthedStateDir swap the directory per
 * case. Spread it into a runCli `io.env` so the injected environment reaches
 * the store pointing at the same directory the fixture seeded:
 * `env: { ...stateDirEnv(), AGENTSFLEET_API_URL: url }`.
 */
export function stateDirEnv(): NodeJS.ProcessEnv {
  return { [STATE_DIR_ENV]: process.env[STATE_DIR_ENV] };
}

/**
 * The environment every in-process runCli test injects: the current state
 * dir plus the case's own overrides. One call site per test instead of a
 * hand-spread `{ ...stateDirEnv(), … }` a test can forget — forgetting it
 * points the store at the operator's real `~/.config/agentsfleet`, which is
 * how a suite once overwrote a developer's live credentials.json. Throws
 * when no state dir is set at all (the setup preload or a fixture always
 * sets one), so a mis-ordered fixture fails loudly instead of escaping the
 * sandbox. Tests proving DIVERGENCE between injected and process
 * environments (injected-env.integration.test.ts) build their env by hand
 * on purpose.
 */
export function cliEnv(overrides: NodeJS.ProcessEnv = {}): NodeJS.ProcessEnv {
  const base = stateDirEnv();
  if (base[STATE_DIR_ENV] === undefined) {
    throw new Error(
      `cliEnv(): ${STATE_DIR_ENV} is unset — run under test/setup.ts or inside a state-dir fixture`,
    );
  }
  return { ...base, ...overrides };
}

// Mirror of helpers.ts:TestStream — Writable + optional isTTY so tests
// can flip TTY-dependent code paths without an `as` cast at every site.
export type TestStream = Writable & { isTTY?: boolean };

/** Discard-all writable stream — handy when a test only cares about return code or stderr. */
export function makeNoop(): TestStream {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/**
 * Writable that buffers everything into a string. Use one per test to
 * avoid leaking output between cases.
 */
export function bufferStream(): { stream: TestStream; read: () => string } {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

/**
 * Run `fn` inside an isolated, fresh AGENTSFLEET_STATE_DIR. The directory is
 * created empty (no credentials, no workspaces). Restores the previous
 * value of process.env.AGENTSFLEET_STATE_DIR + removes the temp dir on exit,
 * regardless of whether `fn` threw.
 */
export async function withFreshStateDir<T>(
  fn: (stateDir: string) => Promise<T>,
): Promise<T> {
  const previous = process.env[STATE_DIR_ENV];
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), TMP_PREFIX));
  process.env[STATE_DIR_ENV] = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env[STATE_DIR_ENV];
    else process.env[STATE_DIR_ENV] = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

/**
 * Hook-scoped sibling of `withFreshStateDir`, for files that want the isolated
 * directory to span a whole describe block instead of one call. Registers
 * `beforeEach`/`afterEach` at the calling scope and returns an accessor for the
 * directory belonging to the running test.
 *
 *     const stateDir = useFreshStateDir();
 *     test("...", async () => { await Bun.write(`${stateDir()}/x.json`, "…"); });
 */
export function useFreshStateDir(): () => string {
  let dir = "";
  let previous: string | undefined;
  beforeEach(() => {
    previous = process.env[STATE_DIR_ENV];
    dir = mkdtempSync(path.join(os.tmpdir(), TMP_PREFIX));
    process.env[STATE_DIR_ENV] = dir;
  });
  afterEach(() => {
    if (previous === undefined) delete process.env[STATE_DIR_ENV];
    else process.env[STATE_DIR_ENV] = previous;
    rmSync(dir, { recursive: true, force: true });
  });
  return () => dir;
}

/**
 * Save/restore only — no directory is created. For files whose individual tests
 * assign their own state dir because they seed its contents before the CLI
 * reads it (the telemetry suites write `telemetry.json` first). Those tests own
 * the directory; this only guarantees the environment variable is put back, so
 * one file cannot leak its state dir into the next.
 */
export function preserveStateDirEnv(): void {
  let previous: string | undefined;
  beforeEach(() => {
    previous = process.env[STATE_DIR_ENV];
  });
  afterEach(() => {
    if (previous === undefined) delete process.env[STATE_DIR_ENV];
    else process.env[STATE_DIR_ENV] = previous;
  });
}

export interface AuthedStateDirOpts {
  workspaceId: string;
  workspaceName?: string;
  sessionId?: string;
  token?: string;
  apiUrl?: string | null;
}

/**
 * Like withFreshStateDir, but pre-seeds the dir so the auth guard passes
 * and workspace-scoped commands have a workspace context. Intended for
 * tests that want to drive an authed CLI invocation without going
 * through the login flow.
 */
export async function withAuthedStateDir<T>(
  opts: AuthedStateDirOpts,
  fn: (stateDir: string) => Promise<T>,
): Promise<T> {
  const {
    workspaceId,
    workspaceName = "test-ws",
    sessionId = "sess_test",
    token = FIXTURE_CREDENTIAL,
    apiUrl = null,
  } = opts;
  return withFreshStateDir(async (dir) => {
    await saveCredentials(process.env, {
      token,
      saved_at: Date.now(),
      session_id: sessionId,
      api_url: apiUrl,
      credential_id: FIXTURE_CREDENTIAL_ID,
    });
    await saveWorkspaces(process.env, {
      current_workspace_id: workspaceId,
      items: [{ workspace_id: workspaceId, name: workspaceName, created_at: Date.now() }],
    });
    return await fn(dir);
  });
}
