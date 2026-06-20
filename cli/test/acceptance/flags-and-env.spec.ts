/**
 * Cross-cutting flag / env-var / signal-handling acceptance scenario.
 *
 * Covered here:
 *   - global-flag matrix and precedence (no backend needed)
 *   - NO_COLOR semantics (no backend needed)
 *   - a spawned (non-TTY) `agentsfleet login` with no token fast-fails:
 *     no browser, no partial credentials.json (token-only contract)
 *
 * Live-API precedence tests (--api / AGENTSFLEET_API_URL, env-var auth
 * permutations) only register when `AGENTSFLEET_ACCEPTANCE_TARGET` is an
 * https:// URL — they run against real api-dev / api in CI, not local.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import { runFleetctl, spawnFleetctl, composeEnv } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";
import { resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";

import type { ChildProcessWithoutNullStreams } from "node:child_process";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const CLI_ROOT = path.resolve(HERE, "..", "..");
const ANSI_RE = /\x1b\[/;

interface ExitResult {
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
  readonly stdout: string;
  readonly stderr: string;
}

let pkgVersion: string;

beforeAll(async () => {
  const pkgRaw = await fs.readFile(path.join(CLI_ROOT, "package.json"), "utf8");
  pkgVersion = (JSON.parse(pkgRaw) as { version: string }).version;
});

function emptyEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({ AGENTSFLEET_API_URL: UNROUTABLE_API_URL, NO_COLOR: "1", ...(extra ?? {}) });
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

describe("global flag matrix", () => {
  it("--version --help → --version wins (precedence)", async () => {
    const result = await runFleetctl(["--version", "--help"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion), `expected version ${pkgVersion} in stdout: ${result.stdout}`);
    assert.ok(!/usage|commands:/i.test(result.stdout), `--help body leaked into --version output: ${result.stdout}`);
  });

  it("--help --version → --version still wins (order-independent)", async () => {
    const result = await runFleetctl(["--help", "--version"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion));
    assert.ok(!/usage|commands:/i.test(result.stdout));
  });

  // --help --json exits 0 and emits help output. A structured JSON help
  // tree (Discovery row) is a future enhancement; commander's text body
  // is what --json currently returns. Assert exit 0 with help content
  // present — the contract the CLI ships.
  it("--help --json exits 0 with help content", async () => {
    const result = await runFleetctl(["--help", "--json"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.length > 0);
    assert.match(result.stdout, /agentsfleet|usage|commands/i);
  });
});

describe("NO_COLOR semantics", () => {
  it("NO_COLOR=1 strips ANSI escapes from --help", async () => {
    const result = await runFleetctl(["--help"], {
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
  it("advertises AGENTSFLEET_TELEMETRY_DISABLED, DO_NOT_TRACK and POSTHOG knobs", async () => {
    const result = await runFleetctl(["--help"], {
      env: composeEnv({ NO_COLOR: "1" }),
    });
    assert.equal(result.code, 0);
    for (const key of [
      "AGENTSFLEET_TELEMETRY_DISABLED",
      "DO_NOT_TRACK",
      "AGENTSFLEET_TELEMETRY_POSTHOG_KEY",
      "AGENTSFLEET_TELEMETRY_POSTHOG_HOST",
      "AGENTSFLEET_TELEMETRY_DEBUG",
    ]) {
      assert.ok(
        result.stdout.includes(key),
        `--help did not mention ${key}; got: ${result.stdout}`,
      );
    }
    // Negative assertion: the pre-supabase-alignment names must not
    // resurface (catches a future bad merge that brings them back).
    for (const stale of ["DISABLE_TELEMETRY ", "AGENTSFLEET_POSTHOG_KEY", "AGENTSFLEET_POSTHOG_HOST"]) {
      assert.ok(
        !result.stdout.includes(stale),
        `--help still references legacy env name ${stale}`,
      );
    }
  });
});

describe("non-TTY login fast-fails", () => {
  // A spawned `agentsfleet login` inherits a piped (non-TTY) stdin. Per the
  // non-interactive contract (no token + non-TTY → token-only), it aborts
  // before the browser device flow — never opening a browser and never
  // writing a partial credentials.json. The device-flow + SIGINT-abort
  // mechanics are unit-covered (login-device-flow / login-effect); driving
  // the device flow through a spawned binary would need a PTY harness.
  let stateDir: StubbedStateDir | null = null;

  beforeAll(async () => {
    stateDir = await makeStubbedStateDir();
  });

  afterAll(async () => {
    if (stateDir) await stateDir.cleanup();
  });

  // --force skips the D20 idempotency gate (makeStubbedStateDir pre-seeds a
  // credential), so the run reaches the token resolve, where a non-TTY shell
  // with no token aborts. Ending the child's stdin gives that read an EOF
  // instead of a hang. The API is never contacted — the abort precedes it.
  function spawnNonTtyLogin(): ChildProcessWithoutNullStreams {
    if (!stateDir) throw new Error("fixtures not initialised");
    const env = composeEnv({
      AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
      AGENTSFLEET_STATE_DIR: stateDir.dir,
      NO_COLOR: "1",
    });
    const child = spawnFleetctl(["login", "--no-open", "--force"], { env });
    child.stdin.end();
    return child;
  }

  it("exits non-zero with token guidance and never opens a browser", async () => {
    const result = await waitForExit(spawnNonTtyLogin());
    assert.notEqual(result.code, 0, `expected fast-fail, got ${result.code}; stderr=${result.stderr}`);
    assert.match(result.stderr, /--token/, `expected token guidance: ${result.stderr}`);
    assert.ok(
      !/login_url|127\.0\.0\.1/i.test(result.stdout),
      `device flow must not start: ${result.stdout}`,
    );
  });

  it("leaves a pre-existing credentials.json untouched", async () => {
    if (!stateDir) throw new Error("fixtures not initialised");
    await waitForExit(spawnNonTtyLogin());
    const credsRaw = await fs
      .readFile(path.join(stateDir.dir, "credentials.json"), "utf8")
      .catch(() => null);
    if (credsRaw) {
      const parsed = JSON.parse(credsRaw) as { token: string };
      assert.equal(parsed.token, "header.payload.sig", `fast-fail wrote a token: ${credsRaw}`);
    }
  });
});

// --api / AGENTSFLEET_API_URL precedence (live api only). Registered only
// when `AGENTSFLEET_ACCEPTANCE_TARGET` is an https:// URL; conditional registration
// keeps both `node --test` and `bun test` honest about the skip.
{
  const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
  const isLive = target.startsWith("https://");

  if (isLive) {
    describe("--api / AGENTSFLEET_API_URL precedence", () => {
      // process.env.AGENTSFLEET_API_KEY is NOT injected by the CI workflow
      // step (only CLERK_SECRET_KEY + fixture emails land). Mint a real
      // session JWT once per describe and pass it via the env credential slot
      // (a non-`agt_t` JWT routes to the server's OIDC verifier) so the
      // auth-guard passes and the tests actually exercise URL precedence
      // instead of exiting on "not authenticated".
      let sessionJwt: string = "";
      beforeAll(async () => {
        const minted = await attachJwt(resolveClerkSecret(), {
          email: resolveFixtureEmail("regular"),
        });
        sessionJwt = minted.sessionJwt;
      });

      it("--api overrides AGENTSFLEET_API_URL", async () => {
        const env = composeEnv({
          AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
          AGENTSFLEET_API_KEY: sessionJwt,
          NO_COLOR: "1",
        });
        const result: RunResult = await runFleetctl(
          ["--api", target, "workspace", "list", "--json"],
          { env },
        );
        assert.equal(result.code, 0, `expected exit 0; stderr=${result.stderr}`);
        assert.doesNotThrow(() => JSON.parse(result.stdout.trim()));
      });

      it("AGENTSFLEET_API_URL honored when --api absent", async () => {
        const env = composeEnv({
          AGENTSFLEET_API_URL: target,
          AGENTSFLEET_API_KEY: sessionJwt,
          NO_COLOR: "1",
        });
        const result: RunResult = await runFleetctl(["workspace", "list", "--json"], { env });
        assert.equal(result.code, 0, `expected exit 0; stderr=${result.stderr}`);
        assert.doesNotThrow(() => JSON.parse(result.stdout.trim()));
      });
    });
  }
}
