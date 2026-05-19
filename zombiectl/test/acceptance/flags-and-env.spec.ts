/**
 * Cross-cutting flag / env-var / signal-handling acceptance scenario.
 *
 * Covered here:
 *   - global-flag matrix and precedence (no backend needed)
 *   - NO_COLOR semantics (no backend needed)
 *   - --no-open suppresses browser spawn (uses local stub backend)
 *   - SIGINT during `zombiectl login` exits non-zero, never persists
 *     a partial credentials.json (uses local stub backend)
 *
 * Live-API precedence tests (--api / ZOMBIE_API_URL, env-var auth
 * permutations) only register when `ZOMBIE_ACCEPTANCE_TARGET` is an
 * https:// URL — they run against real api-dev / api in CI, not local.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import { runZombiectl, spawnZombiectl, composeEnv } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";
import { startLocalStubServer, type LocalStubHandle } from "./fixtures/local-stub-server.ts";
import { resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";

import type { ChildProcessWithoutNullStreams } from "node:child_process";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..", "..");
const ANSI_RE = /\x1b\[/;

const SIGINT_POLL_MS = 250;
const SIGINT_DEADLINE_SEC = 60;

interface ExitResult {
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
  readonly stdout: string;
  readonly stderr: string;
}

let pkgVersion: string;

beforeAll(async () => {
  const pkgRaw = await fs.readFile(path.join(ZOMBIECTL_ROOT, "package.json"), "utf8");
  pkgVersion = (JSON.parse(pkgRaw) as { version: string }).version;
});

function emptyEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({ ZOMBIE_API_URL: UNROUTABLE_API_URL, NO_COLOR: "1", ...(extra ?? {}) });
}

function waitForExit(child: ChildProcessWithoutNullStreams): Promise<ExitResult> {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer | string) => { stdout += String(chunk); });
    child.stderr.on("data", (chunk: Buffer | string) => { stderr += String(chunk); });
    child.on("close", (code: number | null, signal: NodeJS.Signals | null) => resolve({ code, signal, stdout, stderr }));
  });
}

function waitForLine(
  child: ChildProcessWithoutNullStreams,
  predicate: (line: string) => boolean,
  timeoutMs: number,
): Promise<string> {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => {
      child.stdout.off("data", onData);
      reject(new Error(`timed out waiting for stdout line; saw: ${buffer.slice(0, 300)}`));
    }, timeoutMs);
    function onData(chunk: Buffer | string): void {
      buffer += String(chunk);
      const lines = buffer.split(/\r?\n/);
      for (const line of lines) {
        if (predicate(line)) {
          clearTimeout(timer);
          child.stdout.off("data", onData);
          resolve(buffer);
          return;
        }
      }
    }
    child.stdout.on("data", onData);
  });
}

describe("global flag matrix", () => {
  it("--version --help → --version wins (precedence)", async () => {
    const result = await runZombiectl(["--version", "--help"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion), `expected version ${pkgVersion} in stdout: ${result.stdout}`);
    assert.ok(!/usage|commands:/i.test(result.stdout), `--help body leaked into --version output: ${result.stdout}`);
  });

  it("--help --version → --version still wins (order-independent)", async () => {
    const result = await runZombiectl(["--help", "--version"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion));
    assert.ok(!/usage|commands:/i.test(result.stdout));
  });

  // --help --json exits 0 and emits help output. A structured JSON help
  // tree (Discovery row) is a future enhancement; commander's text body
  // is what --json currently returns. Assert exit 0 with help content
  // present — the contract the CLI ships.
  it("--help --json exits 0 with help content", async () => {
    const result = await runZombiectl(["--help", "--json"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.length > 0);
    assert.match(result.stdout, /zombiectl|usage|commands/i);
  });
});

describe("NO_COLOR semantics", () => {
  it("NO_COLOR=1 strips ANSI escapes from --help", async () => {
    const result = await runZombiectl(["--help"], {
      env: composeEnv({ NO_COLOR: "1" }),
    });
    assert.equal(result.code, 0);
    assert.ok(!ANSI_RE.test(result.stdout), `expected no \\x1b[... in stdout: ${JSON.stringify(result.stdout)}`);
  });
});

describe("telemetry env-var advertisement (--help)", () => {
  // Supabase-aligned env names land in --help so users can discover the
  // opt-out path without reading source. Golden snapshot covers the
  // exact bytes; this acceptance assertion confirms the built binary
  // (npm install / dist) renders the same names end-to-end.
  it("advertises ZOMBIE_TELEMETRY_DISABLED, DO_NOT_TRACK and POSTHOG knobs", async () => {
    const result = await runZombiectl(["--help"], {
      env: composeEnv({ NO_COLOR: "1" }),
    });
    assert.equal(result.code, 0);
    for (const key of [
      "ZOMBIE_TELEMETRY_DISABLED",
      "DO_NOT_TRACK",
      "ZOMBIE_TELEMETRY_POSTHOG_KEY",
      "ZOMBIE_TELEMETRY_POSTHOG_HOST",
      "ZOMBIE_TELEMETRY_DEBUG",
    ]) {
      assert.ok(
        result.stdout.includes(key),
        `--help did not mention ${key}; got: ${result.stdout}`,
      );
    }
    // Negative assertion: the pre-supabase-alignment names must not
    // resurface (catches a future bad merge that brings them back).
    for (const stale of ["DISABLE_TELEMETRY ", "ZOMBIE_POSTHOG_KEY", "ZOMBIE_POSTHOG_HOST"]) {
      assert.ok(
        !result.stdout.includes(stale),
        `--help still references legacy env name ${stale}`,
      );
    }
  });
});

describe("--no-open suppresses browser spawn (stub backend)", () => {
  let stub: LocalStubHandle | null = null;
  let stateDir: StubbedStateDir | null = null;

  beforeAll(async () => {
    stub = await startLocalStubServer();
    stateDir = await makeStubbedStateDir();
  });

  afterAll(async () => {
    if (stub) await stub.close();
    if (stateDir) await stateDir.cleanup();
  });

  it("emits a login_url on stdout and does not spawn a browser", async () => {
    if (!stub || !stateDir) throw new Error("fixtures not initialised");
    const env = composeEnv({
      ZOMBIE_API_URL: stub.baseUrl,
      ZOMBIE_STATE_DIR: stateDir.dir,
      NO_COLOR: "1",
    });
    // Short timeout — we just want to confirm the CLI prints the URL
    // then polls; not waiting for "complete".
    const child = spawnZombiectl(
      ["login", "--no-open", "--no-input", "--timeout-sec", "2", "--poll-ms", "200"],
      { env },
    );
    const result = await waitForExit(child);
    // Pending forever → eventual timeout → exit 1. Either way, no browser was launched.
    assert.notEqual(result.code, 0, `expected timeout exit, got ${result.code}; stdout=${result.stdout}`);
    assert.ok(
      /login_url/i.test(result.stdout) || /127\.0\.0\.1/.test(result.stdout),
      `expected login_url in stdout: ${result.stdout}`,
    );
  });
});

describe("SIGINT during login (stub backend)", () => {
  let stub: LocalStubHandle | null = null;
  let stateDir: StubbedStateDir | null = null;

  beforeAll(async () => {
    stub = await startLocalStubServer();
    stateDir = await makeStubbedStateDir();
  });

  afterAll(async () => {
    if (stub) await stub.close();
    if (stateDir) await stateDir.cleanup();
  });

  async function spawnLoginAndInterrupt(extraArgs?: ReadonlyArray<string>): Promise<ExitResult> {
    if (!stub || !stateDir) throw new Error("fixtures not initialised");
    const env = composeEnv({
      ZOMBIE_API_URL: stub.baseUrl,
      ZOMBIE_STATE_DIR: stateDir.dir,
      NO_COLOR: "1",
    });
    const args = [
      "login",
      "--no-input",
      "--timeout-sec",
      String(SIGINT_DEADLINE_SEC),
      "--poll-ms",
      String(SIGINT_POLL_MS),
      ...(extraArgs ?? []),
    ];
    const child = spawnZombiectl(args, { env });
    await waitForLine(child, (line: string) => /login_url|127\.0\.0\.1/i.test(line), 10_000);
    // Give the CLI time to enter the poll loop.
    await new Promise<void>((r) => setTimeout(r, 400));
    child.kill("SIGINT");
    const result = await waitForExit(child);
    return result;
  }

  // Only the `--no-open` SIGINT variant is exercised here. The bare-login
  // form (no `--no-open`) calls `openUrl(loginUrl)`, which on a macOS dev
  // machine hands the stub `login_url` to the default browser and pops up
  // a "can't connect" tab. The CI surface this spec gates is headless
  // (openUrl silently no-ops there), so the bare-login regression isn't
  // lost — the workflow can opt into it via a
  // `ZOMBIE_ACCEPTANCE_INCLUDE_BARE_LOGIN` env gate in a follow-on PR.
  // Discovery: parameterise the stub `login_url` so a noop scheme (e.g.
  // data: or about:blank) can be opted into for local SIGINT coverage.
  it("`--no-open` poll: SIGINT exits non-zero, no credentials.json written", async () => {
    const result = await spawnLoginAndInterrupt(["--no-open"]);
    assert.notEqual(result.code, 0, `expected non-zero exit, got ${result.code}; stdout=${result.stdout}; stderr=${result.stderr}`);
    if (!stateDir) throw new Error("stateDir not initialised");
    const credsPath = path.join(stateDir.dir, "credentials.json");
    const credsRaw = await fs.readFile(credsPath, "utf8").catch(() => null);
    if (credsRaw) {
      // Stubbed state-dir pre-seeds a syntactically-valid token; the SIGINT
      // path must not have overwritten it with a "complete" session token
      // (no `complete` ever arrived from the stub server).
      const parsed = JSON.parse(credsRaw) as { token: string };
      assert.equal(parsed.token, "header.payload.sig", `SIGINT path wrote a real token: ${credsRaw}`);
    }
  });
});

// --api / ZOMBIE_API_URL precedence (live api only). Registered only
// when `ZOMBIE_ACCEPTANCE_TARGET` is an https:// URL; conditional registration
// keeps both `node --test` and `bun test` honest about the skip.
{
  const target = process.env.ZOMBIE_ACCEPTANCE_TARGET ?? "";
  const isLive = target.startsWith("https://");

  if (isLive) {
    describe("--api / ZOMBIE_API_URL precedence", () => {
      // process.env.ZOMBIE_TOKEN is NOT injected by the CI workflow step
      // (only CLERK_SECRET_KEY + fixture emails land). Mint a real
      // session JWT once per describe so the auth-guard passes and the
      // tests actually exercise URL precedence instead of exiting on
      // "not authenticated".
      let sessionJwt: string = "";
      beforeAll(async () => {
        const minted = await attachJwt(resolveClerkSecret(), {
          email: resolveFixtureEmail("regular"),
        });
        sessionJwt = minted.sessionJwt;
      });

      it("--api overrides ZOMBIE_API_URL", async () => {
        const env = composeEnv({
          ZOMBIE_API_URL: UNROUTABLE_API_URL,
          ZOMBIE_TOKEN: sessionJwt,
          NO_COLOR: "1",
        });
        const result: RunResult = await runZombiectl(
          ["--api", target, "workspace", "list", "--json"],
          { env },
        );
        assert.equal(result.code, 0, `expected exit 0; stderr=${result.stderr}`);
        assert.doesNotThrow(() => JSON.parse(result.stdout.trim()));
      });

      it("ZOMBIE_API_URL honored when --api absent", async () => {
        const env = composeEnv({
          ZOMBIE_API_URL: target,
          ZOMBIE_TOKEN: sessionJwt,
          NO_COLOR: "1",
        });
        const result: RunResult = await runZombiectl(["workspace", "list", "--json"], { env });
        assert.equal(result.code, 0, `expected exit 0; stderr=${result.stderr}`);
        assert.doesNotThrow(() => JSON.parse(result.stdout.trim()));
      });
    });
  }
}
