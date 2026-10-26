import { afterAll, afterEach, beforeEach, describe, expect, test } from "bun:test";
import { CATALOGUE_DATABASE_URL, openIntegrationDatabase, removeFixtureModels } from "./integration-fixtures";
import { seedCatalogue } from "./seed";
import { catalogueFixture, MODEL_NAME, modelFixture, priceFixture } from "./test-fixtures";

const database = openIntegrationDatabase();
const CRATES = new URL("../../rustd/crates/", import.meta.url);
const FIRST_PAGE = new URL("afd_tenant/src/sql/models/select_library_page_first.sql", CRATES).pathname;
const AFTER_PAGE = new URL("afd_tenant/src/sql/models/select_library_page_after.sql", CRATES).pathname;
const REGISTRY_RATES = new URL("afd_credential/src/provider/entries/sql/select_rates_for_pairs.sql", CRATES).pathname;
const PROVIDER = "fireworks";
const NEXT_MODEL = "accounts/fireworks/models/kimi-k3";
const EXPIRED_MODEL = "expired-model";
const FUTURE_MODEL = "future-model";
const PAST = "2000-01-01T00:00:00Z";
const NEXT_CENTURY = "2100-01-01T00:00:00Z";

describe.skipIf(!CATALOGUE_DATABASE_URL)("real catalogue base-price quotes", () => {
  beforeEach(async () => {
    await removeFixtureModels(database);
    await seedCatalogue(database, catalogueFixture([
      modelFixture({ prices: [priceFixture({ effective_from: PAST }),
        priceFixture({ effective_from: PAST, prompt_tokens_above: 272_000, input: "4" })] }),
      modelFixture({ model_name: NEXT_MODEL, prices: [priceFixture({ effective_from: PAST })] }),
      modelFixture({ model_name: EXPIRED_MODEL, prices: [priceFixture({ effective_from: PAST, effective_until: "2001-01-01T00:00:00Z" })] }),
      modelFixture({ model_name: FUTURE_MODEL, prices: [priceFixture({ effective_from: NEXT_CENTURY })] }),
    ]), true);
  });
  afterEach(() => removeFixtureModels(database));
  afterAll(() => database.close());

  test("the registry quotes one current base rate per exact provider and model", async () => {
    const rows = await database.file(REGISTRY_RATES, [database.array([PROVIDER, PROVIDER, PROVIDER, PROVIDER, "openai"], "TEXT"),
      database.array([MODEL_NAME, NEXT_MODEL, EXPIRED_MODEL, FUTURE_MODEL, MODEL_NAME], "TEXT")]);
    expect(rows).toHaveLength(2);
    expect(rows.map((row: { model_name: string }) => row.model_name).sort()).toEqual([MODEL_NAME, NEXT_MODEL]);
    expect(rows.map((row: { input_nanos_per_mtok: string }) => row.input_nanos_per_mtok)).toEqual(["150000000", "150000000"]);
  });

  test("tenant pages omit expired and future quotes and resume without duplicating a tier", async () => {
    const first = await database.file(FIRST_PAGE, [PROVIDER, 1, false]);
    expect(first).toHaveLength(1);
    expect(first[0].model_name).toBe(MODEL_NAME);
    const after = await database.file(AFTER_PAGE, [PROVIDER, first[0].display_key, first[0].vendor_key, first[0].id, 10, false]);
    expect(after).toHaveLength(1);
    expect(after[0].model_name).toBe(NEXT_MODEL);
    expect(Array.from(await database.file<unknown[]>(FIRST_PAGE, ["openai", 10, false]))).toEqual([]);
  });

  test("a price-only edit advances the tenant page stamp without changing identity", async () => {
    const first = await database.file(FIRST_PAGE, [PROVIDER, 1, false]);
    await database`UPDATE model.platform_model_prices
      SET updated_at = statement_timestamp() + interval '1 second', input_nanos_per_mtok = 37500000
      WHERE platform_model_id = ${first[0].id}::uuid AND prompt_tokens_above = 0`;
    const changed = await database.file(FIRST_PAGE, [PROVIDER, 1, false]);
    expect(changed[0].id).toBe(first[0].id);
    expect(changed[0].input_nanos_per_mtok).toBe("37500000");
    expect(changed[0].updated_at.getTime()).toBeGreaterThan(first[0].updated_at.getTime());
  });
});
