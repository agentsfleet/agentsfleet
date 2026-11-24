// `chat`: a line is a turn on a fleet's thread. With no fleet named, the first
// line is the fleet's first message, and the rest of the session talks to it.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { runCli } from "../src/cli.ts";
import { chatEffect } from "../src/commands/chat/chat.ts";
import { KIND_CHUNK, KIND_TURN_COMPLETE } from "../src/commands/fleet/steer-turns.ts";
import { TURN_STATUS } from "../src/constants/turn-status.ts";
import { ServerError, ValidationError, type NetworkError } from "../src/errors/index.ts";
import { isValidId } from "../src/lib/id.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
import type { SseFrame, StreamGetCallback } from "../src/lib/sse.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { authedScope, nullOutput, streamFrom, WS_ID as STEER_WS_ID } from "./helpers-fleet-steer.ts";
import { withMockApi } from "./helpers-mock-api.ts";
import { outputDouble } from "./helpers-output-double.ts";

const WS_ID = "01910000-0000-7000-8000-0000000c4a01";
const FLEET_ID = "01910000-0000-7000-8000-0000000c4a02";
const FIRST_TURN = "01910000-0000-7000-8000-0000000c4a03";
const SECOND_TURN = "01910000-0000-7000-8000-0000000c4a04";
const API_URL = "https://api.chat-test.local";
const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const FLEET_PATH = `${FLEETS}/${FLEET_ID}`;
const STREAM_URL = `${API_URL}${FLEET_PATH}/turns/stream`;
const CREATED = {
  fleet: { id: FLEET_ID, name: "fleet-00000c4a02", status: "active" },
  accepted: { turn_id: FIRST_TURN, disposition: "started" },
  replayed: false,
};
const STARTED = `Started fleet fleet-00000c4a02 (${FLEET_ID})`;
const READ_AFTER_STREAM = "Reading saved activity after the live stream ended.";

type Answer = (input: HttpRequestInput) => Effect.Effect<unknown, NetworkError | ServerError>;

interface Run {
  readonly requests: HttpRequestInput[];
  readonly stdout: string[];
  readonly stderr: string[];
  readonly streams: string[];
}

const newRun = (): Run => ({ requests: [], stdout: [], stderr: [], streams: [] });

const layer = (run: Run, answer: Answer) => Layer.mergeAll(
  Layer.succeed(CliConfig, {
    apiUrl: API_URL, dashboardUrl: API_URL, accessToken: Option.none(), jsonMode: false, noOpen: true,
    telemetryPosthogKey: "", telemetryPosthogHost: API_URL,
  }),
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("chat-token"))),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  }),
  Layer.succeed(Workspaces, { load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }), save: () => Effect.void }),
  Layer.succeed(HttpClient, {
    request: <T>(input: HttpRequestInput) => Effect.suspend(() => {
      run.requests.push(input);
      return answer(input) as Effect.Effect<T, NetworkError | ServerError>;
    }),
  }),
  Layer.succeed(Output, {
    ...outputDouble(),
    info: (line) => Effect.sync(() => { run.stdout.push(line); }),
    success: (line) => Effect.sync(() => { run.stdout.push(line); }),
    error: (line) => Effect.sync(() => { run.stderr.push(line); }),
  }),
);

const refusal = (status: number, code: string) =>
  new ServerError({ detail: `refused ${code}`, suggestion: "test suggestion", code, status, requestId: null });

// The fleet's stream: it opens, then carries what each run publishes.
const tail = (run: Run, frames: ReadonlyArray<ReadonlyArray<SseFrame>>) => async (
  url: string, _headers: Record<string, string>, onEvent: StreamGetCallback, options?: { readonly onOpen?: () => void },
): Promise<void> => {
  const opened = run.streams.push(url) - 1;
  options?.onOpen?.();
  for (const frame of frames[opened] ?? []) if (onEvent(frame) === false) return;
};

const answered = (turnId: string, reply: string): ReadonlyArray<SseFrame> => [
  { id: null, type: KIND_CHUNK, data: { turn_id: turnId, text: reply, fencing_token: "1" } },
  { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: turnId, status: TURN_STATUS.PROCESSED, fencing_token: "1" } },
];

const calls = (run: Run): ReadonlyArray<string> => run.requests.map((request) => `${request.method ?? "GET"} ${request.path}`);

const running = (turnId: string) => ({ turn_id: turnId, status: "running" });

const daemon = (routes: Readonly<Record<string, () => Effect.Effect<unknown, ServerError>>>): Answer => (input) =>
  routes[`${input.method ?? "GET"} ${input.path}`]?.() ?? Effect.fail(refusal(404, "UZ-TEST-404"));

