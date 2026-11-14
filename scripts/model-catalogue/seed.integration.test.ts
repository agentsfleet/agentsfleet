import { afterAll, afterEach, beforeEach, describe, expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { runSeedCommand } from "./command";
import { CATALOGUE_URL, readCatalogue } from "./catalogue";
import { CATALOGUE_DATABASE_URL, NO_OTHER_CLIENTS, openIntegrationDatabase, removeFixtureModels, snapshot, version, waitForClientCount } from "./integration-fixtures";
import { seedCatalogue } from "./seed";
import { desiredRows } from "./rows";
import { APPLY_FLAG, catalogueFixture, MODEL_NAME, modelFixture, priceFixture } from "./test-fixtures";
import { SELECT_ROWS, UPSERT_MODELS } from "./statements";

const database = openIntegrationDatabase();
const KEY = `fireworks/${MODEL_NAME}`;
const EXTRA_MODEL = "pruned-model";
const FILES: string[] = [];
const TEST_FILE_PREFIX = `${tmpdir()}/agentsfleet-seeder-live`;
const CONCURRENT_CONNECTIONS = 8;
const CONCURRENT_APPLIES = 100;
const EXPLAIN_PREFIX = "EXPLAIN (ANALYZE, BUFFERS) ";

describe.skipIf(!CATALOGUE_DATABASE_URL)("real catalogue seeder", () => {
  beforeEach(() => removeFixtureModels(database));
  afterEach(() => removeFixtureModels(database));
  afterAll(async () => {
    await database.close();
    for (const path of FILES) await Bun.file(path).delete();
  });

  test("the complete curated document seeds every proved context on an empty database", async () => {
    const catalogue = await readCatalogue(CATALOGUE_URL);
    expect(catalogue.models).toHaveLength(20);
    expect(catalogue.models.every((model) => model.context !== null)).toBe(true);
    const wanted = catalogue.models.map((model) => `${model.provider}/${model.model_name}`).toSorted();
    const initial = await snapshot(database);
    const diff = await seedCatalogue(database, catalogue);
    expect(diff).toMatchObject({ diff: { changed: [], deleted: [] }, applied: false });
    expect(diff.diff.added.toSorted()).toEqual(wanted);
    expect(await snapshot(database)).toEqual(initial);
    expect((await seedCatalogue(database, catalogue, true)).applied).toBe(true);
    const rows = await database`SELECT provider || '/' || model_name AS identity, inference FROM model.platform_models`;
    expect(rows.map((row: { identity: string }) => row.identity).toSorted()).toEqual(wanted);
    for (const model of catalogue.models) expect(rows.find((row: {identity: string; inference: unknown}) => row.identity === `${model.provider}/${model.model_name}`)?.inference).toEqual(model.inference);
    expect((await seedCatalogue(database, catalogue)).diff).toEqual({ added: [], changed: [], deleted: [] });
  });

  test("empty database diff does not write, apply creates exact identity and dated prices", async () => {
    const catalogue = catalogueFixture();
    const initial = await snapshot(database);
    expect(await seedCatalogue(database, catalogue)).toEqual({
      diff: { added: [KEY], changed: [], deleted: [] }, applied: false,
    });
    expect(await snapshot(database)).toEqual(initial);
    const generation = BigInt(await version(database));
    expect((await seedCatalogue(database, catalogue, true)).applied).toBe(true);
    expect(BigInt(await version(database))).toBe(generation + 1n);
    const models = await database`SELECT provider, model_name, display_name, advanced, context_cap_tokens,
      context_source_url, context_verified_at, created_at = updated_at AS same_clock
      FROM model.platform_models`;
    expect(models).toEqual([{
      provider: "fireworks", model_name: MODEL_NAME, display_name: "GLM 5.3 Flash", advanced: false,
      context_cap_tokens: 1_048_576, context_source_url: modelFixture().context?.source_url,
      context_verified_at: new Date("2026-10-18T12:00:00Z"), same_clock: true,
    }]);
    const prices = await database`SELECT input_nanos_per_mtok::text AS input, cached_input_nanos_per_mtok::text AS cached,
      output_nanos_per_mtok::text AS output, cache_write_nanos_per_mtok, cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok,
      source_url, conditions, effective_until, effective_from FROM model.platform_model_prices`;
    expect(prices).toEqual([{
      input: "150000000", cached: "30000000", output: "500000000", cache_write_nanos_per_mtok: null, cache_write_5m_nanos_per_mtok: null,
      cache_write_1h_nanos_per_mtok: null, source_url: priceFixture().source_url, conditions: priceFixture().conditions,
      effective_until: null, effective_from: new Date("2026-10-18T12:00:00Z"),
    }]);
  });

  test("catalogue reads and connection backfills have measured query plans", async () => {
    const catalogue = await readCatalogue(CATALOGUE_URL);
    await seedCatalogue(database, catalogue, true);
    const identities = desiredRows(catalogue).map(({ prices: _, ...identity }) => ({ id: Bun.randomUUIDv7(), ...identity }));
    await database`UPDATE model.platform_models SET inference = NULL`;
    for (const [name, statement] of [["upsert_models", UPSERT_MODELS], ["select_rows", SELECT_ROWS]]) {
      const query = EXPLAIN_PREFIX + statement;
      const parameters = name === "upsert_models" ? [identities] : [];
      const plan = await database.unsafe(query, parameters);
      const lines = plan.map((row: { "QUERY PLAN": string }) => row["QUERY PLAN"]);
      console.info(`CATALOGUE EXPLAIN ${name}\n${lines.join("\n")}`);
      expect(lines.some((line: string) => line.startsWith("Execution Time:"))).toBe(true);
    }
    expect((await seedCatalogue(database, catalogue)).diff.changed).toEqual([]);
  });

  test("apply backfills a missing connection and preserves an administrator's endpoint on later priming", async () => {
    const catalogue = catalogueFixture();
    await seedCatalogue(database, catalogue, true);
    await database`UPDATE model.platform_models SET inference = NULL`;
    expect((await seedCatalogue(database, catalogue)).diff.changed).toEqual([KEY]);
    expect((await seedCatalogue(database, catalogue, true)).applied).toBe(true);
    expect((await database`SELECT inference FROM model.platform_models`)[0].inference).toEqual(modelFixture().inference);
    const custom = { ...modelFixture().inference, base_url: "https://gateway.example.com/v1" };
    await database.unsafe("UPDATE model.platform_models SET inference = $1::jsonb", [custom]);
    expect((await seedCatalogue(database, catalogue, true)).applied).toBe(false);
    const renamed = catalogueFixture([modelFixture({ display_name: "Renamed" })]);
    expect((await seedCatalogue(database, renamed, true)).applied).toBe(true);
    expect((await database`SELECT inference FROM model.platform_models`)[0].inference).toEqual(custom);
  });

  test("repeat apply preserves identifiers, clocks and generation", async () => {
    await seedCatalogue(database, catalogueFixture(), true);
    const committed = await snapshot(database);
    const result = await seedCatalogue(database, catalogueFixture(), true);
    expect(result).toEqual({ diff: { added: [], changed: [], deleted: [] }, applied: false });
    expect(await snapshot(database)).toEqual(committed);
    expect((await seedCatalogue(database, catalogueFixture())).diff).toEqual(result.diff);
  });

  test("price-only edits retain model identity and store sub-cent rates without rounding", async () => {
    await seedCatalogue(database, catalogueFixture(), true);
    const identity = await database`SELECT id, created_at, updated_at FROM model.platform_models`;
    const wanted = catalogueFixture([modelFixture({ prices: [priceFixture({ input: "0.0375", cached_input: "0.006",
      output: "0.075", cache_write_5m: "0.125", cache_write_1h: "0.1875" })] })]);
    expect((await seedCatalogue(database, wanted, true)).diff.changed).toEqual([KEY]);
    expect(await database`SELECT id, created_at, updated_at FROM model.platform_models`).toEqual(identity);
    expect(Array.from(await database<unknown[]>`SELECT input_nanos_per_mtok::text AS input, cached_input_nanos_per_mtok::text AS cached,
      output_nanos_per_mtok::text AS output, cache_write_5m_nanos_per_mtok::text AS write_5m,
      cache_write_1h_nanos_per_mtok::text AS write_1h FROM model.platform_model_prices`)).toEqual([
      { input: "37500000", cached: "6000000", output: "75000000", write_5m: "125000000", write_1h: "187500000" },
    ]);
    expect((await seedCatalogue(database, wanted)).diff.changed).toEqual([]);
  });

  test("pruned identities and price tiers are deleted with no orphan prices", async () => {
    const extra = modelFixture({ model_name: EXTRA_MODEL });
    const tiered = modelFixture({ prices: [priceFixture(), priceFixture({ prompt_tokens_above: 100_000 })] });
    await seedCatalogue(database, catalogueFixture([tiered, extra]), true);
    const result = await seedCatalogue(database, catalogueFixture(), true);
    expect(result.diff).toEqual({ added: [], changed: [KEY], deleted: [`fireworks/${EXTRA_MODEL}`] });
    expect(Array.from(await database<unknown[]>`SELECT model_name FROM model.platform_models`)).toEqual([{ model_name: MODEL_NAME }]);
    expect(Array.from(await database<unknown[]>`SELECT prompt_tokens_above FROM model.platform_model_prices`)).toEqual([{ prompt_tokens_above: 0 }]);
    expect((await seedCatalogue(database, catalogueFixture())).diff).toEqual({ added: [], changed: [], deleted: [] });
  });

  test("models without exact context evidence never acquire a database row", async () => {
    const wanted = catalogueFixture([modelFixture({ context: null, context_unverified_reason: "Provider API unavailable." })]);
    expect((await seedCatalogue(database, wanted, true)).applied).toBe(false);
    expect(Array.from(await database<unknown[]>`SELECT id FROM model.platform_models`)).toEqual([]);
    expect(Array.from(await database<unknown[]>`SELECT id FROM model.platform_model_prices`)).toEqual([]);
  });

  test("an unspecified cache-write price persists exactly, changes and clears independently", async () => {
    const catalogue = catalogueFixture([modelFixture({ prices: [priceFixture({ cache_write: "0.1875" })] })]);
    expect((await seedCatalogue(database, catalogue, true)).applied).toBe(true);
    expect(Array.from(await database<unknown[]>`SELECT cache_write_nanos_per_mtok::text AS unspecified,
      cache_write_5m_nanos_per_mtok AS write_5m, cache_write_1h_nanos_per_mtok AS write_1h
      FROM model.platform_model_prices`)).toEqual([{ unspecified: "187500000", write_5m: null, write_1h: null }]);
    expect((await seedCatalogue(database, catalogue)).diff.changed).toEqual([]);
    catalogue.models[0]!.prices[0]!.cache_write = "0.0375";
    expect((await seedCatalogue(database, catalogue, true)).diff.changed).toEqual([KEY]);
    expect(Array.from(await database<unknown[]>`SELECT cache_write_nanos_per_mtok::text AS unspecified
      FROM model.platform_model_prices`)).toEqual([{ unspecified: "37500000" }]);
    expect((await seedCatalogue(database, catalogueFixture(), true)).diff.changed).toEqual([KEY]);
    expect(Array.from(await database<unknown[]>`SELECT cache_write_nanos_per_mtok FROM model.platform_model_prices`))
      .toEqual([{ cache_write_nanos_per_mtok: null }]);
  });

  test("concurrent identical applies bump one generation and produce one identity", async () => {
    const generation = BigInt(await version(database));
    const clients = Array.from({ length: CONCURRENT_CONNECTIONS }, openIntegrationDatabase);
    try {
      const results = await Promise.all(Array.from({ length: CONCURRENT_APPLIES }, (_, index) =>
        seedCatalogue(clients[index % clients.length]!, catalogueFixture(), true)));
      expect(results.filter((result) => result.applied)).toHaveLength(1);
    } finally {
      await Promise.all(clients.map((client) => client.close()));
    }
    await waitForClientCount(database, NO_OTHER_CLIENTS);
    expect(BigInt(await version(database))).toBe(generation + 1n);
    expect(Array.from(await database<unknown[]>`SELECT count(*)::integer AS models FROM model.platform_models`)).toEqual([{ models: 1 }]);
    expect(Array.from(await database<unknown[]>`SELECT count(*)::integer AS prices FROM model.platform_model_prices`)).toEqual([{ prices: 1 }]);
  });

  test("the command applies then diffs through separate real connections and closes them", async () => {
    const path = `${TEST_FILE_PREFIX}-${crypto.randomUUID()}.json`;
    FILES.push(path);
    await Bun.write(path, JSON.stringify(catalogueFixture()));
    const lines: string[] = [];
    expect(await runSeedCommand({ args: [APPLY_FLAG], databaseUrl: CATALOGUE_DATABASE_URL, cataloguePath: path }, (line) => lines.push(line))).toBe(0);
    expect(lines.at(-1)).toBe("✓ catalogue and generation committed");
    await waitForClientCount(database, NO_OTHER_CLIENTS);
    expect(await runSeedCommand({ args: [], databaseUrl: CATALOGUE_DATABASE_URL, cataloguePath: path }, (line) => lines.push(line))).toBe(0);
    expect(lines.at(-2)).toBe("0 added; 0 changed; 0 deleted");
    await waitForClientCount(database, NO_OTHER_CLIENTS);
  });

  test("the connection leak check refuses a count that never converges", async () => {
    await expect(waitForClientCount(database, [])).rejects.toThrow("toEqual");
  });
});
