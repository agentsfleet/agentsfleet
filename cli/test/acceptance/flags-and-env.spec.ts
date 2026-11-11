/**
 * Cross-cutting flag / env-var / signal-handling acceptance scenario.
 *
 * Covered here:
 *   - global-flag matrix and precedence (no backend needed)
 *   - NO_COLOR semantics (no backend needed)
 *   - a spawned (non-TTY) `agentsfleet login` with no token fast-fails:
 *     no browser, no partial credentials.json (token-only contract)
 *
 * API URL precedence uses a local HTTP server and an isolated credential
 * store, so both cases run without a live target or Clerk credentials.
 */

import { describe, it, beforeAll, beforeEach, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import type { AddressInfo } from "node:net";
import path from "node:path";
import { VERSION } from "../../src/cli.ts";
import { helpTail } from "../../src/program/entry/help-formatter.ts";

import { runFleetctl, spawnFleetctl, composeEnv } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import {
  makeStubbedStateDir,
  STUB_TOKEN,
  type StubbedStateDir,
} from "./fixtures/state-dir.ts";

import type { ChildProcessWithoutNullStreams } from "node:child_process";

const ANSI_RE = /\x1b\[/;

interface ExitResult {
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
  readonly stdout: string;
  readonly stderr: string;
}

function emptyEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({
    AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
    NO_COLOR: "1",
    ...(extra ?? {}),
  });
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
    assert.ok(result.stdout.includes(VERSION), `expected version ${VERSION} in stdout: ${result.stdout}`);
    assert.ok(!/usage|commands:/i.test(result.stdout), `--help body leaked into --version output: ${result.stdout}`);
  });

  it("--help --version → --version still wins (order-independent)", async () => {
    const result = await runFleetctl(["--help", "--version"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(VERSION));
    assert.ok(!/usage|commands:/i.test(result.stdout));
  });

  // --help --json exits 0 and emits help output. A structured JSON help
  // tree (Discovery row) is a future enhancement; the renderer's text body
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

describe("env-var reference (--help)", () => {
  // --help stays terse: the full environment-variable matrix (telemetry
  // opt-out included) lives in the docs, surfaced as a single docs pointer
  // (refactor: inline matrix → docs link). This confirms the built binary
  // (npm install / dist) renders that pointer end-to-end. `helpTail()` is
  // the single source.
  it("points to the env-var docs instead of inlining the matrix", async () => {
    const result = await runFleetctl(["--help"], {
      env: composeEnv({ NO_COLOR: "1" }),
    });
    assert.equal(result.code, 0);
    const tail = helpTail().trim();
    assert.ok(
      result.stdout.includes(tail),
      `--help did not surface the env-var docs pointer (${tail}); got: ${result.stdout}`,
    );
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

  // Since direct-token seeding was retired, the guidance a non-TTY
  // shell gets must name the environment variable rather than the removed
  // flag — otherwise the fast-fail points at something that no longer exists.
  it("exits non-zero naming AGENTSFLEET_API_KEY and never opens a browser", async () => {
    const result = await waitForExit(spawnNonTtyLogin());
    assert.notEqual(result.code, 0, `expected fast-fail, got ${result.code}; stderr=${result.stderr}`);
    assert.match(result.stderr, /AGENTSFLEET_API_KEY/, `expected env-var guidance: ${result.stderr}`);
    assert.doesNotMatch(result.stderr, /--token\b/, `must not point at the removed flag: ${result.stderr}`);
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
      assert.equal(parsed.token, STUB_TOKEN, `fast-fail wrote a token: ${credsRaw}`);
    }
  });
});

describe("--api / AGENTSFLEET_API_URL precedence", () => {
  // Synthetic identity served only by the local HTTP fixture; no live account is used.
  const identity = {user_id: "0193c5e1-0000-7000-8000-00000000abcd",
    tenant_id: "0193c5e0-0000-7000-8000-000000001234", email: "routing@example.test",
    display_name: null, tenant_name: "Routing probe", credential: "cli_credential", scopes: []};
  const requests: Array<{url: string; authorization: string}> = [];
  const server = http.createServer((request, response) => {
    requests.push({url: request.url ?? "", authorization: request.headers.authorization ?? ""});
    response.writeHead(200, {"content-type": "application/json"});
    response.end(JSON.stringify(identity));
  });
  let state: StubbedStateDir;
  let target = "";

  beforeAll(async () => {
    state = await makeStubbedStateDir();
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    target = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  afterAll(async () => {
    await new Promise<void>(resolve => server.close(() => resolve()));
    await state.cleanup();
  });

  beforeEach(() => { requests.length = 0; });

  it("--api overrides AGENTSFLEET_API_URL", async () => {
    const env = composeEnv({AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
      AGENTSFLEET_STATE_DIR: state.dir, NO_PROXY: "localhost,127.0.0.1", NO_COLOR: "1"});
    const result = await runFleetctl(["--api", target, "whoami", "--json"], {env});
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), {...identity, authenticated: true, source: "file", api_url: target});
    assert.deepEqual(requests.splice(0), [{url: "/v1/users/me", authorization: `Bearer ${STUB_TOKEN}`}]);
  });

  it("AGENTSFLEET_API_URL honored when --api absent", async () => {
    const env = composeEnv({AGENTSFLEET_API_URL: target,
      AGENTSFLEET_STATE_DIR: state.dir, NO_PROXY: "localhost,127.0.0.1", NO_COLOR: "1"});
    const result = await runFleetctl(["whoami", "--json"], {env});
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), {...identity, authenticated: true, source: "file", api_url: target});
    assert.deepEqual(requests.splice(0), [{url: "/v1/users/me", authorization: `Bearer ${STUB_TOKEN}`}]);
  });
});
