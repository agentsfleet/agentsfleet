import { afterEach, beforeEach, expect, test } from "bun:test";
import { PlatformFixture, INSTANT, LIBRARY_ENTRY, MODEL, RUNNER_ID, WORKSPACE_ID } from "./helpers-platform.ts";

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

test("lists and inspects staged Library entries", async () => {
  fixture.respond = () => Response.json({entries: [LIBRARY_ENTRY]});
  const listed = await fixture.run(["fleet-library", "list", "--json"]);
  expect(listed.code).toBe(0); expect(JSON.parse(listed.stdout).entries[0].published).toBe(false);
  const shown = await fixture.run(["fleet-library", "show", RUNNER_ID, "--json"]);
  expect(shown.code).toBe(0); expect(JSON.parse(shown.stdout).card.etag).toBe('"inspected"');
  const missing = await fixture.run(["fleet-library", "show", "missing", "--json"]);
  expect(missing.code).toBe(4); expect(missing.stderr).toContain("not found");
});

test("stages a private candidate with an explicit replay identity", async () => {
  const publication = {library_id: RUNNER_ID, revision_id: WORKSPACE_ID, head_revision_id: WORKSPACE_ID, etag: '"staged"'};
  fixture.respond = () => Response.json(publication);
  const result = await fixture.run(["fleet-library", "stage", "--workspace", WORKSPACE_ID,
    "--candidate-id", RUNNER_ID, "--label", "Support triage", "--operation-id", RUNNER_ID, "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout)).toEqual(publication);
  expect(fixture.calls[0]?.headers.get("Idempotency-Key")).toBe(RUNNER_ID);
  expect(fixture.calls[0]?.body).toEqual({workspace_id: WORKSPACE_ID, draft_candidate_id: RUNNER_ID, label: "Support triage"});
});

test.each([["publish", true], ["unpublish", false]] as const)("curates %s against the inspected version", async (command, published) => {
  fixture.respond = () => Response.json({...LIBRARY_ENTRY, published});
  const result = await fixture.run(["fleet-library", command, RUNNER_ID, "--if-match", '"inspected"', "--json"]);
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({published});
  expect(fixture.calls[0]?.headers.get("If-Match")).toBe('"inspected"');
});

test("updates Library metadata without publishing and deletes conditionally", async () => {
  fixture.respond = call => call.method === "DELETE" ? new Response(null, {status: 204}) : Response.json(LIBRARY_ENTRY);
  const result = await fixture.run(["fleet-library", "update", RUNNER_ID, "--file", "-", "--if-match", '"inspected"', "--json"], '{"description":"Reviewed description"}');
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({description: "Reviewed description"});
  const deleted = await fixture.run(["fleet-library", "delete", RUNNER_ID, "--if-match", '"inspected"', "--json"]);
  expect(deleted.code).toBe(0); expect(fixture.calls[1]?.headers.get("If-Match")).toBe('"inspected"');
});

test.each(["{", "{}", '{"published":true}', '{"name":""}'])("refuses invalid metadata input %s", async input => {
  const result = await fixture.run(["fleet-library", "update", RUNNER_ID, "--file", "-", "--if-match", '"inspected"', "--json"], input);
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});

test("retains the caller's inspected tag on a stale Library refusal", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-POLICY-001", user_message: "The entry changed"}, {status: 412});
  const result = await fixture.run(["fleet-library", "publish", RUNNER_ID, "--if-match", '"stale"', "--json"]);
  expect(result.code).not.toBe(0); expect(result.stderr).toContain("UZ-POLICY-001");
  expect(fixture.calls).toHaveLength(1); expect(fixture.calls[0]?.headers.get("If-Match")).toBe('"stale"');
});

test("reads provider defaults without credential material", async () => {
  fixture.respond = () => Response.json({keys: [{provider: "example", source_workspace_id: WORKSPACE_ID, model: "reasoning-model", active: true, updated_at: INSTANT}]});
  const result = await fixture.run(["keys", "list", "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout).keys[0].model).toBe("reasoning-model");
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/admin/platform-keys");
});

test("selects a provider default from a workspace and deletes it by encoded provider", async () => {
  fixture.respond = call => call.method === "DELETE" ? new Response(null, {status: 204}) : Response.json({provider: "example", model: "reasoning-model", active: true});
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID, "--model", "reasoning-model", "--json"]);
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({provider: "example", source_workspace_id: WORKSPACE_ID, model: "reasoning-model"});
  const deleted = await fixture.run(["keys", "delete", "custom/provider", "--json"]);
  expect(deleted.code).toBe(0); expect(fixture.calls[1]?.url.pathname).toBe("/v1/admin/platform-keys/custom%2Fprovider");
});

test("stores a piped provider credential before selecting its default", async () => {
  const secret = "unit-" + "credential";
  fixture.respond = call => call.method === "POST" ? new Response(null, {status: 204}) : Response.json({provider: "example", model: "reasoning-model", active: true});
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID, "--model", "reasoning-model", "--api-key-stdin", "--json"], secret);
  expect(result.code).toBe(0); expect(fixture.calls.map(call => call.method)).toEqual(["POST", "PUT"]);
  expect(fixture.calls[0]?.body).toMatchObject({data: {api_key: secret}}); expect(result.stdout + result.stderr).not.toContain(secret);
});

