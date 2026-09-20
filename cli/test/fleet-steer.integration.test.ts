// Integration + unit coverage for src/commands/fleet_steer.ts (part 1).
// Targets branches in SSE frame callbacks, validation paths, terminal
// status detection, and json-mode renderOutcome (lines 65-67, 93-94,
// 97-98, 101-103, 110, 127, 231, 284-286, 340-345, 247-252).
//
// Part 2 (error paths, poll, REPL) lives in fleet-steer-errors.integration.test.ts.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { runCli } from "../src/cli.ts";
import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import {
  KIND_CHUNK,
  KIND_EVENT_COMPLETE,
  KIND_TOOL_CALL_COMPLETED,
  KIND_TOOL_CALL_STARTED,
  STATUS_COMPLETE,
} from "../src/commands/fleet_steer_events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi } from "./helpers-mock-api.ts";
import {
  FLEET_ID,
  EVENT_ID,
  CALL_STREAM_OPEN,
  CALL_POST,
  authedScope,
  streamFrom,
  nullOutput,
  makeRecorder,
  makeLayer,
  eventStream,
} from "./helpers-fleet-steer.ts";

// ── Integration: empty message validation (lines 340-345) ─────────────────

describe("steer — empty message validation via CLI (lines 340-345)", () => {
  test("whitespace-only message positional fails with ValidationError", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["steer", FLEET_ID, "   "],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toMatch(/message is required/i);
      });
    });
  });
});

// ── Unit: undefined fleet_id validation (lines 284-286) ──────────────────

describe("steer — undefined fleet_id validation (lines 284-286)", () => {
  test("steerEffectFromArgs with undefined fleetId fails with ValidationError", async () => {
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
      { id: null, type: KIND_CHUNK, data: { event_id: EVENT_ID, text: "hello from claw" } },
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "ping", {}, {
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
      { id: null, type: KIND_TOOL_CALL_STARTED, data: { event_id: EVENT_ID, name: "read_file" } },
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
      { id: null, type: KIND_TOOL_CALL_COMPLETED, data: { event_id: EVENT_ID, name: "write_file", ms: 42 } },
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
  test("fleet_error status is terminal; renderOutcome fails with ConfigError", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.FLEET_ERROR } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes(EVENT_STATUS.FLEET_ERROR))).toBe(true);
  });

  test("gate_blocked status is terminal; renderOutcome fails with ConfigError", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.GATE_BLOCKED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
    expect(parsed["kind"]).toBe(STATUS_COMPLETE);
  });

  test("test_json_mode_shape_unchanged", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec, undefined, true))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // pin test: literal is the contract — the serialized string is public CLI
    // output; consumers parse it byte-for-byte.
    const expected = JSON.stringify({
      event_id: EVENT_ID,
      kind: STATUS_COMPLETE,
      status: EVENT_STATUS.PROCESSED,
    });
    expect(rec.stdout).toContain(expected);
  });
});

// ── Integration: subscribe-before-send ordering ───────────────────────────

describe("steer — the tail opens before the message posts", () => {
  test("test_stream_opens_before_post", async () => {
    const rec = makeRecorder();
    const calls: string[] = [];
    // A real handshake takes time: the marker lands only when headers are
    // accepted (onOpen), so an implementation that merely dispatches the
    // stream and posts immediately records the POST first and fails here.
    const HANDSHAKE_DELAY_MS = 10;
    const recordingStream = async (
      _url: string,
      _headers: Record<string, string>,
      cb: StreamGetCallback,
      options?: { onOpen?: () => void },
    ): Promise<void> => {
      await new Promise((resolve) => {
        setTimeout(resolve, HANDSHAKE_DELAY_MS);
      });
      calls.push(CALL_STREAM_OPEN);
      options?.onOpen?.();
      cb({ id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } });
    };
    const httpReply = <T>(_input: HttpRequestInput): T => {
      calls.push(CALL_POST);
      return { event_id: EVENT_ID } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: recordingStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls).toEqual([CALL_STREAM_OPEN, CALL_POST]);
  });
});
