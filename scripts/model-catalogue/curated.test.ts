import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { catalogueModelNames, isCuratedModel, isOfficialPriceSource } from "./curated";
import { CATALOGUE_URL, parseCatalogue, readCatalogue } from "./catalogue";
import { catalogueFixture, modelFixture, priceFixture } from "./test-fixtures";

describe("curated catalogue bounds", () => {
  test("Fireworks MiniMax uses its native route and standard serverless prices", async () => {
    const catalogue = await readCatalogue(CATALOGUE_URL);
    // pin test: literal is the contract
    expect(catalogue.models.find(model => model.provider === "fireworks" && model.model_name === "accounts/fireworks/models/minimax-m3p1-flash"))
      // pin test: literal is the contract
      .toMatchObject({context: {tokens: 1048576}, inference: {api: "chat", base_url: "https://api.fireworks.ai/inference/v1"},
        // pin test: literal is the contract
        prices: [{input: "0.15", cached_input: "0.015", output: "0.60"}]});
  });

  test.each(["input", "cached_input", "output", "cache_write", "cache_write_5m", "cache_write_1h"])(
    "models refuse decimal zero in %s", (field) => {
      const subject = modelFixture({ prices: [priceFixture({ [field]: "0.0" })] });
      expect(() => parseCatalogue(catalogueFixture([subject]))).toThrow("zero prices are not allowed in the curated catalogue");
    });

  test("the allowlist includes native and routed Gemini independently", () => {
    expect(isCuratedModel(modelFixture({ provider: "gemini", model_name: "gemini-4-argon" }))).toBe(true);
    expect(isCuratedModel(modelFixture({ provider: "openrouter", model_name: "google/gemini-4-argon" }))).toBe(true);
    expect(isCuratedModel(modelFixture({ model_name: "accounts/fireworks/models/deepseek-v4-pro-0813" }))).toBe(false);
    expect(() => parseCatalogue(catalogueFixture([modelFixture({ model_name: "unrequested-model" })]))).toThrow("not a curated identity");
  });

  test("membership and safe diagnostic names come from the allowlist", async () => {
    const catalogue = await readCatalogue(CATALOGUE_URL);
    expect(catalogue.models).toHaveLength(20);
    expect(catalogue.models.every(isCuratedModel)).toBe(true);
    expect([...catalogueModelNames]).toEqual([...new Set(catalogue.models.map((model) => model.model_name))]);
    expect(() => parseCatalogue(catalogueFixture([modelFixture(), modelFixture()])))
      .toThrow("duplicate model identity");
  });

  test("a quoted price belongs to the provider's official host", () => {
    expect(isOfficialPriceSource("fireworks", priceFixture().source_url)).toBe(true);
    expect(() => parseCatalogue(catalogueFixture([modelFixture({ prices: [priceFixture({ source_url: "https://api.moonshot.cn/pricing" })] })])))
      .toThrow("provider's official page");
    expect(() => parseCatalogue(catalogueFixture([modelFixture({ prices: [priceFixture({ cached_input: "0.16" })] })])))
      .toThrow("cached_input exceeds input");
    expect(() => parseCatalogue(catalogueFixture([modelFixture({ prices: [priceFixture({ input: "0.000000001", cached_input: "0.000000001" })] })])))
      .toThrow("sentinel-sized input prices");
  });

  test.each(["nonsense", "1x", "NaN", "-1", "", 0, null, {}])("malformed money stays a schema error: %j", (input) => {
    const raw = catalogueFixture();
    const model = modelFixture();
    const price = { ...priceFixture(), input };
    expect(() => parseCatalogue({ ...raw, models: [{ ...model, prices: [price] }] })).toThrow(z.ZodError);
  });
});
