import { afterEach, describe, expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { z } from "zod";
import { catalogueReport, MAX_CATALOGUE_BYTES, nanosPerMtok, parseCatalogue, priceForRequest, readCatalogue } from "./catalogue";
import type { Catalogue, Model, Price } from "./catalogue";
import { HAIKU_PROMPT_THRESHOLD } from "./test-fixtures";

const VERIFIED = "2026-10-18T12:00:00Z";
const CUTOVER = "2026-12-01T00:00:00Z";
const BEFORE = "2026-11-30T23:59:59.999Z";
const SOURCE = "https://developers.openai.com/api/docs/pricing";
const MODEL = "gpt-6.1-sol";
const PRICING_UNIT = "USD per 1000000 tokens";
const FILES: string[] = [];

function price(overrides: Partial<Price> = {}): Price {
  return { effective_from: VERIFIED, effective_until: null, prompt_tokens_above: 0,
    input: "2", cached_input: "0.1", output: "10", cache_write: "2.5",
    cache_write_5m: null, cache_write_1h: null, source_url: SOURCE,
    verified_at: VERIFIED, conditions: "Standard synchronous inference.", ...overrides };
}

function model(overrides: Partial<Model> = {}): Model {
  return { provider: "openai", model_name: MODEL, display_name: "GPT 6.1 Sol", advanced: false,
    inference: { api: "responses", base_url: "https://api.openai.com/v1", dialect: "openai", max_output_tokens: 128000 },
    context: null, context_unverified_reason: "Models API requires authentication.",
    prices: [price()], ...overrides };
}

function document(models: Model[] = [model()]): Catalogue {
  return { verified_at: VERIFIED, pricing_unit: PRICING_UNIT, models,
    ollama: { display_name: "Ollama", pricing: "unknown", models_source: "configured server /api/tags", endpoint_kinds: ["local", "cloud"] } };
}

async function fixture(contents: string): Promise<string> {
  const path = `${tmpdir()}/agentsfleet-catalogue-${crypto.randomUUID()}.json`;
  FILES.push(path);
  await Bun.write(path, contents);
  return path;
}

afterEach(async () => {
  for (const path of FILES.splice(0)) await Bun.file(path).delete();
});

describe("catalogue provenance", () => {
  test("unverified exact contexts are reported without inventing a limit", () => {
    const result = parseCatalogue(document());
    expect(result.models[0]?.context).toBeNull();
    expect(catalogueReport(result)).toEqual([
      `NOT SEEDED openai/${MODEL}: Models API requires authentication.`,
      "✓ [models] 1 curated identities; 0 verified contexts; Ollama pricing unknown",
    ]);
  });

  test("context evidence names the exact model in its provider models API", () => {
    const result = parseCatalogue(document([model({ context: { tokens: 1_048_576,
      source_url: `https://api.openai.com/v1/models/${MODEL}#context_length`, verified_at: VERIFIED },
      context_unverified_reason: null })]));
    expect(result.models[0]?.context?.tokens).toBe(1_048_576);
    expect(catalogueReport(result)).toEqual(["✓ [models] 1 curated identities; 1 verified contexts; Ollama pricing unknown"]);
  });

  test("OpenRouter evidence uses the served model's own API entry", () => {
    const context = { tokens: 1_048_576, source_url: "https://openrouter.ai/api/v1/models#google/gemini-4-argon", verified_at: VERIFIED };
    const routed = model({ provider: "openrouter", model_name: "google/gemini-4-argon",
      context, prices: [price({ source_url: "https://openrouter.ai/google/gemini-4-argon" })],
      context_unverified_reason: null });
    expect(parseCatalogue(document([routed])).models[0]?.context?.tokens).toBe(1_048_576);
    context.source_url = "https://openrouter.ai/api/v1/models#google/other-model";
    expect(() => parseCatalogue(document([routed]))).toThrow("context source must name its provider models API");
  });

  test("a keyed provider's limit may come from OpenRouter's endpoint entry tagged with that provider's standard endpoint", () => {
    const served = (source_url: string, overrides: Partial<Model> = {}) => document([model({
      context: { tokens: 1_050_000, source_url, verified_at: VERIFIED }, context_unverified_reason: null, ...overrides })]);
    const endpoints = `https://openrouter.ai/api/v1/models/openai/${MODEL}/endpoints`;
    expect(parseCatalogue(served(`${endpoints}#openai`)).models[0]?.context?.tokens).toBe(1_050_000);
    const gemini = { provider: "gemini" as const, model_name: "gemini-4-argon", display_name: "Gemini 4 Argon",
      prices: [price({ source_url: "https://ai.google.dev/gemini-api/docs/pricing" })] };
    expect(parseCatalogue(served("https://openrouter.ai/api/v1/models/google/gemini-4-argon/endpoints#google-ai-studio",
      gemini)).models[0]?.context?.tokens).toBe(1_050_000);
    for (const refused of [`${endpoints}#azure`, `${endpoints}#openai/flex`, `${endpoints}#OpenAI`, `${endpoints}`, `${endpoints}?region=eu#openai`,
      `https://openrouter.ai/api/v1/models/${MODEL}/endpoints#openai`, `https://example.com/api/v1/models/openai/${MODEL}/endpoints#openai`])
      expect(() => parseCatalogue(served(refused))).toThrow("context source must name its provider models API");
    expect(() => parseCatalogue(served("https://openrouter.ai/api/v1/models/google/gemini-4-argon/endpoints#google-ai-studio/flex",
      gemini))).toThrow("context source must name its provider models API");
    const routed = { provider: "openrouter" as const, model_name: "google/gemini-4-argon",
      prices: [price({ source_url: "https://openrouter.ai/google/gemini-4-argon" })] };
    expect(() => parseCatalogue(served("https://openrouter.ai/api/v1/models/google/gemini-4-argon/endpoints#openrouter",
      routed))).toThrow("context source must name its provider models API");
  });

  test.each([
    model({ context_unverified_reason: null }),
    model({ context: { tokens: 1_048_576, source_url: `https://api.openai.com/v1/models/${MODEL}`, verified_at: VERIFIED } }),
    model({ context: { tokens: 1_048_576, source_url: "https://example.com/marketing-1M", verified_at: VERIFIED }, context_unverified_reason: null }),
    model({ advanced: true }),
    model({ model_name: "gpt-5.6-terra", advanced: false }),
  ])("refuses contradictory or unsupported identity evidence %#", (entry) => {
    expect(() => parseCatalogue(document([entry]))).toThrow(entry.model_name);
  });

  test("Terra is the advanced entry", () => {
    expect(parseCatalogue(document([model({ model_name: "gpt-5.6-terra", advanced: true })])).models[0]?.advanced).toBe(true);
  });

  test("duplicate provider and model identities are refused", () => {
    expect(() => parseCatalogue(document([model(), model()]))).toThrow(`openai/${MODEL}: duplicate model identity`);
  });

  test.each(["source_url", "verified_at", "conditions", "effective_from", "effective_until"]) (
    "missing price %s is refused by model name", (field) => {
      const rawPrice: Record<string, unknown> = { ...price() };
      delete rawPrice[field];
      const raw = { ...document(), models: [{ ...model(), prices: [rawPrice] }] };
      expect(() => parseCatalogue(raw)).toThrow(MODEL);
    });

  test("the unit is mandatory, and extra availability flags are refused", () => {
    expect(() => parseCatalogue({ ...document(), pricing_unit: "USD per token" })).toThrow("catalogue:");
    expect(() => parseCatalogue({ ...document(), models: [{ ...model(), enabled: true }] })).toThrow("Unrecognized key");
    expect(() => parseCatalogue(null)).toThrow("catalogue:");
    expect(() => parseCatalogue({ models: [{}] })).toThrow("catalogue:");
  });

  test("prices need HTTPS evidence, positive amounts and bounded exact decimals", () => {
    for (const source_url of ["http://example.com/rates", "not a URL"])
      expect(() => parseCatalogue(document([model({ prices: [price({ source_url })] })]))).toThrow(MODEL);
    for (const field of ["source_url", "verified_at"]) {
      const rawPrice: Record<string, unknown> = { ...price() };
      delete rawPrice[field];
      const refused = (() => {
        try {
          parseCatalogue({ ...document(), models: [{ ...model(), prices: [rawPrice] }] });
        } catch (error) {
          return error;
        }
        return undefined;
      })();
      expect(refused).toBeInstanceOf(z.ZodError);
      const issues = (refused as z.ZodError).issues;
      expect(issues.map((issue) => issue.path)).toEqual([["models", 0, "prices", 0, field]]);
      expect(issues[0]?.message.startsWith(`${MODEL}: `)).toBe(true);
    }
    for (const amount of ["-1", "NaN", "1.0000000001", "9223372037", "1".repeat(33)])
      expect(() => nanosPerMtok(amount)).toThrow();
  });
});

describe("request pricing", () => {
  test.each([272_000, HAIKU_PROMPT_THRESHOLD])("the whole request switches above %i prompt tokens", (boundary) => {
    const subject = model({ prices: [price(), price({ prompt_tokens_above: boundary, input: "4", cached_input: "0.2", output: "15" })] });
    expect(priceForRequest(subject, boundary, VERIFIED)?.input).toBe("2");
    expect(priceForRequest(subject, boundary + 1, VERIFIED)?.input).toBe("4");
    expect(priceForRequest(subject, boundary + 1, VERIFIED)?.output).toBe("15");
  });

  test("effective dates switch at UTC midnight and respect offsets", () => {
    const subject = model({ prices: [price({ effective_until: CUTOVER }), price({ effective_from: CUTOVER, input: "4" })] });
    expect(priceForRequest(subject, 1, BEFORE)?.input).toBe("2");
    expect(priceForRequest(subject, 1, CUTOVER)?.input).toBe("4");
    expect(priceForRequest(subject, 1, "2026-12-01T05:30:00+05:30")?.input).toBe("4");
    expect(priceForRequest(subject, 1, "2026-10-17T00:00:00Z")).toBeNull();
    expect(priceForRequest(model({ prices: [price({ effective_until: CUTOVER })] }), 1, CUTOVER)).toBeNull();
    expect(parseCatalogue(document([subject])).models[0]?.prices).toHaveLength(2);
  });

  test("invalid instants and prompt sizes cannot select a price", () => {
    expect(() => priceForRequest(model(), 1, "yesterday")).toThrow();
    for (const tokens of [-1, 1.5, NaN, 2_147_483_648])
      expect(() => priceForRequest(model(), tokens, VERIFIED)).toThrow();
  });

  test.each([
    { prices: [price({ effective_until: VERIFIED })] },
    { prices: [price(), price()] },
    { prices: [price({ effective_until: CUTOVER }), price({ effective_from: BEFORE })] },
  ])("overlapping or reversed prices are refused %#", ({ prices }) => {
    expect(() => parseCatalogue(document([model({ prices: [...prices] })]))).toThrow(MODEL);
  });

  test("separate prompt tiers can share the same effective interval", () => {
    expect(parseCatalogue(document([model({ prices: [price(), price({ prompt_tokens_above: 272_000 })] })])).models[0]?.prices).toHaveLength(2);
  });

  test.each([
    ["0", "0"],
    ["0.006", "6000000"], ["0.015", "15000000"], ["0.0375", "37500000"],
    ["0.0625", "62500000"], ["0.075", "75000000"], ["0.125", "125000000"],
    ["0.1875", "187500000"], ["0.000000001", "1"], ["9223372036.854775807", "9223372036854775807"],
  ])("%s USD per million tokens stores as exactly %s nanos", (amount, nanos) => {
    expect(nanosPerMtok(amount)).toBe(nanos);
  });
});

describe("bounded catalogue reads", () => {
  test("reads a validated file without coercing decimal strings", async () => {
    const path = await fixture(JSON.stringify(document()));
    expect((await readCatalogue(new URL(`file://${path}`))).models[0]?.prices[0]?.input).toBe("2");
  });

  test("refuses an oversized file before parsing and propagates invalid JSON", async () => {
    await expect(readCatalogue(await fixture(" ".repeat(MAX_CATALOGUE_BYTES + 1)))).rejects.toThrow("byte limit");
    await expect(readCatalogue(await fixture("{"))).rejects.toThrow();
  });
});


test("inference settings require a safe endpoint and a completion limit inside the proved context", () => {
  const entry = model({ context: { tokens: 128000, source_url: `https://api.openai.com/v1/models/${MODEL}`, verified_at: VERIFIED }, context_unverified_reason: null });
  expect(parseCatalogue(document([entry])).models[0]?.inference.max_output_tokens).toBe(128000);
  for (const inference of [undefined, { ...entry.inference, base_url: "http://provider.example.com" },
    { ...entry.inference, base_url: "https://user:secret@provider.example.com" },
    { ...entry.inference, base_url: "https://:secret@provider.example.com" },
    { ...entry.inference, base_url: "https://provider.example.com?key=secret" },
    { ...entry.inference, base_url: "https://provider.example.com#fragment" },
    { ...entry.inference, api: "unknown" }, { ...entry.inference, max_output_tokens: 0 },
    { ...entry.inference, max_output_tokens: 128001 }]) {
    expect(() => parseCatalogue(document([{ ...entry, inference } as Model]))).toThrow();
  }
});
