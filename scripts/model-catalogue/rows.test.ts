import { describe, expect, test } from "bun:test";
import { parseCatalogue } from "./catalogue";
import { catalogueDiff, desiredRows, hasChanges, modelRowsSchema } from "./rows";
import { seedReport } from "./seed";
import { catalogueFixture, HAIKU_PROMPT_THRESHOLD, MODEL_NAME, modelFixture, priceFixture } from "./test-fixtures";

const KEY = `fireworks/${MODEL_NAME}`;
// pin test: literal is the contract
const NANOS = ["150000000", "30000000", "500000000"];

describe("seeder rows", () => {
  test("only proved contexts become identities and exact integer price rows", () => {
    const catalogue = parseCatalogue(catalogueFixture([modelFixture(), modelFixture({ model_name: "accounts/fireworks/models/kimi-k3",
      context: null, context_unverified_reason: "No authenticated models API evidence." })]));
    const rows = desiredRows(catalogue);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.model_name).toBe(MODEL_NAME);
    expect(rows[0]?.context_verified_at).toBe("2026-10-18T12:00:00.000Z");
    const price = rows[0]?.prices[0];
    expect([price?.input_nanos_per_mtok, price?.cached_input_nanos_per_mtok, price?.output_nanos_per_mtok]).toEqual(NANOS);
    expect(price?.cache_write_5m_nanos_per_mtok).toBeNull();
    expect(price?.cache_write_1h_nanos_per_mtok).toBeNull();
  });

  test("cache-write lifetimes remain separate and price ordering is canonical", () => {
    const rows = desiredRows(catalogueFixture([modelFixture({ prices: [
      priceFixture({ prompt_tokens_above: HAIKU_PROMPT_THRESHOLD }),
      priceFixture({ effective_from: "2026-12-01T00:00:00Z", cache_write_5m: "0.125", cache_write_1h: "0.20" }),
      priceFixture(),
    ] })]));
    expect(rows[0]?.prices.map((price) => price.prompt_tokens_above)).toEqual([0, 0, HAIKU_PROMPT_THRESHOLD]);
    expect(rows[0]?.prices[1]?.cache_write_5m_nanos_per_mtok).toBe("125000000");
    expect(rows[0]?.prices[1]?.cache_write_1h_nanos_per_mtok).toBe("200000000");
  });

  test("a default diff includes additions and leaves the database untouched", () => {
    const rows = desiredRows(catalogueFixture());
    const diff = catalogueDiff(rows, []);
    expect(diff).toEqual({ added: [KEY], changed: [], deleted: [] });
    expect(hasChanges(diff)).toBe(true);
    expect(seedReport({ diff, applied: false })).toEqual([
      `  + ${KEY}`, "1 added; 0 changed; 0 deleted", "✓ diff only; no rows written",
    ]);
    expect(catalogueDiff(rows, structuredClone(rows))).toEqual({ added: [], changed: [], deleted: [] });
    expect(hasChanges(catalogueDiff(rows, rows))).toBe(false);
  });

  test("changed metadata, prices and pruned identities appear in the reviewed diff", () => {
    const old = desiredRows(catalogueFixture());
    const wanted = desiredRows(catalogueFixture([modelFixture({ display_name: "Provider display name",
      prices: [priceFixture({ cached_input: "0.006" })] })]));
    const diff = catalogueDiff(wanted, old);
    expect(diff).toEqual({ added: [], changed: [KEY], deleted: [] });
    expect(seedReport({ diff, applied: true })).toEqual([
      `  ~ ${KEY}`, "0 added; 1 changed; 0 deleted", "✓ catalogue and generation committed",
    ]);
    expect(seedReport({ diff: catalogueDiff([], old), applied: true })).toEqual([
      `  - ${KEY}`, "0 added; 0 changed; 1 deleted", "✓ catalogue and generation committed",
    ]);
  });

  test("unspecified cache-write lifetimes are retained without assigning a duration", () => {
    const rows = desiredRows(catalogueFixture([modelFixture({ prices: [priceFixture({ cache_write: "0.1875" })] })]));
    expect(rows[0]?.prices[0]?.cache_write_nanos_per_mtok).toBe("187500000");
    expect(rows[0]?.prices[0]?.cache_write_5m_nanos_per_mtok).toBeNull();
    expect(rows[0]?.prices[0]?.cache_write_1h_nanos_per_mtok).toBeNull();
    expect(desiredRows(catalogueFixture())[0]?.prices[0]?.cache_write_nanos_per_mtok).toBeNull();
  });

  test("malformed database rows are refused before comparison or writes", () => {
    expect(() => modelRowsSchema.parse([{ provider: "fireworks" }])).toThrow("model_name");
    expect(() => modelRowsSchema.parse([{ ...desiredRows(catalogueFixture())[0], prices: [{}] }])).toThrow("effective_from");
  });
});
