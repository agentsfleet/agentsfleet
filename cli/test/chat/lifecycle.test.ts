import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option } from "effect";
import { resumeEffectFromId, stopEffectFromId, killEffectFromId, deleteEffectFromId } from "../../src/commands/fleet.ts";
import { CliConfig } from "../../src/services/config.ts";
import { Credentials } from "../../src/services/credentials.ts";
import { httpClientLayer } from "../../src/services/http-client.ts";
import { Output } from "../../src/services/output.ts";
import { Workspaces } from "../../src/services/workspaces.ts";
import { outputDouble } from "../helpers-output-double.ts";
import { FLEET, FLEET_ID, WORKSPACE, TOKEN, problem } from "./fixture.ts";

type Command = typeof resumeEffectFromId;
type Call = { method: string; tag: string | null; body: string };

async function run(command: Command, changed: unknown, refuse?: "GET" | "PATCH") {
  const calls: Call[] = [];
  const messages: string[] = [];
  const server = Bun.serve({hostname: "127.0.0.1", port: 0, async fetch(request) {
    calls.push({method: request.method, tag: request.headers.get("If-Match"), body: await request.text()});
    if (request.method === refuse) return problem(503);
    if (request.method === "DELETE") return new Response(null, {status: 204});
    return Response.json(request.method === "GET" ? FLEET : changed);
  }});
  const config = Layer.succeed(CliConfig, {apiUrl: `http://127.0.0.1:${server.port}`, dashboardUrl: "http://app.test",
    accessToken: Option.some(TOKEN), jsonMode: false, noOpen: true, telemetryPosthogKey: "", telemetryPosthogHost: "http://app.test"});
  try {
    const exit = await Effect.runPromiseExit(command(FLEET_ID).pipe(
      Effect.provide(Layer.provide(httpClientLayer, config)), Effect.provide(config),
      Effect.provide(Layer.succeed(Credentials, {getAccessToken: Effect.succeed(Option.none()),
        snapshot: Effect.die("unexpected snapshot"), saveAccessToken: () => Effect.void, clearAccessToken: Effect.void})),
      Effect.provide(Layer.succeed(Workspaces, {load: Effect.succeed({tenant_id: "tenant", current_workspace_id: WORKSPACE, items: []}), save: () => Effect.void})),
      Effect.provide(Layer.succeed(Output, {...outputDouble(), success: (message) => Effect.sync(() => {messages.push(message);})})),
    ));
    return {calls, messages, exit};
  } finally { await server.stop(true); }
}

describe("reviewed fleet lifecycle", () => {
  test.each([[resumeEffectFromId, "active", "resumed"], [stopEffectFromId, "stopped", "stopped"],
    [killEffectFromId, "killed", "killed"]] as const)("uses the observed tag for %s", async (command, status, label) => {
    const result = await run(command, {...FLEET, status});
    expect(Exit.isSuccess(result.exit)).toBe(true);
    expect(result.calls).toEqual([{method: "GET", tag: null, body: ""},
      {method: "PATCH", tag: FLEET.etag, body: JSON.stringify({status})}]);
    expect(result.messages).toEqual([`${FLEET_ID} ${label}.`]);
  });
  test("a waiting setup is never reported as resumed", async () => {
    const result = await run(resumeEffectFromId, {...FLEET, status: "installing", setup: {state: "waiting", checks: []}});
    expect(Exit.isSuccess(result.exit)).toBe(true);
    expect(result.messages[0]).toContain("setup is still waiting");
    expect(result.messages[0]).not.toContain("resumed");
  });
  test("purge uses the observed resource tag", async () => {
    const result = await run(deleteEffectFromId, null);
    expect(Exit.isSuccess(result.exit)).toBe(true);
    expect(result.calls.map(({method, tag}) => ({method, tag}))).toEqual([
      {method: "GET", tag: null}, {method: "DELETE", tag: FLEET.etag}]);
  });
  test.each(["GET", "PATCH"] as const)("refuses %s failure without success or replay", async (refuse) => {
    const result = await run(resumeEffectFromId, FLEET, refuse);
    expect(Exit.isFailure(result.exit)).toBe(true);
    expect(result.messages).toEqual([]);
    if (refuse === "GET") expect(result.calls.every(({method}) => method === "GET")).toBe(true);
    else expect(result.calls.filter(({method}) => method === "PATCH")).toHaveLength(1);
  });
  test("refuses a malformed mutation response without reporting success", async () => {
    const result = await run(resumeEffectFromId, {});
    expect(Exit.isFailure(result.exit)).toBe(true);
    expect(result.messages).toEqual([]);
  });
});
