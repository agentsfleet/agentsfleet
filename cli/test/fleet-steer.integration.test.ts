// Integration + unit coverage for src/commands/fleet/steer.ts (part 1).
// Targets branches in SSE frame callbacks, validation paths, terminal
// status detection, and json-mode renderOutcome (lines 65-67, 93-94,
// 97-98, 101-103, 110, 127, 231, 284-286, 340-345, 247-252).
//
// Part 2 (error paths, poll, REPL) lives in fleet-steer-errors.integration.test.ts.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { runCli } from "../src/cli.ts";
import { steerEffectFromArgs } from "../src/commands/fleet/steer.ts";
import {
  KIND_CHUNK,
  KIND_TURN_COMPLETE,
  KIND_TOOL_CALL_COMPLETED,
  KIND_TOOL_CALL_STARTED,
  STATUS_COMPLETE,
} from "../src/commands/fleet/steer-turns.ts";
import { TURN_STATUS } from "../src/constants/turn-status.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi } from "./helpers-mock-api.ts";
import {
  FLEET_ID,
  TURN_ID,
  WS_ID,
  POST,
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
          ["fleet", "steer", FLEET_ID, "   "],
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
      { id: null, type: KIND_CHUNK, data: { turn_id: TURN_ID, text: "hello from claw" } },
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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
      { id: null, type: KIND_TOOL_CALL_STARTED, data: { turn_id: TURN_ID, name: "read_file" } },
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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

  test.each(["succeeded", "failed", undefined] as const)("tool completion presents recorded status %s without inferring success", async status => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_TOOL_CALL_COMPLETED, data: { turn_id: TURN_ID, name: "write_file", ms: 42, ...(status ? {status} : {}) } },
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("write_file") && l.includes(status ?? "outcome unavailable") && l.includes("42ms"))).toBe(true);
  });

  test("unknown event type is silently skipped (line 110)", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: "unknown_event_xyz", data: { turn_id: TURN_ID } },
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.FLEET_ERROR } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes(TURN_STATUS.FLEET_ERROR))).toBe(true);
  });

  test("gate_blocked status is terminal; renderOutcome fails with ConfigError", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.GATE_BLOCKED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(events),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes(TURN_STATUS.GATE_BLOCKED))).toBe(true);
  });
});

// ── Unit: json mode renderOutcome (lines 127, 231) ────────────────────────

describe("steer — json mode renderOutcome (lines 127, 231)", () => {
  test("json mode outputs structured JSON with turn_id and outcome", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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
    expect(parsed["turn_id"]).toBe(TURN_ID);
    expect(parsed["kind"]).toBe(STATUS_COMPLETE);
  });

  test("test_json_mode_shape_unchanged", async () => {
    const rec = makeRecorder();
    const events = [
      { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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
      turn_id: TURN_ID,
      kind: STATUS_COMPLETE,
      status: TURN_STATUS.PROCESSED,
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
      cb({ id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } });
    };
    const httpReply = <T>(input: HttpRequestInput): T => {
      calls.push(input.method === POST ? CALL_POST : `read:${input.path}`);
      return { turn_id: TURN_ID } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: recordingStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls).toEqual([CALL_STREAM_OPEN, CALL_POST, `read:/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/turns/${TURN_ID}`]);
    expect(rec.requests.filter(request => request.method === POST).map(request => request.body)).toEqual([{message: "go"}]);
  });
});
