/**
 * Retry-policy acceptance: the built binary against a scripted loopback
 * server that answers the way a flaky backend does. Deterministic — no
 * credentials, no network beyond 127.0.0.1 — and the assertions are the
 * server's request log, not the CLI's prose.
 *
 *   - a read recovers from a 503 blip: two round-trips, exit 0
 *   - a read whose socket reset after sending is read again
 *   - a write whose socket reset after sending is sent exactly once
 */
import { afterAll, afterEach, beforeAll, describe, it } from "bun:test";
import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";

const LOOPBACK = "127.0.0.1";
const HTTP_OK = 200;
const HTTP_SERVICE_UNAVAILABLE = 503;
const EXIT_OK = 0;
const CONTENT_TYPE_JSON = "application/json";
const EMPTY_LIST_BODY = JSON.stringify({ items: [] });
const API_KEY_CREATE_ARGS = ["api-key", "create", "--name", "retry-probe"];
const FLEET_LIST_ARGS = ["list"];
const API_KEYS_PATH = "/v1/api-keys";
const STACK_FRAME_RE = /\n\s+at\s+\S+/;

type Step = { readonly status: number; readonly body?: string } | { readonly reset: true };

const queue: Step[] = [];
const requests: string[] = [];
let server: http.Server;
let apiUrl: string;
let stubState: StubbedStateDir;

beforeAll(async () => {
  server = http.createServer((req, res) => {
    requests.push(`${req.method} ${req.url}`);
    const step = queue.shift() ?? { status: HTTP_OK, body: EMPTY_LIST_BODY };
    if ("reset" in step) {
      // The request was on the wire; the answer never comes.
      req.socket.destroy();
      return;
    }
    res.writeHead(step.status, { "content-type": CONTENT_TYPE_JSON });
    res.end(step.body ?? EMPTY_LIST_BODY);
  });
  await new Promise<void>((resolve) => server.listen(0, LOOPBACK, resolve));
  apiUrl = `http://${LOOPBACK}:${(server.address() as AddressInfo).port}`;
  stubState = await makeStubbedStateDir();
});

afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await stubState.cleanup();
});

afterEach(() => {
  queue.length = 0;
  requests.length = 0;
});

function env(): Record<string, string> {
  return composeEnv({
    AGENTSFLEET_API_URL: apiUrl,
    AGENTSFLEET_STATE_DIR: stubState.dir,
    NO_COLOR: "1",
  });
}

function methods(): string[] {
  return requests.map((line) => line.split(" ")[0] ?? "");
}

describe("retry policy over a real socket", () => {
  it("a read recovers from a blip: the 503 is retried and the list arrives", async () => {
    queue.push({ status: HTTP_SERVICE_UNAVAILABLE, body: JSON.stringify({ error: { code: "HTTP_503" } }) });
    queue.push({ status: HTTP_OK, body: EMPTY_LIST_BODY });
    const result = await runFleetctl([...FLEET_LIST_ARGS], { env: env() });
    assert.equal(result.code, EXIT_OK, `${result.stdout}\n${result.stderr}`);
    assert.deepEqual(methods(), ["GET", "GET"]);
  });

  it("a read whose socket reset after sending is read again", async () => {
    queue.push({ reset: true });
    queue.push({ status: HTTP_OK, body: EMPTY_LIST_BODY });
    const result = await runFleetctl([...FLEET_LIST_ARGS], { env: env() });
    assert.equal(result.code, EXIT_OK, `${result.stdout}\n${result.stderr}`);
    assert.deepEqual(methods(), ["GET", "GET"]);
  });

  it("a write whose socket reset after sending is sent exactly once", async () => {
    queue.push({ reset: true });
    const result = await runFleetctl([...API_KEY_CREATE_ARGS], { env: env() });
    assert.notEqual(result.code, EXIT_OK, "a write the server may hold must not be reported as done");
    assert.deepEqual(requests, [`POST ${API_KEYS_PATH}`]);
    const output = `${result.stdout}\n${result.stderr}`;
    assert.ok(!STACK_FRAME_RE.test(output), `stack frames leaked into operator output: ${output}`);
  });
});
