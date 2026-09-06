/**
 * retry-policy.spec.ts — the retry policy over a real socket, for both
 * runtimes an operator uses: the dashboard's server-side transport and the
 * command line. A scripted loopback server answers the way a flaky backend
 * does; the assertions are its request log and the clock, not prose.
 *
 *   - a dashboard read recovers from a 503 blip in two round-trips
 *   - a Retry-After the policy will not honour fails the read at once
 *   - a dashboard write whose socket reset after sending is sent once
 *   - the command line makes the same decisions against the same server
 *
 * The dashboard fetches server-side (docs/architecture/web_app.md, statement
 * 1), so the transport is driven against the scripted origin in a child
 * process (fixtures/retry-probe.ts) whose NEXT_PUBLIC_API_URL names that
 * origin: the transport captures its origin at module load, and a worker that
 * already loaded it for another spec would otherwise keep the live backend.
 */
import { spawn } from "node:child_process";
import * as fs from "node:fs/promises";
import http from "node:http";
import type { AddressInfo } from "node:net";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { cliEnv, makeCliStateDir, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";

const PROBE = path.join(import.meta.dirname, "fixtures", "retry-probe.ts");
const APP_ROOT = path.resolve(import.meta.dirname, "..", "..", "..");
const PROBE_TIMEOUT_MS = 60_000;

type Probe = { settled: "answered" | "failed"; status: number | undefined; name: string | undefined; elapsedMs: number };

function probe(scenario: "read" | "write"): Promise<Probe> {
  return new Promise((resolve, reject) => {
    const child = spawn("bun", ["run", PROBE, scenario], {
      cwd: APP_ROOT,
      env: { ...process.env, NEXT_PUBLIC_API_URL: apiUrl, AGENTSFLEET_NO_RETRY: "" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => child.kill(), PROBE_TIMEOUT_MS);
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) reject(new Error(`retry probe exited ${code}: ${stderr}`));
      else resolve(JSON.parse(stdout.trim()) as Probe);
    });
  });
}

const LOOPBACK = "127.0.0.1";
const HTTP_OK = 200;
const HTTP_SERVICE_UNAVAILABLE = 503;
const HTTP_TOO_MANY_REQUESTS = 429;
const RETRY_AFTER_TOO_LONG_SECONDS = "120";
const FAIL_FAST_BUDGET_MS = 2_000;
const OK_BODY = JSON.stringify({ ok: true });
const EMPTY_LIST_BODY = JSON.stringify({ items: [] });
const CONTENT_TYPE_JSON = "application/json";
const TEMP_DIR_PREFIX = "agentsfleet-retry-policy-";
const WORKSPACE_ID = "ws_retry_policy";
const WORKSPACE_NAME = "retry-policy";
const STUB_TOKEN = `afc_${"e".repeat(64)}`;
const EXIT_OK = 0;
const API_KEYS_PATH = "/v1/api-keys";

type Step = { readonly status: number; readonly body?: string; readonly headers?: Record<string, string> } | { readonly reset: true };

const queue: Step[] = [];
const requests: string[] = [];
let server: http.Server;
let apiUrl: string;

function methods(): string[] {
  return requests.map((line) => line.split(" ")[0] ?? "");
}

test.beforeAll(async () => {
  server = http.createServer((req, res) => {
    requests.push(`${req.method} ${req.url}`);
    const step = queue.shift() ?? { status: HTTP_OK, body: OK_BODY };
    if ("reset" in step) {
      req.socket.destroy();
      return;
    }
    res.writeHead(step.status, { "content-type": CONTENT_TYPE_JSON, ...(step.headers ?? {}) });
    res.end(step.body ?? OK_BODY);
  });
  await new Promise<void>((resolve) => server.listen(0, LOOPBACK, resolve));
  apiUrl = `http://${LOOPBACK}:${(server.address() as AddressInfo).port}`;
});

test.afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

test.beforeEach(() => {
  queue.length = 0;
  requests.length = 0;
});

test.describe("the dashboard's transport over a real socket", () => {
  test("a read recovers from a blip in two round-trips", async () => {
    queue.push({ status: HTTP_SERVICE_UNAVAILABLE, body: OK_BODY }, { status: HTTP_OK, body: OK_BODY });
    const outcome = await probe("read");
    expect(outcome.settled).toBe("answered");
    expect(methods()).toEqual(["GET", "GET"]);
  });

  test("a Retry-After the policy will not honour fails the read at once", async () => {
    queue.push({ status: HTTP_TOO_MANY_REQUESTS, body: OK_BODY, headers: { "retry-after": RETRY_AFTER_TOO_LONG_SECONDS } });
    const outcome = await probe("read");
    expect(outcome).toMatchObject({ settled: "failed", status: HTTP_TOO_MANY_REQUESTS });
    expect(outcome.elapsedMs).toBeLessThan(FAIL_FAST_BUDGET_MS);
    expect(methods()).toEqual(["GET"]);
  });

  test("a write whose socket reset after sending is sent once, even when its caller asked for retries", async () => {
    queue.push({ reset: true });
    const outcome = await probe("write");
    expect(outcome).toMatchObject({ settled: "failed", name: "TypeError" });
    expect(methods()).toEqual(["POST"]);
  });
});

test.describe("the command line makes the same decisions", () => {
  test("a read recovers from a blip; a write reset after sending is sent once", async () => {
    const { root, stateDir } = await makeCliStateDir(TEMP_DIR_PREFIX);
    try {
      await writeCliState(stateDir, WORKSPACE_ID, STUB_TOKEN, apiUrl, WORKSPACE_NAME);
      const env = cliEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir });

      queue.push({ status: HTTP_SERVICE_UNAVAILABLE, body: EMPTY_LIST_BODY }, { status: HTTP_OK, body: EMPTY_LIST_BODY });
      const listed = await spawnAgentsfleet(["list"], env);
      expect(listed.code, `${listed.stdout}\n${listed.stderr}`).toBe(EXIT_OK);
      expect(methods()).toEqual(["GET", "GET"]);

      queue.length = 0;
      requests.length = 0;
      queue.push({ reset: true });
      const created = await spawnAgentsfleet(["api-key", "create", "--name", "retry-probe"], env);
      expect(created.code).not.toBe(EXIT_OK);
      expect(requests).toEqual([`POST ${API_KEYS_PATH}`]);
    } finally {
      await fs.rm(root, { recursive: true, force: true });
    }
  });
});
