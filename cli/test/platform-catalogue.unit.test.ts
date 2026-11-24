import { afterEach, beforeEach, expect, test } from "bun:test";
import { PlatformFixture, INSTANT, MODEL, RUNNER_ID, WORKSPACE_ID } from "./helpers-platform.ts";

const MODEL_INPUT = {provider: MODEL.provider, model_id: MODEL.model_id, metadata: MODEL.metadata, prices: MODEL.prices};

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test("lists catalogue metadata and pages a show request until its row is found", async () => {
  fixture.respond = call => Response.json(call.url.searchParams.has("starting_after")
    ? {models: [MODEL], next_cursor: null} : {models: [], next_cursor: WORKSPACE_ID});
  const shown = await fixture.run(["models", "show", RUNNER_ID, "--json"]);
  expect(shown.code).toBe(0); expect(JSON.parse(shown.stdout)).toEqual(MODEL);
  expect(fixture.calls.map(call => call.url.search)).toEqual(["", `?starting_after=${WORKSPACE_ID}`]);
  const list = await fixture.run(["models", "list", "--limit", "50", "--json"]);
  expect(list.code).toBe(0); expect(JSON.parse(list.stdout).next_cursor).toBe(WORKSPACE_ID);
});

test.each([null, WORKSPACE_ID])("refuses an absent or cyclic model catalogue cursor %s", async cursor => {
  fixture.respond = () => Response.json({models: [], next_cursor: cursor});
  const result = await fixture.run(["models", "show", RUNNER_ID, "--json"]);
  expect(result.code).toBe(4); expect(result.stderr).toContain(cursor === null ? "not found" : "could not be read completely");
  expect(fixture.calls).toHaveLength(cursor === null ? 1 : 2);
});

test("creates and updates a priced catalogue model from JSON", async () => {
  fixture.respond = call => Response.json(call.method === "POST" ? MODEL : {id: RUNNER_ID, updated: true});
  const {id: _id, ...input} = MODEL;
  const created = await fixture.run(["models", "create", "--file", "-", "--json"], JSON.stringify(input));
  expect(created.code).toBe(0); expect(fixture.calls[0]).toMatchObject({method: "POST", body: input});
  const updated = await fixture.run(["models", "update", RUNNER_ID, "--file", "-", "--json"], JSON.stringify({metadata: MODEL.metadata, prices: MODEL.prices}));
  expect(updated.code).toBe(0); expect(fixture.calls[1]?.method).toBe("PATCH");
});

test.each([{}, {...MODEL_INPUT, prices: []}, {...MODEL_INPUT, prices: [MODEL.prices[0], MODEL.prices[0]]},
  {...MODEL_INPUT, prices: [{...MODEL.prices[0], input_nanos_per_mtok: "-1"}]}])("refuses invalid catalogue input without writing", async input => {
  const result = await fixture.run(["models", "create", "--file", "-", "--json"], JSON.stringify(input));
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});

test("surfaces a scoped model refusal without retrying", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-AUTH-022", user_message: "Missing model administration scope"}, {status: 403});
  const result = await fixture.run(["models", "delete", RUNNER_ID, "--json"]);
  expect(result.code).toBe(3);
  expect(JSON.parse(result.stderr).error).toMatchObject({code: "UZ-AUTH-022", message: "Missing model administration scope",
    suggestion: "ask your administrator to grant the required permissions"});
  expect(result.stdout).toBe(""); expect(fixture.calls).toHaveLength(1);
});

test.each(["list", "show", "stage", "update", "publish", "unpublish", "delete"])("platform_catalogue_routes_and_commands_are_removed: %s", async verb => {
  const result = await fixture.run(["fleet-library", verb, "--json"]);
  expect(result.code).toBe(4);
  expect(JSON.parse(result.stderr).error.code).toBe("UNKNOWN_COMMAND");
  expect(fixture.calls.map(call => `${call.method} ${call.url.pathname}`)).toEqual(["GET /v1/users/me"]);
});

test("reads provider defaults without credential material", async () => {
  fixture.respond = () => Response.json({keys: [{provider: "example", source_workspace_id: WORKSPACE_ID, model: "reasoning-model", active: true, updated_at: INSTANT}]});
  const result = await fixture.run(["models", "default", "list", "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout).keys[0].model).toBe("reasoning-model");
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/admin/platform-keys");
});

test("selects a provider default from a workspace and deletes it by encoded provider", async () => {
  fixture.respond = call => call.method === "DELETE" ? new Response(null, {status: 204}) : Response.json({provider: "example", model: "reasoning-model", active: true});
  const result = await fixture.run(["models", "default", "use", "--provider", "example", "--workspace", WORKSPACE_ID, "--model", "reasoning-model", "--json"]);
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({provider: "example", source_workspace_id: WORKSPACE_ID, model: "reasoning-model"});
  const deleted = await fixture.run(["models", "default", "delete", "custom/provider", "--json"]);
  expect(deleted.code).toBe(0); expect(fixture.calls[1]?.url.pathname).toBe("/v1/admin/platform-keys/custom%2Fprovider");
});

test("retries default selection after adding the missing priced model", async () => {
  const args = ["models", "default", "use", "--provider", "example", "--workspace", WORKSPACE_ID,
    "--model", "reasoning-model", "--json"];
  fixture.respond = () => Response.json({error_code: "UZ-PROVIDER-004", user_message: "Model is not in the catalogue"}, {status: 400});
  const failed = await fixture.run(args);
  expect(failed.code).not.toBe(0);
  expect(JSON.parse(failed.stderr).error.code).toBe("UZ-PROVIDER-004");

  fixture.respond = () => Response.json({provider: "example", model: "reasoning-model", active: true});
  const recovered = await fixture.run(args);
  expect(recovered.code).toBe(0);
  expect(fixture.calls.map(call => [call.method, call.url.pathname])).toEqual([
    ["PUT", "/v1/admin/platform-keys"], ["PUT", "/v1/admin/platform-keys"],
  ]);
  expect(fixture.calls[0]?.body).toEqual(fixture.calls[1]?.body);
});

test("default selection requires a model and makes no secret write", async () => {
  const result = await fixture.run(["models", "default", "use", "--provider", "example", "--workspace", WORKSPACE_ID, "--json"]);
  expect(result.code).toBe(4);
  expect(result.stderr).toContain("model");
  expect(fixture.calls).toHaveLength(0);
});
