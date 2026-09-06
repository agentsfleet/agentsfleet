import { afterEach, describe, expect, it } from "bun:test";

import { composeEnv, runFleetctl, spawnFleetctl } from "./fixtures/cli.js";
import { makeStubbedStateDir } from "./fixtures/state-dir.ts";

const WORKSPACE_ID = "01910000-0000-7000-8000-000000a6e711";
const FLEET_ID = "01910000-0000-7000-8000-000000a67e57";
const EVENT_ID = "1729874000000-acceptance";
// A well-formed command-line credential: the load check refuses anything
// else, so a session-token-shaped stub reads as logged out.
const TEST_TOKEN = `afc_${"f".repeat(64)}`;
const STEER_COMMAND = "steer";
const JSON_FLAG = "--json";
const TEST_MESSAGE = "return one short acknowledgement";
const STATUS_PROCESSED = "processed";
const STATUS_FLEET_ERROR = "fleet_error";
const KIND_COMPLETE = "complete";
const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_SSE = "text/event-stream";
const HTTP_ACCEPTED = 202;
const HTTP_OK = 200;
const PHASE_TIMEOUT_MS = 10_000;
const TEST_TIMEOUT_MS = 15_000;
const LIVE_CHUNK = "incremental reply observed before completion";
const UNRELATED_EVENT_ID = "unrelated";
const UNRELATED_CHUNK = "another fleets reply must not print";
const FRAME = {
  received: "event_received", gate: "gate_opened", chunk: "chunk", complete: "event_complete",
} as const;
const encoder = new TextEncoder();
const SERVER_MODE = {
  sseTerminal: "sse_terminal",
  fallbackTerminal: "fallback_terminal",
  explicitFailure: "explicit_failure",
} as const;

type ServerMode = (typeof SERVER_MODE)[keyof typeof SERVER_MODE];

interface RequestCounts {
  messages: number;
  streams: number;
  polls: number;
}

interface StubServer {
  readonly baseUrl: string;
  readonly counts: RequestCounts;
  stop(): Promise<void>;
}

let cleanups: Array<() => Promise<void>> = [];

afterEach(async () => {
  const pending = cleanups;
  cleanups = [];
  await Promise.all(pending.map((cleanup) => cleanup()));
});

function jsonResponse(body: unknown, status = HTTP_OK): Response {
  return Response.json(body, {
    status,
    headers: { "content-type": CONTENT_TYPE_JSON },
  });
}

function sseResponse(status: string): Response {
  const frame = `event: event_complete\ndata: ${JSON.stringify({
    event_id: EVENT_ID,
    status,
  })}\n\n`;
  return new Response(frame, {
    status: HTTP_OK,
    headers: { "content-type": CONTENT_TYPE_SSE },
  });
}

function requestKind(pathname: string): "messages" | "stream" | "poll" | "unknown" {
  if (pathname.endsWith("/messages")) return "messages";
  if (pathname.endsWith("/events/stream")) return "stream";
  if (pathname.endsWith("/events")) return "poll";
  return "unknown";
}

function startSteerStub(mode: ServerMode): StubServer {
  const counts: RequestCounts = { messages: 0, streams: 0, polls: 0 };
  const server = Bun.serve({
    port: 0,
    fetch(request) {
      const kind = requestKind(new URL(request.url).pathname);
      if (kind === "messages") {
        counts.messages++;
        return jsonResponse({ event_id: EVENT_ID }, HTTP_ACCEPTED);
      }
      if (kind === "stream") {
        counts.streams++;
        if (mode === SERVER_MODE.sseTerminal) return sseResponse(STATUS_PROCESSED);
        return new Response("", {
          status: HTTP_OK,
          headers: { "content-type": CONTENT_TYPE_SSE },
        });
      }
      if (kind === "poll") {
        counts.polls++;
        const status = mode === SERVER_MODE.explicitFailure
          ? STATUS_FLEET_ERROR
          : STATUS_PROCESSED;
        return jsonResponse({ items: [{ event_id: EVENT_ID, status }] });
      }
      return jsonResponse({ error: "not found" }, 404);
    },
  });
  return {
    baseUrl: server.url.origin,
    counts,
    stop: async () => {
      await server.stop(true);
    },
  };
}

async function runSteer(mode: ServerMode, jsonMode: boolean) {
  const server = startSteerStub(mode);
  cleanups.push(() => server.stop());
  const state = await makeStubbedStateDir({
    workspaceId: WORKSPACE_ID,
    token: TEST_TOKEN,
    apiUrl: server.baseUrl,
  });
  cleanups.push(() => state.cleanup());
  const env = composeEnv({
    AGENTSFLEET_API_URL: server.baseUrl,
    AGENTSFLEET_STATE_DIR: state.dir,
    NO_COLOR: "1",
  });
  const args = jsonMode
    ? [STEER_COMMAND, FLEET_ID, TEST_MESSAGE, JSON_FLAG]
    : [STEER_COMMAND, FLEET_ID, TEST_MESSAGE];
  return {
    result: await runFleetctl(args, { env }),
    counts: server.counts,
  };
}

