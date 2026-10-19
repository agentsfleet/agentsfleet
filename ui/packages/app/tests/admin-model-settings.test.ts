import { describe, expect, it } from "vitest";
import { ADMIN_MODEL, METADATA, PRICE } from "./helpers/catalogue-fixtures";
import { INFERENCE_API } from "@/lib/api/admin-model-library-types";
import { SETTINGS_DEFAULTS, modelSettings, settingsSchema, valuesFromModel } from "@/app/(dashboard)/admin/models/components/model-settings";

const VALUES = valuesFromModel(ADMIN_MODEL);

describe("complete catalogue settings", () => {
  it("round trips metadata, every dated price and inference settings exactly", () => {
    const model = { ...ADMIN_MODEL, metadata: { ...METADATA, advanced: true }, prices: [
      { ...PRICE, cache_write_nanos_per_mtok: "1", cache_write_5m_nanos_per_mtok: "9223372036854775807", cache_write_1h_nanos_per_mtok: "187500000" },
      { ...PRICE, prompt_tokens_above: 272000 },
    ] };
    expect(modelSettings(settingsSchema.parse(valuesFromModel(model)))).toEqual({ metadata: model.metadata, prices: model.prices, inference: model.inference });
  });
  it("keeps missing cache-write prices null and distinguishes a confirmed zero", () => {
    const prices = [{ ...PRICE, cache_write_nanos_per_mtok: "0" }];
    expect(modelSettings(settingsSchema.parse(valuesFromModel({ ...ADMIN_MODEL, prices }))).prices).toEqual(prices);
  });
  it("omits inference to preserve existing settings and allows an unconfigured new row", () => {
    const values = valuesFromModel({ ...ADMIN_MODEL, inference: undefined });
    expect(values.update_inference).toBe(false);
    expect(modelSettings(settingsSchema.parse(values)).inference).toBeUndefined();
    expect(modelSettings(settingsSchema.parse({ ...VALUES, update_inference: false, base_url: "" })).inference).toBeUndefined();
    expect(settingsSchema.safeParse(SETTINGS_DEFAULTS).success).toBe(false);
  });
  it("accepts the five explicit inference APIs without inventing a dialect", () => {
    for (const api of Object.values(INFERENCE_API)) {
      expect(modelSettings(settingsSchema.parse({ ...VALUES, api, dialect: " " })).inference?.api).toBe(api);
    }
  });
  it("retains a configured compatible dialect", () => {
    expect(modelSettings(settingsSchema.parse({ ...VALUES, dialect: "openai" })).inference?.dialect).toBe("openai");
  });
  it.each(["", "not a URL", "http://models.example.com", "https://user:secret@models.example.com", "https://models.example.com?key=secret", "https://models.example.com#secret"])("refuses unsafe inference endpoint %s", (base_url) => {
    expect(settingsSchema.safeParse({ ...VALUES, base_url }).success).toBe(false);
  });
  it.each(["", "0", "-1", "1.5", "NaN", "Infinity", "9007199254740992"])("refuses invalid positive token count %s", (value) => {
    for (const field of ["context_cap_tokens", "max_output_tokens"]) expect(settingsSchema.safeParse({ ...VALUES, [field]: value }).success).toBe(false);
  });
  it.each(["", "-1", "NaN", "Infinity", "0.0000000001"])("refuses invalid price %s with a validation result", (value) => {
    for (const field of ["input_usd", "cached_usd", "output_usd"]) {
      expect(settingsSchema.safeParse({ ...VALUES, prices: [{ ...VALUES.prices[0], [field]: value }] }).success).toBe(false);
    }
  });
  it("refuses excess output, long dialects, unsupported APIs and native dialects", () => {
    for (const delta of [{ max_output_tokens: "128001" }, { dialect: "x".repeat(65) }, { api: "unknown" }, { api: INFERENCE_API.gemini, dialect: "openai" }]) {
      expect(settingsSchema.safeParse({ ...VALUES, ...delta }).success).toBe(false);
    }
    expect(settingsSchema.safeParse({ ...VALUES, max_output_tokens: "128000" }).success).toBe(true);
  });
  it.each([
    { prompt_tokens_above: "1" }, { effective_until: "2026-10-17T00:00:00Z" },
  ])("reports price schedule refusals beside the offending data", (delta) => {
    const result = settingsSchema.safeParse({ ...VALUES, prices: [{ ...VALUES.prices[0], ...delta }] });
    expect(result.success).toBe(false);
    if (!result.success) expect(result.error.issues[0]?.path[0]).toBe("prices");
  });
  it("reports overlapping dates but accepts an adjacent half-open range", () => {
    expect(settingsSchema.safeParse({ ...VALUES, prices: [VALUES.prices[0], VALUES.prices[0]] }).success).toBe(false);
    const first = { ...VALUES.prices[0], effective_until: "2026-12-01T00:00:00Z" };
    const next = { ...VALUES.prices[0], effective_from: first.effective_until };
    expect(settingsSchema.safeParse({ ...VALUES, prices: [first, next] }).success).toBe(true);
  });
});
