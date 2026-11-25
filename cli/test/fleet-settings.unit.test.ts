// `fleet model use` and `fleet network use`: one PATCH under the tag the read
// before it answered, printing what the daemon then holds.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit } from "effect";
import { runCli } from "../src/cli.ts";
import { fleetModelUseEffect } from "../src/commands/fleet/settings.ts";
import { ValidationError } from "../src/errors/index.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { FLEET } from "./helpers-fleet.ts";
import { authedScope, makeLayer, makeRecorder, WS_ID } from "./helpers-fleet-steer.ts";
import { jsonResponse, withMockApi, type MockCall, type MockRoutes } from "./helpers-mock-api.ts";

const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const FLEET_PATH = `${FLEETS}/${FLEET.id}`;

const run = (argv: ReadonlyArray<string>, routes: MockRoutes, terminal = true) =>
  authedScope(() => withMockApi(routes, async (apiUrl, calls) => {
    const out = bufferStream();
    if (terminal) out.stream.isTTY = true;
    const err = bufferStream();
    const code = await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }) });
    return { code, out: out.read(), err: err.read(), calls };
  }));

const patched = (change: Record<string, unknown>): MockRoutes => ({
  [`GET ${FLEET_PATH}`]: () => jsonResponse(200, FLEET),
  [`PATCH ${FLEET_PATH}`]: () => jsonResponse(200, { ...FLEET, ...change, etag: '"fleet-2"' }),
});

const sent = (calls: ReadonlyArray<MockCall>) => calls.map((call) => `${call.method} ${call.path}`);
const patchOf = (calls: ReadonlyArray<MockCall>) => calls.find((call) => call.method === "PATCH");

describe("fleet model use", () => {
  test("patches model_ref under the read tag and names the model the fleet now runs on", async () => {
    const result = await run(["fleet", "model", "use", FLEET.id, "owner/fast"], patched({ model_ref: "owner/fast" }));
    expect(result.code).toBe(0);
    expect(sent(result.calls)).toEqual([`GET ${FLEET_PATH}`, `PATCH ${FLEET_PATH}`]);
    expect(patchOf(result.calls)?.headers["if-match"]).toBe(FLEET.etag);
    expect(JSON.parse(patchOf(result.calls)?.body ?? "{}")).toEqual({ model_ref: "owner/fast" });
    expect(result.out).toContain("Fleet argo now runs on owner/fast; its next run uses it.");
  });

  test("finds a fleet by @name before it reads and patches it", async () => {
    const page = { items: [{ id: FLEET.id, name: "argo", status: "active", events_processed: "0", budget_used_nanos: "0",
      created_at: "2026-11-25T10:00:00Z" }], next_cursor: null };
    const result = await run(["fleet", "model", "use", "@argo", "owner/fast"],
      { [`GET ${FLEETS}`]: () => jsonResponse(200, page), ...patched({ model_ref: "owner/fast" }) });
    expect(result.code).toBe(0);
    expect(result.calls[0]?.search).toBe("?name=argo");
    expect(sent(result.calls)).toEqual([`GET ${FLEETS}`, `GET ${FLEET_PATH}`, `PATCH ${FLEET_PATH}`]);
  });

  test("an @name no fleet carries changes nothing", async () => {
    const result = await run(["fleet", "model", "use", "@argo", "owner/fast"],
      { [`GET ${FLEETS}`]: () => jsonResponse(200, { items: [], next_cursor: null }) });
    expect(result.code).toBe(4);
    expect(result.err).toContain("No fleet named `argo` in this workspace");
    expect(sent(result.calls)).toEqual([`GET ${FLEETS}`]);
  });

  test("a model the workspace cannot use is the daemon's refusal, and nothing is printed as changed", async () => {
    const result = await run(["--json", "fleet", "model", "use", FLEET.id, "owner/missing"], {
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, FLEET),
      [`PATCH ${FLEET_PATH}`]: () => jsonResponse(409, { error_code: "UZ-SETUP-008", detail: "Model not usable", request_id: "req_model" }),
    });
    expect(result.code).toBe(3);
    expect(result.out).toBe("");
    expect(JSON.parse(result.err).error).toMatchObject({ code: "UZ-SETUP-008", request_id: "req_model" });
  });

  test("a patch answer of another shape is refused", async () => {
    const result = await run(["fleet", "model", "use", FLEET.id, "owner/fast"], {
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, FLEET),
      [`PATCH ${FLEET_PATH}`]: () => jsonResponse(200, { id: FLEET.id, status: "active" }),
    });
    expect(result.code).toBe(3);
    expect(result.err).toContain("FLEET_RESPONSE_INVALID");
    expect(result.out).not.toContain("now runs on");
  });

  test("a stdout that is not a terminal gets the fleet the daemon answered as JSON", async () => {
    const result = await run(["fleet", "model", "use", FLEET.id, "owner/fast"], patched({ model_ref: "owner/fast" }), false);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.out)).toMatchObject({ id: FLEET.id, model_ref: "owner/fast", etag: '"fleet-2"' });
  });

  test.each([["an empty reference", ""], ["a blank reference", "  "]] as const)("%s is a usage error with no request", async (_case, ref) => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(fleetModelUseEffect(FLEET.id, ref).pipe(Effect.provide(makeLayer(rec))));
    if (Exit.isSuccess(exit)) throw new Error("expected a usage refusal");
    const error = Cause.squash(exit.cause);
    expect(error).toBeInstanceOf(ValidationError);
    expect(error).toMatchObject({ detail: "a model reference is required" });
    expect(rec.requests).toEqual([]);
  });
});