// The server cannot finish until the subprocess has printed a chunk. A
// transport that buffers to EOF therefore fails at the owned deadline.
function startPhasedStub() {
  const counts: RequestCounts = { messages: 0, streams: 0, polls: 0 };
  let controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  const send = (kind: string, data: unknown) => {
    if (!controller) throw new Error("stream must open before the message posts");
    controller.enqueue(encoder.encode(`event: ${kind}\ndata: ${JSON.stringify(data)}\n\n`));
  };
  const server = Bun.serve({ port: 0, fetch(request) {
    const kind = requestKind(new URL(request.url).pathname);
    if (kind === "stream") {
      counts.streams++;
      return new Response(new ReadableStream<Uint8Array>({ start(next) {
        controller = next;
        next.enqueue(encoder.encode(": subscribed\n\n"));
      } }), { headers: { "content-type": CONTENT_TYPE_SSE } });
    }
    if (kind === "messages") {
      counts.messages++;
      send(FRAME.gate, { event_id: EVENT_ID, pending_approvals: 1 });
      send(FRAME.received, { event_id: EVENT_ID });
      send(FRAME.complete, null);
      send(FRAME.chunk, { event_id: EVENT_ID, text: null });
      send(FRAME.chunk, { event_id: UNRELATED_EVENT_ID, text: UNRELATED_CHUNK });
      send(FRAME.complete, { event_id: UNRELATED_EVENT_ID, status: STATUS_FLEET_ERROR });
      send(FRAME.chunk, { event_id: EVENT_ID, text: LIVE_CHUNK });
      return jsonResponse({ event_id: EVENT_ID }, HTTP_ACCEPTED);
    }
    if (kind === "poll") counts.polls++;
    return jsonResponse({ error: "unexpected request" }, 500);
  } });
  return { counts, send, baseUrl: server.url.origin, stop: () => server.stop(true) };
}

async function runPhasedSteer() {
  const server = startPhasedStub();
  cleanups.push(() => server.stop());
  const state = await makeStubbedStateDir({
    workspaceId: WORKSPACE_ID, token: TEST_TOKEN, apiUrl: server.baseUrl,
  });
  cleanups.push(() => state.cleanup());
  const env = composeEnv({ AGENTSFLEET_API_URL: server.baseUrl,
    AGENTSFLEET_STATE_DIR: state.dir, NO_COLOR: "1" });
  const child = spawnFleetctl([STEER_COMMAND, FLEET_ID, TEST_MESSAGE], { env });
  child.stdin.end();
  let stdout = "";
  let stderr = "";
  const exited = new Promise<number | null>((resolve, reject) => {
    child.once("error", reject);
    child.once("close", resolve);
  });
  child.stderr.on("data", (chunk) => { stderr += String(chunk); });
  const chunkPrinted = new Promise<void>((resolve) => {
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
      if (stdout.includes(LIVE_CHUNK)) resolve();
    });
  });
  const timer = setTimeout(() => child.kill("SIGKILL"), PHASE_TIMEOUT_MS);
  try {
    await Promise.race([chunkPrinted, exited.then(() => {
      throw new Error(`CLI exited before the incremental chunk: ${stderr}`);
    })]);
    expect(stdout).not.toContain(STATUS_PROCESSED);
    expect(stdout).not.toContain(UNRELATED_CHUNK);
    expect(server.counts).toEqual({ messages: 1, streams: 1, polls: 0 });
    server.send(FRAME.complete, { event_id: EVENT_ID, status: STATUS_PROCESSED });
    server.send(FRAME.complete, { event_id: EVENT_ID, status: STATUS_PROCESSED });
    expect(await exited).toBe(0);
    expect(stdout.split(`${EVENT_ID} ${STATUS_PROCESSED}`)).toHaveLength(2);
    expect(stdout.split(LIVE_CHUNK)).toHaveLength(2);
    expect(server.counts).toEqual({ messages: 1, streams: 1, polls: 0 });
  } finally {
    clearTimeout(timer);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    await exited;
  }
}

describe("steer transport reaches a truthful terminal result", () => {
  it("prints a chunk before completion and ignores unrelated, malformed and duplicate frames",
    runPhasedSteer, TEST_TIMEOUT_MS);

  it("prints a human terminal result delivered through SSE without polling", async () => {
    const { result, counts } = await runSteer(SERVER_MODE.sseTerminal, false);

    expect(result.code).toBe(0);
    expect(result.stdout).toContain(`${EVENT_ID} ${STATUS_PROCESSED}`);
    expect(counts).toEqual({ messages: 1, streams: 1, polls: 0 });
  });

  it("falls back after stream loss and prints the same terminal JSON result", async () => {
    const { result, counts } = await runSteer(SERVER_MODE.fallbackTerminal, true);

    expect(result.code).toBe(0);
    const parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    expect(parsed.event_id).toBe(EVENT_ID);
    expect(parsed.kind).toBe(KIND_COMPLETE);
    expect(parsed.status).toBe(STATUS_PROCESSED);
    expect(counts).toEqual({ messages: 1, streams: 1, polls: 1 });
  });

  it("reports an explicit fleet failure distinctly from a transport timeout", async () => {
    const { result, counts } = await runSteer(SERVER_MODE.explicitFailure, true);

    expect(result.code).not.toBe(0);
    expect(result.stdout).toContain(STATUS_FLEET_ERROR);
    expect(`${result.stdout}\n${result.stderr}`).not.toContain("timeout");
    expect(counts).toEqual({ messages: 1, streams: 1, polls: 1 });
  });
});