const prompt = (lines: ReadonlyArray<string>, run: Run, frames: ReadonlyArray<ReadonlyArray<SseFrame>>) => ({
  stdin: streamFrom(lines, true), stdout: nullOutput(), signalSource: new ReplSignalEmitter(), streamGet: tail(run, frames),
});

describe("chat with no fleet named", () => {
  test("chat with no fleet creates one on the first line", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () => Effect.succeed(running(FIRST_TURN)),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] },
      prompt(["watch checkout errors\n"], run, [answered(FIRST_TURN, "Watching checkout errors.")]))
      .pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls(run)).toEqual([
      `POST ${FLEETS}`,
      `GET ${FLEET_PATH}/turns/${FIRST_TURN}`,
      `GET ${FLEET_PATH}/turns/${FIRST_TURN}`,
    ]);
    const body = run.requests[0]?.body as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual(["message", "operation_id"]);
    expect(body.message).toBe("watch checkout errors");
    expect(isValidId(body.operation_id)).toBe(true);
    expect(run.streams).toEqual([STREAM_URL]);
    expect(run.stdout[0]).toBe(STARTED);
    expect(run.stdout.some((line) => line.includes("Watching checkout errors."))).toBe(true);
    expect(run.stdout.at(-1)).toBe(`turn ${FIRST_TURN} processed`);
  });

  test("a second line steers the fleet the first line made", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () => Effect.succeed(running(FIRST_TURN)),
      [`POST ${FLEET_PATH}/messages`]: () => Effect.succeed({ turn_id: SECOND_TURN, disposition: "started" }),
      [`GET ${FLEET_PATH}/turns/${SECOND_TURN}`]: () => Effect.succeed(running(SECOND_TURN)),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] },
      prompt(["watch checkout errors\n", "check it now\n"], run, [answered(FIRST_TURN, "Watching."), answered(SECOND_TURN, "Checked.")]))
      .pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls(run).filter((call) => call.startsWith("POST"))).toEqual([`POST ${FLEETS}`, `POST ${FLEET_PATH}/messages`]);
    expect(run.requests.find((request) => request.path.endsWith("/messages"))?.body).toEqual({ message: "check it now" });
    expect(run.streams).toEqual([STREAM_URL, STREAM_URL]);
    expect(run.stdout).toContain(`turn ${SECOND_TURN} processed`);
  });

  test("a refused first line leaves the next line to start the fleet", async () => {
    const run = newRun();
    let creates = 0;
    const answer = daemon({
      [`POST ${FLEETS}`]: () => ++creates === 1 ? Effect.fail(refusal(409, "UZ-SETUP-008")) : Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () => Effect.succeed(running(FIRST_TURN)),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] },
      prompt(["one\n", "two\n"], run, [answered(FIRST_TURN, "Started.")]))
      .pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls(run).filter((call) => call.startsWith("POST"))).toEqual([`POST ${FLEETS}`, `POST ${FLEETS}`]);
    expect(run.requests.filter((request) => request.method === "POST").map((request) => (request.body as { message: string }).message))
      .toEqual(["one", "two"]);
    expect(run.stderr).toEqual(["refused UZ-SETUP-008"]);
    expect(run.stdout).toContain(STARTED);
  });

  test("a reply that settled before the stream opened is read from the saved turn", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () =>
        Effect.succeed({ turn_id: FIRST_TURN, status: TURN_STATUS.PROCESSED, response_text: "Already done." }),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] }, {
      stdin: streamFrom(["watch checkout errors"], false), streamGet: tail(run, []),
    }).pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(run.stdout).toContain("Final reply:\nAlready done.");
    expect(run.stdout).not.toContain(READ_AFTER_STREAM);
    expect(run.stdout.at(-1)).toBe(`turn ${FIRST_TURN} processed`);
  });

  test("a turn not yet saved is followed on the stream", async () => {
    const run = newRun();
    let reads = 0;
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () =>
        ++reads === 1 ? Effect.fail(refusal(404, "UZ-AGT-015")) : Effect.succeed(running(FIRST_TURN)),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] }, {
      stdin: streamFrom(["watch checkout errors"], false), streamGet: tail(run, [answered(FIRST_TURN, "On it.")]),
    }).pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(run.stdout.some((line) => line.includes("On it."))).toBe(true);
    expect(run.stdout).not.toContain(READ_AFTER_STREAM);
  });

  test("a stream that fails before it opens leaves the saved turn to answer", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () => Effect.succeed({ turn_id: FIRST_TURN, status: TURN_STATUS.PROCESSED }),
    });
    const broken = async (): Promise<void> => { throw new Error("stream refused"); };
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] }, {
      stdin: streamFrom(["watch checkout errors"], false), streamGet: broken,
    }).pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(run.stdout).toContain(READ_AFTER_STREAM);
    expect(run.stdout.at(-1)).toBe(`turn ${FIRST_TURN} processed`);
  });

  test("a saved read the daemon refuses ends the chat with that refusal", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEETS}`]: () => Effect.succeed(CREATED),
      [`GET ${FLEET_PATH}/turns/${FIRST_TURN}`]: () => Effect.fail(refusal(503, "UZ-TEST-503")),
    });
    const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] }, {
      stdin: streamFrom(["watch checkout errors"], false), streamGet: tail(run, []),
    }).pipe(Effect.provide(layer(run, answer))));
    if (Exit.isSuccess(exit)) throw new Error("expected the saved read's refusal");
    expect(Cause.squash(exit.cause)).toMatchObject({ code: "UZ-TEST-503", status: 503 });
  });

  test.each([["an empty pipe", ""], ["a pipe of blank lines", "\n \n"]] as const)(
    "%s is refused before anything is sent", async (_case, piped) => {
      const run = newRun();
      const exit = await Effect.runPromiseExit(chatEffect(undefined, { retired: [] }, {
        stdin: streamFrom([piped], false), streamGet: tail(run, []),
      }).pipe(Effect.provide(layer(run, daemon({})))));
      if (Exit.isSuccess(exit)) throw new Error("expected a usage refusal");
      const error = Cause.squash(exit.cause);
      expect(error).toBeInstanceOf(ValidationError);
      expect(error).toMatchObject({ detail: "message is required", suggestion: "usage: agentsfleet chat [<fleet_id|@name>]" });
      expect(run.requests).toEqual([]);
    });
});

describe("chat with a fleet named", () => {
  test("a piped message goes to that fleet's thread and starts no fleet", async () => {
    const run = newRun();
    const answer = daemon({
      [`POST ${FLEET_PATH}/messages`]: () => Effect.succeed({ turn_id: SECOND_TURN, disposition: "started" }),
      [`GET ${FLEET_PATH}/turns/${SECOND_TURN}`]: () => Effect.succeed(running(SECOND_TURN)),
    });
    const exit = await Effect.runPromiseExit(chatEffect(FLEET_ID, { retired: [] }, {
      stdin: streamFrom(["check it now"], false), streamGet: tail(run, [answered(SECOND_TURN, "Checked.")]),
    }).pipe(Effect.provide(layer(run, answer))));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls(run).filter((call) => call.startsWith("POST"))).toEqual([`POST ${FLEET_PATH}/messages`]);
    expect(run.stdout).not.toContain(STARTED);
  });
});

describe("the draft chat's flags", () => {
  const chat = (argv: ReadonlyArray<string>) => authedScope(() => withMockApi({}, async (apiUrl, sent) => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["chat", ...argv], { stdout: out.stream, stderr: err.stream,
      env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }) });
    return { code, out: out.read(), err: err.read(), fleetCalls: sent.filter((call) => call.path.startsWith(`/v1/workspaces/${STEER_WS_ID}`)) };
  }));

  test("chat --save is refused with the new command", async () => {
    const result = await chat(["--save"]);
    expect(result.code).toBe(4);
    expect(result.err).toContain("chat --save was removed");
    expect(result.err).toContain('agentsfleet fleet create "<message>"');
    expect(result.out).toBe("");
    expect(result.fleetCalls).toEqual([]);
  });

  test("--json carries the refusal as a structured error naming the replacement", async () => {
    const result = await chat(["--json", "--save"]);
    expect(result.code).toBe(4);
    expect(JSON.parse(result.err)).toEqual({ error: {
      code: "INVALID_ARGUMENT",
      message: "chat --save was removed: a fleet starts from its first message, with no draft to review or save",
      suggestion: 'start a fleet with: agentsfleet fleet create "<message>"; talk to one with: agentsfleet chat <fleet_id|@name>',
    } });
  });

  test.each([
    [["--review"], "review"],
    [["--no-input"], "no-input"],
    [["--definition", "setup.json"], "definition"],
    [["--model", "owner/runtime"], "model"],
    [["--message", "hello"], "message"],
    [["--fleet", FLEET_ID], "fleet"],
    [[FLEET_ID, "--apply", "--review-digest", "digest"], "apply"],
  ] as const)("%p is refused by its name before any request", async (argv, flag) => {
    const result = await chat(argv);
    expect(result.code).toBe(4);
    expect(result.err).toContain(`chat --${flag} was removed`);
    expect(result.fleetCalls).toEqual([]);
  });
});