describe("fleet network use", () => {
  test.each([
    [["allow-all"], { http_egress: "public" }, "Fleet argo can reach any public site, including package registries."],
    [["private", "--packages"], { http_egress: "restricted", package_domains: true }, "Fleet argo uses private access. Package downloads are allowed."],
    [["private", "--no-packages"], { http_egress: "restricted", package_domains: false }, "Fleet argo uses private access. Package downloads are blocked."],
    [["allow-all", "--no-packages"], { http_egress: "public", package_domains: false }, "Fleet argo can reach any public site, including package registries."],
  ] as const)("%p patches %p", async (words, change, line) => {
    const result = await run(["fleet", "network", "use", FLEET.id, ...words], patched(change));
    expect(result.code).toBe(0);
    expect(sent(result.calls)).toEqual([`GET ${FLEET_PATH}`, `PATCH ${FLEET_PATH}`]);
    expect(patchOf(result.calls)?.headers["if-match"]).toBe(FLEET.etag);
    expect(JSON.parse(patchOf(result.calls)?.body ?? "{}")).toEqual(change);
    expect(result.out).toContain(line);
  });

  test.each([["no mode word", []], ["a mode word it does not know", ["open"]], ["the old switch", ["--allow-all"]]] as const)(
    "%s is refused before any request", async (_case, words) => {
      const result = await run(["fleet", "network", "use", FLEET.id, ...words], {});
      expect(result.code).toBe(4);
      expect(result.calls.filter((call) => call.path.startsWith(FLEETS))).toEqual([]);
    });

  test("an API key opening the network gets the daemon's refusal, and the fleet reads unchanged", async () => {
    const result = await run(["--json", "fleet", "network", "use", FLEET.id, "allow-all"], {
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, FLEET),
      [`PATCH ${FLEET_PATH}`]: () => jsonResponse(403, { error_code: "UZ-AUTH-001", detail: "person identity required", request_id: "req_net" }),
    });
    expect(result.code).toBe(3);
    expect(result.out).toBe("");
    expect(JSON.parse(result.err).error).toMatchObject({ code: "UZ-AUTH-001", request_id: "req_net" });
  });
});
