// `fleet steer @name`: one lookup by the fleet's saved name before the event
// stream opens, and from there the run an id gets. The cases run the CLI in
// process against a loopback API, so the lookup, the stream and the message
// all cross real HTTP; the prompt case drives the handler, the one way to give
// it a terminal.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { runCli } from "../src/cli.ts";
import { steerEffectFromArgs } from "../src/commands/fleet/steer.ts";
import { KIND_CHUNK, KIND_EVENT_COMPLETE } from "../src/commands/fleet/steer-events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { jsonResponse, withMockApi, type MockCall, type MockRoutes } from "./helpers-mock-api.ts";
import {
  EVENT_ID, FLEET_ID, POST, WS_ID, authedScope, eventStream, makeLayer, makeRecorder, nullOutput, streamFrom,
} from "./helpers-fleet-steer.ts";

const NAME = "agentbob";
const OTHER_FLEET_ID = "01910000-0000-7000-8000-000000a67e58";
const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const LOOKUP = `GET ${FLEETS}?name=agentbob`;
const REPLY = "Run 991 passed on retry.";
const VALIDATION_EXIT_CODE = 4;
const INVALID_ARGUMENT = "INVALID_ARGUMENT";
const UNKNOWN_NAME = "No fleet named `agentbob` in this workspace";
const NOT_A_NAME = "expected @ and a fleet name of 1 to 64 a-z, 0-9 or - characters (e.g. @support-triage)";
const NOT_AN_ID = "expected uuidv7 format (e.g. 0192a3b4-c5d6-7e8f-9012-345678901234)";

interface Run {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly calls: ReadonlyArray<MockCall>;
}

const row = (id: string, name: string) => ({
  id, name, description: null, status: "active", created_at: "2026-11-19T00:00:00Z",
  events_processed: "0", budget_used_nanos: "0",
});
const page = (...items: ReadonlyArray<ReturnType<typeof row>>) => ({ items, total: null, next_cursor: null });

// The list as the filter answers it: the one fleet saved under the name asked for.
const filteredLookup: MockRoutes = {
  [`GET ${FLEETS}`]: (_req, url) =>
    jsonResponse(200, url.searchParams.get("name") === NAME ? page(row(FLEET_ID, NAME)) : page()),
};

const sse = (...frames: ReadonlyArray<{ readonly type: string; readonly data: unknown }>): Response =>
  new Response(frames.map((frame) => `event: ${frame.type}\ndata: ${JSON.stringify(frame.data)}\n\n`).join(""),
    { headers: { "content-type": "text/event-stream" } });

// Every route a steer to `fleetId` crosses once the fleet is known, answered so
// a run that reaches them succeeds; an absent request is then a real absence.
const steerRoutes = (fleetId: string): MockRoutes => ({
  [`GET ${FLEETS}/${fleetId}/events/stream`]: () => sse(
    { type: KIND_CHUNK, data: { event_id: EVENT_ID, text: "Retrying run 991." } },
    { type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED, final_reply: REPLY } },
  ),
  [`POST ${FLEETS}/${fleetId}/messages`]: () => jsonResponse(202, { event_id: EVENT_ID, disposition: "started" }),
  [`GET ${FLEETS}/${fleetId}/events/${EVENT_ID}`]: () =>
    jsonResponse(200, { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }),
});

const steer = (argv: ReadonlyArray<string>, routes: MockRoutes): Promise<Run> =>
  authedScope(() => withMockApi(routes, async (apiUrl, calls) => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["fleet", "steer", ...argv], {
      stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
    });
    return { code, stdout: out.read(), stderr: err.read(), calls };
  }));

const requested = (run: Run): ReadonlyArray<string> =>
  run.calls.map((call) => `${call.method} ${call.path}${call.search}`);

const postedBodies = (run: Run): ReadonlyArray<unknown> =>
  run.calls.filter((call) => call.method === POST).map((call) => JSON.parse(call.body ?? "null"));

