import { expect, it } from "vitest";
import { DefinitionSchema, FleetSchema } from "@/lib/api/draft-types";
import { FLEET } from "./helpers/draft-fixtures";
import { DEFAULT_DAILY_NANOS, initialDefinition } from "@/components/domain/chat/chat-defaults";
import { fixtureDefinition } from "./e2e/acceptance/fixtures/definition";

it.each([undefined, null, "", 7])("refuses a missing, empty or non-string description (%s)", (description) => {
  expect(DefinitionSchema.safeParse({schema_version: 1, description, instructions_md: "", config: {name: "probe"}}).success).toBe(false);
});

it("creates chat and acceptance documents with a description and the fleet name in config", () => {
  const definition = initialDefinition();
  expect(definition).toEqual({schema_version: 1, description: "A fleet created in chat", instructions_md: "",
    config: {name: "my-fleet", "x-agentsfleet": {triggers: [], tools: [], budget: {daily_nanos: DEFAULT_DAILY_NANOS}}}});
  expect(DefinitionSchema.parse(definition)).toEqual(definition);
  const fixture = fixtureDefinition("acceptance-probe");
  expect(DefinitionSchema.parse(fixture)).toEqual({schema_version: 1, description: "Acceptance fixture",
    instructions_md: "Acknowledge the request.", config: fixture.config});
  expect(fixture.config.name).toBe("acceptance-probe");
});

const SAVED_WEBHOOK_URL = "https://delivery.example.test/daemon-provided/path?token=public";
it("preserves deployment-provided webhook URLs from Save without deriving an endpoint", () => {
  const webhook_urls = [{source: "github", url: SAVED_WEBHOOK_URL}];
  expect(FleetSchema.parse({...FLEET, webhook_urls}).webhook_urls).toEqual(webhook_urls);
});
it.each([undefined, null, [{source: "github", url: "/guessed-relative-url"}], [{source: "github"}]].map((webhook_urls) => ({webhook_urls})))(
  "rejects malformed Save webhook URLs (%j)", ({webhook_urls}) => {
    expect(FleetSchema.safeParse({...FLEET, webhook_urls}).success).toBe(false);
  },
);
