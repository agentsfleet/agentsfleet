/** Native Chromium + real incremental loopback SSE, not a FakeEventSource test.
 * This proves browser transport/registry wiring; Rust tests own server keepalives.
 * No Clerk, deployment, datastore, or production application writes. */
// Playwright workers run under Node, so its child-process and HTTP APIs own
// this fixture's process/socket lifecycle; Bun only builds the browser entry.
import { execFile } from "node:child_process";
import http from "node:http";
import type { AddressInfo } from "node:net";
import path from "node:path";
import { promisify } from "node:util";
import { expect, test, type Page, type TestInfo } from "@playwright/test";

const HEARTBEAT = 'event: heartbeat\ndata: {"kind":"heartbeat"}\n\n';
const OBSERVATION_TIMEOUT_MS = 60_000;
const TEST_TIMEOUT_MS = 100_000;
const HEARTBEAT_SPACING_MS = 15_100;
const BUNDLE_TIMEOUT_MS = 20_000;
const APP_ROOT = path.resolve(import.meta.dirname, "../..");
let bundle: string;
let server: http.Server;
let origin: string;
let streams: http.ServerResponse[];
let active: Set<http.ServerResponse>;
let peakActive: number;
let historyReads: number;

test.beforeAll(async () => {
  const built = await promisify(execFile)("bun", [
    "build", "tests/e2e/fixtures/fleet-stream-browser-probe.ts", "--target=browser", "--format=iife",
  ], { cwd: APP_ROOT, maxBuffer: 2_000_000, timeout: BUNDLE_TIMEOUT_MS });
  bundle = built.stdout;
});

test.beforeEach(async () => {
  streams = [];
  active = new Set();
  peakActive = 0;
  historyReads = 0;
  server = http.createServer((req, res) => {
    if (req.url?.endsWith("/stream")) {
      res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
      res.flushHeaders();
      streams.push(res);
      active.add(res);
      peakActive = Math.max(peakActive, active.size);
      res.on("close", () => active.delete(res));
    } else if (req.url?.startsWith("/live/")) {
      historyReads += 1;
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ items: [], next_cursor: null }));
    } else if (req.url === "/probe.js") {
      res.writeHead(200, { "content-type": "text/javascript" });
      res.end(bundle);
    } else {
      res.writeHead(200, { "content-type": "text/html" });
      res.end('<button id="start">Start</button><button id="stop">Stop</button><output></output><pre></pre><script src="/probe.js"></script>');
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

test.afterEach(async ({ page }) => {
  await page.goto("about:blank");
  server.closeAllConnections();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  expect(active.size).toBe(0);
});

async function startProbe(page: Page): Promise<void> {
  await page.goto(origin);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect.poll(() => streams.length).toBe(1);
  // HTTP headers alone must never turn the connection green.
  await expect(page.locator("output")).toHaveText("connecting");
}

function currentStream(): http.ServerResponse {
  const stream = streams.at(-1);
  if (!stream) throw new Error("No native EventSource request arrived");
  return stream;
}

test("named heartbeat dispatch and quiet EOF renew without status flicker or duplicate connections", async ({ page }, testInfo) => {
  test.setTimeout(TEST_TIMEOUT_MS);
  const started = performance.now();
  await startProbe(page);
  currentStream().write(HEARTBEAT);
  await expect(page.locator("output")).toHaveText("live");
  // Real elapsed time and separately flushed bytes establish stable liveness.
  for (let beat = 0; beat < 2; beat += 1) {
    await new Promise<void>((resolve) => setTimeout(resolve, HEARTBEAT_SPACING_MS));
    currentStream().write(HEARTBEAT);
  }
  // TCP preserves the heartbeat bytes before this clean EOF.
  currentStream().end();
  await expect.poll(() => streams.length).toBe(2);
  await expect(page.locator("output")).toHaveText("live");
  // History recovery starts on transport open; heartbeat alone proves LIVE.
  await expect.poll(() => historyReads).toBe(1);
  currentStream().write(HEARTBEAT);
  await expect.poll(() => historyReads).toBe(1);
  await expect(page.locator("pre")).toHaveText('["connecting","live"]');
  expect(peakActive).toBe(1);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(() => active.size).toBe(0);
  expect(streams).toHaveLength(2);
  await evidence(testInfo, started);
});

test("an open but silent native stream is replaced automatically and resumes on heartbeat", async ({ page }, testInfo) => {
  test.setTimeout(TEST_TIMEOUT_MS);
  const started = performance.now();
  await startProbe(page);
  currentStream().write(HEARTBEAT);
  await expect(page.locator("output")).toHaveText("live");
  // Leave the HTTP socket OPEN: no EOF/error and no more keepalive bytes.
  await expect.poll(() => streams.length, { timeout: OBSERVATION_TIMEOUT_MS }).toBe(2);
  await expect(page.locator("output")).toHaveText("reconnecting");
  currentStream().write(HEARTBEAT);
  await expect(page.locator("output")).toHaveText("live");
  await expect.poll(() => historyReads).toBe(1);
  expect(peakActive).toBe(1);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(() => active.size).toBe(0);
  await evidence(testInfo, started);
});

async function evidence(info: TestInfo, started: number): Promise<void> {
  await info.attach("native-eventsource-evidence", {
    contentType: "application/json",
    body: JSON.stringify({
      transport: "native Chromium EventSource over incremental loopback HTTP",
      clock: "real elapsed time, no fake timers",
      elapsedMs: performance.now() - started,
      streamRequests: streams.length, peakActive, activeAfterCleanup: active.size, historyReads,
    }),
  });
}
