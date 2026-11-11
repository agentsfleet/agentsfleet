import {afterEach, beforeEach, expect, test} from "bun:test";
import {PlatformFixture, MODEL, RUNNER_ID, INSTANT} from "./helpers-platform.ts";

const threshold = {probability_bps: 7000, confidence_bps: 5000};
const profile = {
  revision: "reviewed-1", api: "typesafe", endpoint: "https://example.test/decide",
  accepted_models: ["reviewed-model"], capabilities: {noul: true, choice: true, score: true}, input_max_bytes: 16384,
  intent: {questions_revision: "dt-routing-v1", intent: threshold, association: threshold,
    single_request_max_bps: 4000, multiple_request_min_bps: 6000},
  goal: {questions_revision: "dt-goal-v1", statement_min_bps: 7000, continue_action: threshold, wait: threshold, done: threshold},
  evaluation_digest: "a".repeat(64), evaluation_source_url: "https://example.test/evaluation", evaluated_at: INSTANT,
};
const decision = {enabled: true, profile, prices: MODEL.prices};
let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test.each(["create", "update"])("%s supports decision-only models with independent rates", async command => {
  fixture.respond = () => Response.json(command === "create" ? {...MODEL, prices: [], decision} : {id: RUNNER_ID, updated: true});
  const input = {metadata: MODEL.metadata, prices: [], decision,
    ...command === "create" ? {provider: MODEL.provider, model_id: MODEL.model_id} : {}};
  const result = await fixture.run(["models", command, ...command === "update" ? [RUNNER_ID] : [], "--file", "-", "--json"], JSON.stringify(input));
  expect(result.code).toBe(0); expect(fixture.calls).toHaveLength(1); expect(fixture.calls[0]?.body).toEqual(input);
  if (command === "create") expect(JSON.parse(result.stdout).decision).toEqual(decision);
});

test("metadata-only updates preserve an existing decision definition by omission", async () => {
  fixture.respond = () => Response.json({id: RUNNER_ID, updated: true});
  const input = {metadata: MODEL.metadata, prices: []};
  const result = await fixture.run(["models", "update", RUNNER_ID, "--file", "-", "--json"], JSON.stringify(input));
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual(input);
});

test.each([
  {...decision, prices: []},
  {...decision, prices: [MODEL.prices[0], MODEL.prices[0]]},
  {...decision, profile: {...profile, api: "unknown"}},
  {...decision, profile: {...profile, input_max_bytes: 24577}},
  {...decision, profile: {...profile, accepted_models: []}},
  {...decision, profile: {...profile, goal: undefined}},
  {...decision, profile: {...profile, evaluation_digest: "invalid"}},
  {...decision, profile: {...profile, intent: {...profile.intent, single_request_max_bps: 6000}}},
  {...decision, profile: {...profile, goal: {...profile.goal, statement_min_bps: 10001}}},
  {...decision, profile: {...profile, extra: true}},
])("refuses malformed decision definitions before HTTP", async invalid => {
  const result = await fixture.run(["models", "create", "--file", "-", "--json"], JSON.stringify({
    provider: MODEL.provider, model_id: MODEL.model_id, metadata: MODEL.metadata, prices: [], decision: invalid,
  }));
  expect(result.code).toBe(4); expect(fixture.calls).toHaveLength(0);
});

test("inference-enabled models still require coding prices", async () => {
  const result = await fixture.run(["models", "create", "--file", "-", "--json"], JSON.stringify({
    provider: MODEL.provider, model_id: MODEL.model_id, metadata: MODEL.metadata, prices: [], decision,
    inference: {api: "chat", base_url: "https://example.test/v1", max_output_tokens: 1024},
  }));
  expect(result.code).toBe(4); expect(fixture.calls).toHaveLength(0);
});