test("replaces an existing provider secret without releasing its encoded name", async () => {
  const secret = "unit-" + "replacement";
  fixture.respond = () => Response.json({provider: "custom/provider", model: "reasoning-model", active: true});
  const result = await fixture.run(["keys", "set", "--provider", "custom/provider", "--workspace", WORKSPACE_ID,
    "--model", "reasoning-model", "--api-key-stdin", "--replace", "--json"], secret);
  expect(result.code).toBe(0); expect(fixture.calls.map(call => call.method)).toEqual(["PUT", "PUT"]);
  expect(fixture.calls[0]?.url.pathname).toBe(`/v1/workspaces/${WORKSPACE_ID}/secrets/custom%2Fprovider`);
  expect(fixture.calls[0]?.body).toEqual({data: {provider: "custom/provider", api_key: secret, model: "reasoning-model"}});
  expect(result.stdout + result.stderr).not.toContain(secret);
});

test("recovers after secret creation succeeds but selecting the default fails", async () => {
  const secret = "unit-" + "recoverable";
  let saved: unknown;
  let selections = 0;
  fixture.respond = call => {
    if (call.url.pathname.includes("/secrets")) {
      saved = call.body;
      return new Response(null, {status: 204});
    }
    return ++selections === 1
      ? Response.json({error_code: "UZ-PROVIDER-004", user_message: "Unknown model"}, {status: 400})
      : Response.json({provider: "example", model: "reasoning-model", active: true});
  };
  const args = ["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID, "--api-key-stdin", "--json"];
  const failed = await fixture.run([...args, "--model", "missing-model"], secret);
  expect(failed.code).not.toBe(0); expect(JSON.parse(failed.stderr).error.code).toBe("UZ-PROVIDER-004");
  expect(saved).toMatchObject({name: "example", data: {api_key: secret}});
  const recovered = await fixture.run([...args, "--model", "reasoning-model", "--replace"], secret);
  expect(recovered.code).toBe(0); expect(saved).toEqual({data: {provider: "example", api_key: secret, model: "reasoning-model"}});
  expect(fixture.calls.map(call => call.method)).toEqual(["POST", "PUT", "PUT", "PUT"]);
  expect(failed.stdout + failed.stderr + recovered.stdout + recovered.stderr).not.toContain(secret);
});

test("requires explicit replacement when a provider secret already exists", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-POLICY-001", user_message: "Secret already exists"}, {status: 409});
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID,
    "--model", "reasoning-model", "--api-key-stdin", "--json"], "unit-" + "credential");
  expect(result.code).not.toBe(0); expect(fixture.calls).toHaveLength(1); expect(fixture.calls[0]?.method).toBe("POST");
});

test("refuses replacement without a piped key before making a request", async () => {
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID,
    "--model", "reasoning-model", "--replace", "--json"]);
  expect(result.code).toBe(4); expect(result.stderr).toContain("--replace requires --api-key-stdin"); expect(fixture.calls).toEqual([]);
});

test.each(["", " ", "x".repeat(65537)])("refuses an empty or oversized piped provider credential", async secret => {
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", WORKSPACE_ID, "--model", "reasoning-model", "--api-key-stdin", "--json"], secret);
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});