describe("fleet steer reaches a fleet by the name it was saved under", () => {
  test("fleet_steer_sends_to_the_fleet_named_with_an_at_sign", async () => {
    const routes = { ...filteredLookup, ...steerRoutes(FLEET_ID) };
    const named = await steer(["@AgentBob", "retry"], routes);
    const byId = await steer([FLEET_ID, "retry"], routes);
    expect(named.code).toBe(0);
    expect(requested(named)).toEqual([
      LOOKUP,
      `GET ${FLEETS}/${FLEET_ID}/events/stream`,
      `POST ${FLEETS}/${FLEET_ID}/messages`,
      `GET ${FLEETS}/${FLEET_ID}/events/${EVENT_ID}`,
    ]);
    expect(postedBodies(named)).toEqual([{ message: "retry" }]);
    expect(requested(byId)).toEqual(requested(named).slice(1));
    expect(named.stdout).toContain(REPLY);
    expect({ stdout: named.stdout, stderr: named.stderr }).toEqual({ stdout: byId.stdout, stderr: byId.stderr });

    const namedJson = await steer(["@AgentBob", "retry", "--json"], routes);
    const byIdJson = await steer([FLEET_ID, "retry", "--json"], routes);
    expect(namedJson.code).toBe(0);
    expect(JSON.parse(namedJson.stdout)).toEqual({
      event_id: EVENT_ID, disposition: "started", kind: "complete", status: EVENT_STATUS.PROCESSED,
    });
    expect(namedJson.stdout).toBe(byIdJson.stdout);
  });

  test("fleet_steer_refuses_an_unknown_name", async () => {
    const routes = { [`GET ${FLEETS}`]: () => jsonResponse(200, page()), ...steerRoutes(FLEET_ID) };
    const json = await steer(["@agentbob", "retry", "--json"], routes);
    expect(json.code).toBe(VALIDATION_EXIT_CODE);
    expect(requested(json)).toEqual([LOOKUP]);
    expect(JSON.parse(json.stderr)).toEqual({ error: {
      code: INVALID_ARGUMENT,
      message: UNKNOWN_NAME,
      suggestion: "run `agentsfleet fleet list` to see each fleet's name",
    } });
    expect(json.stdout).toBe("");

    const text = await steer(["@agentbob", "retry"], routes);
    expect(text.code).toBe(VALIDATION_EXIT_CODE);
    expect(requested(text)).toEqual([LOOKUP]);
    expect(text.stderr).toContain(UNKNOWN_NAME);
    expect(text.stderr).toContain(INVALID_ARGUMENT);
  });

  test("fleet_steer_refuses_a_malformed_name_or_id_before_any_request", async () => {
    // `@agentbob` itself would succeed against these routes, so a refusal here
    // is the parser's, not a failed lookup's.
    const routes = { ...filteredLookup, ...steerRoutes(FLEET_ID) };
    const refused: ReadonlyArray<readonly [string, string]> = [
      ["@agent_bob", NOT_A_NAME],
      ["@", NOT_A_NAME],
      [`@${"a".repeat(65)}`, NOT_A_NAME],
      ["@agentböb", NOT_A_NAME],
      ["@agentbob retry", NOT_A_NAME],
      ["not-a-uuid", NOT_AN_ID],
    ];
    for (const [value, expected] of refused) {
      const run = await steer([value, "retry", "--json"], routes);
      expect({ value, code: run.code, stderr: JSON.parse(run.stderr), requests: requested(run) }).toEqual({
        value,
        code: VALIDATION_EXIT_CODE,
        stderr: { error: { code: INVALID_ARGUMENT, message: `invalid fleet_id: ${expected} (got ${JSON.stringify(value)})` } },
        requests: [],
      });
    }
  });

  test("fleet_steer_never_steers_a_fleet_the_lookup_did_not_name", async () => {
    // A daemon that predates the filter ignores `?name=` and answers its
    // newest fleets, whatever their names.
    const unfiltered = (...items: ReadonlyArray<ReturnType<typeof row>>): MockRoutes => ({
      [`GET ${FLEETS}`]: () => jsonResponse(200, page(...items)),
      ...steerRoutes(FLEET_ID),
      ...steerRoutes(OTHER_FLEET_ID),
    });
    const newestFirst = await steer(["@agentbob", "retry", "--json"],
      unfiltered(row(OTHER_FLEET_ID, "release-notes"), row(FLEET_ID, NAME)));
    expect(newestFirst.code).toBe(0);
    expect(requested(newestFirst)).toEqual([
      LOOKUP, `GET ${FLEETS}/${FLEET_ID}/events/stream`, `POST ${FLEETS}/${FLEET_ID}/messages`,
    ]);

    const nameAbsent = await steer(["@agentbob", "retry", "--json"], unfiltered(row(OTHER_FLEET_ID, "release-notes")));
    expect(nameAbsent.code).toBe(VALIDATION_EXIT_CODE);
    expect(requested(nameAbsent)).toEqual([LOOKUP]);
    expect(JSON.parse(nameAbsent.stderr).error).toMatchObject({ code: INVALID_ARGUMENT, message: UNKNOWN_NAME });
  });

  test("fleet_steer_prompt_resolves_a_name_once_and_stays_on_that_fleet", async () => {
    const rec = makeRecorder();
    const reply = <T>(input: HttpRequestInput): T =>
      (input.method === POST ? { event_id: EVENT_ID } : page(row(FLEET_ID, NAME))) as T;
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(`@${NAME}`, undefined, { forceTty: true }, {
        stdin: streamFrom([], true),
        stdout: nullOutput(),
        streamGet: eventStream([
          { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
        ]),
        runRepl: async ({ runTurn }) => {
          await runTurn("retry", new AbortController().signal);
          await runTurn("report back when it passes", new AbortController().signal);
          return 0;
        },
      }).pipe(Effect.provide(makeLayer(rec, reply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const turn = [`POST ${FLEETS}/${FLEET_ID}/messages`, `GET ${FLEETS}/${FLEET_ID}/events/${EVENT_ID}`];
    expect(rec.requests.map((request) => `${request.method ?? "GET"} ${request.path}`)).toEqual([
      `GET ${FLEETS}?name=agentbob`, ...turn, ...turn,
    ]);
    expect(rec.requests.filter((request) => request.method === POST).map((request) => request.body)).toEqual([
      { message: "retry" }, { message: "report back when it passes" },
    ]);
  });
});
