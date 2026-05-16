// `zombiectl steer <id> "<msg>"` batch roundtrip — exercises the full
// command path with mocked deps:
//   - POST /messages returns 202 + event_id.
//   - streamGet emits scripted SSE frames matching the event_id.
//   - tool_call_started / chunk / tool_call_completed / event_complete
//     all flow through; chunks render as `[claw] <text>` on stdout.
//   - event_complete with status=processed yields exit 0.
//
// Pure mocks — no live API server, no Redis, no harness binary. The
// integration that crosses real wire (worker → executor RPC → activity
// PUBLISH) is asserted in src/zombie/event_loop_harness_integration_test.zig
// against the harness binary; this test covers the CLI half of the same
// loop where the SSE consumer lives.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandSteer } from "../src/commands/zombie_steer.ts";
import {
  buildParsed,
  makeBufferStream,
  ui,
  WS_ID,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";
import type { SseFrame } from "../src/lib/sse.ts";
import type { WriteStream } from "../src/output/index.ts";

type StreamArg = WriteStream | NodeJS.WritableStream;

const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa90";
const EVENT_ID = "1777400000000-0";

interface CtxBundle {
  ctx: CommandCtx;
  readStdout: () => string;
  readStderr: () => string;
}

function makeCtx(overrides: Partial<CommandCtx> = {}): CtxBundle {
  const out = makeBufferStream();
  const err = makeBufferStream();
  return {
    ctx: {
      stdout: out.stream,
      stderr: err.stream,
      jsonMode: false,
      apiUrl: "https://api.test.invalid",
      token: "test-token",
      env: {},
      ...overrides,
    },
    readStdout: out.read,
    readStderr: err.read,
  };
}

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    request: async () => ({ event_id: EVENT_ID }),
    apiHeaders: () => ({}),
    streamGet: async () => {},
    ui,
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

function asString(body: unknown): string {
  if (typeof body !== "string") throw new Error("expected string body");
  return body;
}

// Test SSE frames omit the `id` field intentionally — commandSteer reads
// only `.type` and `.data` for its frame-routing logic; an SSE without
// `id:` is valid per the spec.
type TestFrame = Pick<SseFrame, "type" | "data">;

const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [] };

test("steer batch: POST /messages + SSE roundtrip prints chunks and exits 0", async () => {
  const captured: {
    postUrl: string | null;
    postBody: { message?: string } | null;
    streamUrl: string | null;
    streamHeaders: Record<string, string> | null;
  } = { postUrl: null, postBody: null, streamUrl: null, streamHeaders: null };

  const scriptedFrames: TestFrame[] = [
    { type: "event_received", data: { event_id: EVENT_ID, actor: "steer:test" } },
    { type: "tool_call_started", data: { event_id: EVENT_ID, name: "http.request" } },
    { type: "chunk", data: { event_id: EVENT_ID, text: "Hello" } },
    { type: "chunk", data: { event_id: EVENT_ID, text: " world" } },
    { type: "tool_call_completed", data: { event_id: EVENT_ID, name: "http.request", ms: 120 } },
    { type: "event_complete", data: { event_id: EVENT_ID, status: "processed" } },
  ];

  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      if (opts?.method === "POST" && url.includes("/messages")) {
        captured.postUrl = url;
        captured.postBody = JSON.parse(asString(opts.body)) as { message?: string };
        return { event_id: EVENT_ID };
      }
      throw new Error(`unexpected request: ${opts?.method ?? "?"} ${url}`);
    },
    apiHeaders: () => ({ Authorization: "Bearer test-token" }),
    streamGet: async (url, headers, onEvent) => {
      captured.streamUrl = url;
      captured.streamHeaders = { ...headers };
      for (const frame of scriptedFrames) {
        const result = onEvent({ id: null, ...frame });
        if (result === false) return; // terminal — caller asked to stop
      }
    },
    writeLine: (stream: StreamArg, line?: string) => {
      if (line !== undefined) stream.write(`${line}\n`);
      else stream.write("\n");
    },
    writeError: (ctx, _code, msg) => { ctx.stderr?.write(`${msg}\n`); },
  });

  const { ctx, readStdout, readStderr } = makeCtx();
  const code = await commandSteer(ctx, buildParsed([ZOMBIE_ID, "ping"]), workspaces, deps);

  assert.equal(code, 0);
  assert.ok(captured.postUrl?.includes(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/messages`));
  assert.equal(captured.postBody?.message, "ping");
  assert.ok(captured.streamUrl?.includes(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events/stream`));
  assert.equal(captured.streamHeaders?.["Authorization"], "Bearer test-token");

  const out = readStdout();
  assert.ok(out.includes("[claw] Hello"), `stdout missing first chunk: ${out}`);
  assert.ok(out.includes("[claw]  world"), `stdout missing second chunk: ${out}`);
  assert.ok(out.includes(`event ${EVENT_ID} processed`), `stdout missing terminal line: ${out}`);
  assert.equal(readStderr(), "");
});

test("steer batch: agent_error terminal status yields exit 1", async () => {
  const deps = makeDeps({
    streamGet: async (_url, _h, onEvent) => {
      onEvent({ id: null, type: "event_complete", data: { event_id: EVENT_ID, status: "agent_error" } });
    },
  });
  const { ctx } = makeCtx();
  const code = await commandSteer(ctx, buildParsed([ZOMBIE_ID, "ping"]), workspaces, deps);
  assert.equal(code, 1);
});

test("steer batch: missing message returns exit 2 without calling API", async () => {
  let calls = 0;
  const deps = makeDeps({
    request: async () => { calls += 1; return {}; },
    streamGet: async () => { calls += 1; },
  });
  const { ctx } = makeCtx();
  const code = await commandSteer(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);
  assert.equal(code, 2);
  assert.equal(calls, 0);
});

test("steer batch: missing event_id in POST response returns exit 1", async () => {
  const deps = makeDeps({
    request: async () => ({}), // No event_id.
    streamGet: async () => { throw new Error("should not be called"); },
  });
  const { ctx } = makeCtx();
  const code = await commandSteer(ctx, buildParsed([ZOMBIE_ID, "ping"]), workspaces, deps);
  assert.equal(code, 1);
});

test("steer batch: filters frames whose event_id does not match", async () => {
  const otherEvent = "9999999999999-0";
  const scriptedFrames: TestFrame[] = [
    // Noise from a sibling zombie/event — must not render.
    { type: "chunk", data: { event_id: otherEvent, text: "OTHER NOISE" } },
    { type: "chunk", data: { event_id: EVENT_ID, text: "real" } },
    { type: "event_complete", data: { event_id: EVENT_ID, status: "processed" } },
  ];
  const deps = makeDeps({
    streamGet: async (_url, _h, onEvent) => {
      for (const f of scriptedFrames) {
        if (onEvent({ id: null, ...f }) === false) return;
      }
    },
    writeLine: (stream: StreamArg, line?: string) => {
      if (line !== undefined) stream.write(`${line}\n`);
      else stream.write("\n");
    },
  });
  const { ctx, readStdout } = makeCtx();
  const code = await commandSteer(ctx, buildParsed([ZOMBIE_ID, "ping"]), workspaces, deps);
  assert.equal(code, 0);
  const out = readStdout();
  assert.ok(out.includes("[claw] real"), `stdout missing matched chunk: ${out}`);
  assert.ok(!out.includes("OTHER NOISE"), `stdout leaked unmatched chunk: ${out}`);
});
