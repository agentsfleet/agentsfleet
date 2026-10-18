import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { assertCompleteCatalogue, CURATED_IDENTITIES, isCuratedModel, isOfficialPriceSource } from "./curated";
import { parseCatalogue } from "./catalogue";
import { catalogueFixture, modelFixture, priceFixture } from "./test-fixtures";

describe("curated catalogue bounds", () => {
  test("exactly 18 identities include native and routed Gemini independently", () => {
    expect(CURATED_IDENTITIES).toHaveLength(18);
    expect(isCuratedModel(modelFixture({ provider: "gemini", model_name: "gemini-4-argon" }))).toBe(true);
    expect(isCuratedModel(modelFixture({ provider: "openrouter", model_name: "google/gemini-4-argon" }))).toBe(true);
    expect(isCuratedModel(modelFixture({ model_name: "accounts/fireworks/models/deepseek-v4-pro-0813" }))).toBe(false);
    expect(() => parseCatalogue(catalogueFixture([modelFixture({ model_name: "unrequested-model" })]))).toThrow("not a curated identity");
  });

  test("a missing, extra or repeated identity refuses catalogue replacement", () => {
    const wanted = CURATED_IDENTITIES.map((identity) => ({ provider: identity.split("/")[0], model_name: identity.slice(identity.indexOf("/") + 1) }));
    expect(() => assertCompleteCatalogue(wanted)).not.toThrow();
    expect(() => assertCompleteCatalogue(wanted.slice(1))).toThrow("missing: anthropic/claude-fable-5-1");
    expect(() => assertCompleteCatalogue([...wanted, modelFixture()])).toThrow("exactly the curated identities");
    expect(() => assertCompleteCatalogue([...wanted, { provider: "openai", model_name: "unrequested-model" }])).toThrow("exactly the curated identities");
    expect(() => assertCompleteCatalogue(wanted.map((model) => ({ ...model, provider: "fireworks" })))).toThrow("exactly the curated identities");
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
