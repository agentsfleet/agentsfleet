import { afterAll, afterEach, beforeEach, describe, expect, test } from "bun:test";
import { CATALOGUE_DATABASE_URL, openIntegrationDatabase, removeFixtureModels, snapshot, version, withWriteRefusal } from "./integration-fixtures";
import { seedCatalogue } from "./seed";
import { catalogueFixture, HAIKU_PROMPT_THRESHOLD, modelFixture, priceFixture } from "./test-fixtures";

const database = openIntegrationDatabase();
const STAGES = [
  { target: "model.platform_models", operation: "insert" },
  { target: "model.platform_model_prices", operation: "insert" },
  { target: "model.platform_model_prices", operation: "delete" },
  { target: "model.platform_models", operation: "delete" },
  { target: "model.catalogue_version", operation: "update" },
  { target: "model.platform_model_prices", operation: "commit" },
] as const;
const CHANGE_AT = "2026-12-01T00:00:00Z";
const CATALOGUE = catalogueFixture([modelFixture({ prices: [
  priceFixture({ effective_until: CHANGE_AT, input: "0.125" }),
  priceFixture({ effective_from: CHANGE_AT, input: "0.1875" }),
] })]);
const INITIAL = catalogueFixture([
  modelFixture({ model_name: "to-prune" }),
  modelFixture({ prices: [priceFixture(), priceFixture({ prompt_tokens_above: HAIKU_PROMPT_THRESHOLD })] }),
]);

describe.skipIf(!CATALOGUE_DATABASE_URL)("real catalogue write failures", () => {
  beforeEach(() => removeFixtureModels(database));
  afterEach(() => removeFixtureModels(database));
  afterAll(() => database.close());

  test.each([...STAGES])("refusal at $target/$operation rolls back every effect and retry heals", async ({ target, operation }) => {
    await seedCatalogue(database, INITIAL, true);
    const before = await snapshot(database);
    const generation = BigInt(await version(database));
    await withWriteRefusal(database, target, operation, async () => {
      await expect(seedCatalogue(database, CATALOGUE, true)).rejects.toMatchObject({ errno: "P0001", message: "catalogue_test_refusal" });
      expect(await snapshot(database)).toEqual(before);
    });
    expect((await seedCatalogue(database, CATALOGUE, true)).applied).toBe(true);
    expect(BigInt(await version(database))).toBe(generation + 1n);
    expect((await seedCatalogue(database, CATALOGUE)).diff).toEqual({ added: [], changed: [], deleted: [] });
  });
});
