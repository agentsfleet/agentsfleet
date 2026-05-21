import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { Readable, Writable } from "node:stream";
import { steerEffectFromArgs } from "../src/commands/zombie_steer.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import type { ReplInputStream, ReplOutputStream } from "../src/lib/repl.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";

const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000010";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-000000000020";
const TOKEN = "test-token";
const EVENT_ID = "1729874000000-0";
const API_URL = "https://api.test.local";
const DASHBOARD_URL = "https://dash.test.local";
const POST = "POST";

interface Recorder {
  readonly requests: HttpRequestInput[];
  readonly stdout: string[];
  readonly stderr: string[];
  readonly streamSignals: AbortSignal[];
}

const streamFrom = (chunks: ReadonlyArray<string>, isTTY: boolean): ReplInputStream => {
  const stream = Readable.from(chunks) as ReplInputStream;
  Object.defineProperty(stream, "isTTY", { value: isTTY });
  return stream;
};

const nullOutput = (): ReplOutputStream =>
  new Writable({
    write(_chunk, _encoding, callback): void {
      callback();
    },
  }) as ReplOutputStream;

const makeRecorder = (): Recorder => ({
  requests: [],
  stdout: [],
  stderr: [],
  streamSignals: [],
});

const testLayer = (rec: Recorder) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: API_URL,
      dashboardUrl: DASHBOARD_URL,
      accessToken: Option.none(),
      jsonMode: false,
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
      request: <T = unknown>(input: HttpRequestInput) =>
        Effect.sync(() => {
          rec.requests.push(input);
          return { event_id: EVENT_ID } as T;
        }),
    }),
    Layer.succeed(Output, {
      intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
      info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
      success: (msg) => Effect.sync(() => rec.stdout.push(msg)),
      warn: (msg) => Effect.sync(() => rec.stderr.push(msg)),
      error: (msg) => Effect.sync(() => rec.stderr.push(msg)),
      outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
      printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
      printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: () => Effect.void,
    }),
  );

const completeStream = (rec: Recorder) =>
  async (
    _url: string,
    _headers: Record<string, string>,
    onEvent: StreamGetCallback,
    options?: { signal?: AbortSignal },
  ): Promise<void> => {
    if (options?.signal) rec.streamSignals.push(options.signal);
    onEvent({ id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } });
  };

describe("steerEffectFromArgs REPL dispatch", () => {
  test("non-TTY without --tty reads stdin once and stays single-shot", async () => {
    const rec = makeRecorder();
    const effect = steerEffectFromArgs(
      ZOMBIE_ID,
      undefined,
      {},
      { stdin: streamFrom(["howdy\n"], false), stdout: nullOutput(), streamGet: completeStream(rec) },
    ).pipe(Effect.provide(testLayer(rec)));

    const exit = await Effect.runPromiseExit(effect);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.requests).toHaveLength(1);
    expect(rec.requests[0]?.method).toBe(POST);
    expect(rec.requests[0]?.body).toEqual({ message: "howdy" });
    expect(rec.streamSignals).toHaveLength(0);
  });

  test("--tty forces prompt loop for piped stdin and exits on EOF", async () => {
    const rec = makeRecorder();
    const effect = steerEffectFromArgs(
      ZOMBIE_ID,
      undefined,
      { forceTty: true },
      { stdin: streamFrom(["first\nsecond\n"], false), stdout: nullOutput(), streamGet: completeStream(rec) },
    ).pipe(Effect.provide(testLayer(rec)));

    const exit = await Effect.runPromiseExit(effect);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.requests.map((request) => request.body)).toEqual([
      { message: "first" },
      { message: "second" },
    ]);
    expect(rec.streamSignals).toHaveLength(2);
  });
});
