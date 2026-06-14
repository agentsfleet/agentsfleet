// Integration + unit coverage for src/commands/zombie_steer.ts (part 1).
// Targets branches in SSE frame callbacks, validation paths, terminal
// status detection, and json-mode renderOutcome (lines 65-67, 93-94,
// 97-98, 101-103, 110, 127, 231, 284-286, 340-345, 247-252).
//
// Part 2 (error paths, poll, REPL) lives in zombie-steer-errors.integration.test.ts.

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { Readable, Writable } from "node:stream";

import { runCli } from "../src/cli.ts";
import { steerEffectFromArgs } from "../src/commands/zombie_steer.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi } from "./helpers-mock-api.ts";

// Exported so the sibling error-path suite shares one source of truth for
// the fixture ids + mocked-layer config (see makeLayer below).
export const WS_ID = "01910000-0000-7000-8000-000000a6e711";
export const AGENTSFLEET_ID = "01910000-0000-7000-8000-000000a67e57";
export const TOKEN = "test.jwt.token";
export const EVENT_ID = "1729874000000-abc";
export const API_URL = "https://api.steer-test.local";
export const DASHBOARD_URL = "https://dash.steer-test.local";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_steer" }, fn);

export const streamFrom = (chunks: ReadonlyArray<string>, isTTY: boolean) => {
  const stream = Readable.from(chunks);
  Object.defineProperty(stream, "isTTY", { value: isTTY });
  return stream as unknown as import("../src/lib/repl.ts").ReplInputStream;
};

export const nullOutput = () =>
  new Writable({ write(_c, _e, cb) { cb(); } }) as unknown as
    import("../src/lib/repl.ts").ReplOutputStream;

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly requests: HttpRequestInput[];
}

export const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], requests: [] });

export const makeLayer = (
  rec: Recorder,
  httpReply: <T>(input: HttpRequestInput) => T = <T>() => ({ event_id: EVENT_ID } as T),
  jsonMode = false,
  outputOverrides?: Partial<typeof Output.Service>,
) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: API_URL,
      dashboardUrl: DASHBOARD_URL,
      accessToken: Option.none(),
      jsonMode,
      noOpen: false,
      telemetryPosthogKey: "phc_test",
      telemetryPosthogHost: "https://us.i.posthog.com",
    }),
    Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.some(Redacted.make(TOKEN))),
      getSavedAt: Effect.sync(() => null),
      getSessionId: Effect.sync(() => null),
      getApiUrl: Effect.sync(() => null),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(Workspaces, {
      load: Effect.sync(() => ({ current_workspace_id: WS_ID, items: [] })),
      save: () => Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) =>
        Effect.sync(() => { rec.requests.push(input); return httpReply<T>(input); }),
    }),
    Layer.succeed(Output, {
      intro: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      info: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      success: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      warn: (m) => Effect.sync(() => { rec.stderr.push(m); }),
      error: (m) => Effect.sync(() => { rec.stderr.push(m); }),
      outro: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      printJson: (p) => Effect.sync(() => { rec.stdout.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { rec.stderr.push(JSON.stringify(p)); }),
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: () => Effect.void,
      ...outputOverrides,
    }),
  );

export const eventStream = (events: Parameters<StreamGetCallback>[0][]) =>
  async (
    _url: string,
    _headers: Record<string, string>,
    cb: StreamGetCallback,
  ): Promise<void> => {
    for (const ev of events) {
      if (cb(ev) === false) return;
    }
  };

// ── Integration: empty message validation (lines 340-345) ─────────────────

describe("steer — empty message validation via CLI (lines 340-345)", () => {
  test("whitespace-only message positional fails with ValidationError", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["steer", AGENTSFLEET_ID, "   "],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toMatch(/message is required/i);
      });
    });
  });
});

// ── Unit: undefined zombie_id validation (lines 284-286) ──────────────────

describe("steer — undefined zombie_id validation (lines 284-286)", () => {
  test("steerEffectFromArgs with undefined zombieId fails with ValidationError", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(undefined, "hello", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream([]),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.requests).toHaveLength(0);
  });
});

// ── Unit: SSE frame callbacks (lines 93-94, 97-98, 101-103, 110) ──────────

describe("steer — SSE frame callbacks", () => {
  test("chunk event prints claw-prefixed text (lines 93-94)", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "chunk", data: { event_id: EVENT_ID, text: "hello from claw" } },
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "ping", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("hello from claw"))).toBe(true);
  });

  test("tool_call_started prints tool name with 'starting' suffix (lines 97-98)", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "tool_call_started", data: { event_id: EVENT_ID, name: "read_file" } },
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("read_file") && l.includes("starting"))).toBe(true);
  });

  test("tool_call_completed prints tool name, 'done', and ms (lines 101-103)", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "tool_call_completed", data: { event_id: EVENT_ID, name: "write_file", ms: 42 } },
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("write_file") && l.includes("done") && l.includes("42ms"))).toBe(true);
  });

  test("unknown event type is silently skipped (line 110)", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "unknown_event_xyz", data: { event_id: EVENT_ID } },
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });
});

// ── Unit: isTerminal + non-PROCESSED renderOutcome (lines 65-67, 247-252) ──

describe("steer — terminal status checks (lines 65-67, 247-252)", () => {
  test("agent_error status is terminal; renderOutcome fails with ConfigError", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.AGENT_ERROR } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes(EVENT_STATUS.AGENT_ERROR))).toBe(true);
  });

  test("gate_blocked status is terminal; renderOutcome fails with ConfigError", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.GATE_BLOCKED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes(EVENT_STATUS.GATE_BLOCKED))).toBe(true);
  });
});

// ── Unit: json mode renderOutcome (lines 127, 231) ────────────────────────

describe("steer — json mode renderOutcome (lines 127, 231)", () => {
  test("json mode outputs structured JSON with event_id and outcome", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(AGENTSFLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec, undefined, true))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const jsonOut = rec.stdout.find((l) => l.startsWith("{"));
    expect(jsonOut).toBeDefined();
    const parsed = JSON.parse(jsonOut ?? "{}") as Record<string, unknown>;
    expect(parsed["event_id"]).toBe(EVENT_ID);
    expect(parsed["kind"]).toBe("complete");
  });
});
