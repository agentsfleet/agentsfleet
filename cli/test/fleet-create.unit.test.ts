import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit } from "effect";
import { runCli } from "../src/cli.ts";
import { fleetCreateEffect } from "../src/commands/fleet/create.ts";
import { ServerError, ValidationError } from "../src/errors/index.ts";
import { isValidId } from "../src/lib/id.ts";
import { exitCodeForFailure } from "../src/program/entry/exit-code.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { authedScope, makeLayer, makeRecorder, WS_ID } from "./helpers-fleet-steer.ts";
import { jsonResponse, withMockApi } from "./helpers-mock-api.ts";

const NEW_FLEET_ID = "01910000-0000-7000-8000-000000c0ea7e";
const FIRST_TURN_ID = "01910000-0000-7000-8000-000000c0ea7f";
const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const CREATED = {
  fleet: { id: NEW_FLEET_ID, name: "fleet-0000c0ea7e", status: "active" },
  accepted: { turn_id: FIRST_TURN_ID, disposition: "queued" },
  replayed: false,
};
const created = <T>(): T => CREATED as T;

const failureOf = (exit: Exit.Exit<void, unknown>): unknown => {
  if (Exit.isSuccess(exit)) throw new Error("expected the create to fail");
  return Cause.squash(exit.cause);
};

describe("fleet create", () => {
  test.each([
    ["no argument", undefined],
    ["an empty argument", ""],
    ["whitespace only", " \t\n"],
  ] as const)("fleet create with no message is a usage error: %s", async (_case, message) => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(fleetCreateEffect(message, {}).pipe(Effect.provide(makeLayer(rec, created))));
    const error = failureOf(exit);
    expect(error).toBeInstanceOf(ValidationError);
    expect(error).toMatchObject({
      detail: "message is required",
      suggestion: 'usage: agentsfleet fleet create "<message>" [--name <name>] [--model <ref>]',
    });
    if (Exit.isFailure(exit)) expect(exitCodeForFailure(exit.cause)).toBe(4);
    expect(rec.requests).toEqual([]);
  });

  test("sends the first message with a fresh operation id and prints the new ids as JSON", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      fleetCreateEffect("watch checkout errors", {}).pipe(Effect.provide(makeLayer(rec, created))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.requests.map((request) => [request.method, request.path])).toEqual([["POST", FLEETS]]);
    const body = rec.requests[0]?.body as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual(["message", "operation_id"]);
    expect(body.message).toBe("watch checkout errors");
    expect(isValidId(body.operation_id)).toBe(true);
    expect(rec.stdout).toEqual([JSON.stringify({ fleet_id: NEW_FLEET_ID, turn_id: FIRST_TURN_ID })]);
  });

  test("passes --name and --model as name and model_ref", async () => {
    const rec = makeRecorder();
    await Effect.runPromise(fleetCreateEffect("triage support tickets", { name: "support-triage", model: "owner/runtime" })
      .pipe(Effect.provide(makeLayer(rec, created))));
    expect(rec.requests[0]?.body).toMatchObject({
      message: "triage support tickets", name: "support-triage", model_ref: "owner/runtime",
    });
  });

  test("mints a new operation id for every create", async () => {
    const rec = makeRecorder();
    const layer = makeLayer(rec, created);
    await Effect.runPromise(fleetCreateEffect("one", {}).pipe(Effect.provide(layer)));
    await Effect.runPromise(fleetCreateEffect("one", {}).pipe(Effect.provide(layer)));
    const [first, second] = rec.requests.map((request: HttpRequestInput) => (request.body as { operation_id: string }).operation_id);
    expect(first).not.toBe(second);
  });

  test("tells a terminal the fleet started and how to talk to it", async () => {
    const rec = makeRecorder();
    await Effect.runPromise(fleetCreateEffect("watch checkout errors", {})
      .pipe(Effect.provide(makeLayer(rec, created, false, { stdoutIsTty: true }))));
    expect(rec.stdout).toEqual([
      `Started fleet fleet-0000c0ea7e (${NEW_FLEET_ID}). First turn ${FIRST_TURN_ID}: queued.`,
      `Talk to it with: agentsfleet chat ${NEW_FLEET_ID}`,
    ]);
  });

  test.each([
    ["no turn", { fleet: CREATED.fleet, replayed: false }],
    ["a fleet without a name", { ...CREATED, fleet: { id: NEW_FLEET_ID } }],
    ["an unknown disposition", { ...CREATED, accepted: { turn_id: FIRST_TURN_ID, disposition: "folded" } }],
    ["a null body", null],
  ] as const)("refuses an answer with %s and prints nothing", async (_case, answer) => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(fleetCreateEffect("watch checkout errors", {})
      .pipe(Effect.provide(makeLayer(rec, <T>() => answer as T))));
    const error = failureOf(exit);
    expect(error).toBeInstanceOf(ServerError);
    expect(error).toMatchObject({ code: "FLEET_CREATE_RESPONSE_INVALID", status: 502 });
    expect(rec.stdout).toEqual([]);
  });
});

describe("fleet create through the command line", () => {
  const run = (apiUrl: string, argv: ReadonlyArray<string>) => {
    const out = bufferStream();
    const err = bufferStream();
    return runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) })
      .then((code) => ({ code, out: out.read(), err: err.read() }));
  };

  test("--json prints the fleet and turn ids the daemon answered", async () => {
    await authedScope(async () => {
      await withMockApi({ [`POST ${FLEETS}`]: () => jsonResponse(201, CREATED) }, async (apiUrl, calls) => {
        const result = await run(apiUrl, ["fleet", "create", "watch checkout errors", "--name", "checkout-watch", "--json"]);
        expect(result.code).toBe(0);
        expect(JSON.parse(result.out)).toEqual({ fleet_id: NEW_FLEET_ID, turn_id: FIRST_TURN_ID });
        expect(JSON.parse(calls[0]?.body ?? "{}")).toMatchObject({ message: "watch checkout errors", name: "checkout-watch" });
      });
    });
  });

  test("a create with no message sends nothing", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const result = await run(apiUrl, ["fleet", "create"]);
        expect(result.code).toBe(4);
        expect(result.err).toContain("message is required");
        expect(calls.filter((call) => call.path.startsWith(FLEETS))).toEqual([]);
      });
    });
  });

  test("a refused create exits non-zero with the daemon's code and writes no ids", async () => {
    await authedScope(async () => {
      const refusal = () => jsonResponse(409, { error_code: "UZ-SETUP-008", detail: "No usable model", request_id: "req_create" });
      await withMockApi({ [`POST ${FLEETS}`]: refusal }, async (apiUrl) => {
        const result = await run(apiUrl, ["--json", "fleet", "create", "watch checkout errors"]);
        expect(result.code).toBe(3);
        expect(result.out).toBe("");
        expect(JSON.parse(result.err).error).toMatchObject({ code: "UZ-SETUP-008", request_id: "req_create" });
      });
    });
  });
});
