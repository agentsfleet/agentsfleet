import Decimal from "decimal.js";
import { z } from "zod";
import { isCuratedModel, isOfficialPriceSource } from "./curated";

export const NANOS_PER_USD = 1_000_000_000;
export const TOKENS_PER_MTOK = 1_000_000;
export const PRICING_UNIT = `USD per ${TOKENS_PER_MTOK} tokens`;
export const MAX_CATALOGUE_BYTES = 256 * 1024;
export const CATALOGUE_URL = new URL("../model-library-allowlist.json", import.meta.url);
const MONEY = z.string().max(32).regex(/^\d+(?:\.\d{1,9})?$/, { abort: true }).pipe(z.string().refine((value) =>
  new Decimal(value).gt(0) && new Decimal(value).mul(NANOS_PER_USD).lte("9223372036854775807")));
const INSTANT = z.iso.datetime({ offset: true });
const SOURCE = z.url().max(2048).pipe(z.string().refine((value) => new URL(value).protocol === "https:"));
const CONTEXT_LIMIT = 2_147_483_647;
const MODEL_LIMIT = 64;
const PRICE_LIMIT = 16;
const CONTEXT_REASON_LIMIT = 1024;
const CUSTOM_ISSUE = "custom";
const OPENAI_PROVIDER = "openai";
const OPENROUTER_PROVIDER = "openrouter";
const MIN_CURATED_INPUT_PRICE = "0.0001";
const PROVIDERS = ["anthropic", OPENAI_PROVIDER, "fireworks", OPENROUTER_PROVIDER, "groq", "mistral", "gemini"] as const;
const CONTEXT_APIS = {
  anthropic: "https://api.anthropic.com/v1/models/", openai: "https://api.openai.com/v1/models/",
  fireworks: "https://api.fireworks.ai/v1/", openrouter: "https://openrouter.ai/api/v1/models#",
  groq: "https://api.groq.com/openai/v1/models/", mistral: "https://api.mistral.ai/v1/models/",
  gemini: "https://generativelanguage.googleapis.com/v1beta/models/",
} as const;

export const priceSchema = z.strictObject({
  effective_from: INSTANT, effective_until: INSTANT.nullable(),
  prompt_tokens_above: z.int().min(0).max(CONTEXT_LIMIT),
  input: MONEY, cached_input: MONEY, output: MONEY,
  cache_write: MONEY.nullable(), cache_write_5m: MONEY.nullable(), cache_write_1h: MONEY.nullable(),
  source_url: SOURCE, verified_at: INSTANT, conditions: z.string().min(1).max(2048),
}).superRefine((price, context) => {
  if (new Decimal(price.cached_input).gt(price.input))
    context.addIssue({ code: CUSTOM_ISSUE, message: "cached_input exceeds input" });
});

const modelSchema = z.strictObject({
  provider: z.enum(PROVIDERS), model_name: z.string().min(1).max(256),
  display_name: z.string().min(1).max(128), advanced: z.boolean(),
  context: z.strictObject({ tokens: z.int().positive().max(CONTEXT_LIMIT),
    source_url: SOURCE, verified_at: INSTANT }).nullable(),
  context_unverified_reason: z.string().min(1).max(CONTEXT_REASON_LIMIT).nullable(),
  prices: z.array(priceSchema).min(1).max(PRICE_LIMIT),
}).superRefine((model, context) => {
  if (!isCuratedModel(model))
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: not a curated identity` });
  if (!model.context && !model.context_unverified_reason)
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: missing context evidence or refusal reason` });
  if (model.context && model.context_unverified_reason)
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: context and refusal reason disagree` });
  if (model.context && !providerContextSource(model, model.context.source_url))
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: context source must name its provider models API` });
  if (model.advanced !== (model.provider === OPENAI_PROVIDER && model.model_name === "gpt-5.6-terra"))
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: only gpt-5.6-terra is advanced` });
  if (model.prices.every((price) => new Decimal(price.input).lt(MIN_CURATED_INPUT_PRICE)))
    context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: sentinel-sized input prices cannot enter the curated catalogue` });
  const tiers = new Map<number, Price[]>();
  for (const price of model.prices) {
    if (!isOfficialPriceSource(model.provider, price.source_url))
      context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: price source must be the provider's official page` });
    const from = Date.parse(price.effective_from);
    const until = price.effective_until === null ? Infinity : Date.parse(price.effective_until);
    if (until <= from) context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: reversed price dates` });
    const previous = tiers.get(price.prompt_tokens_above) ?? [];
    if (previous.some((other) => from < (other.effective_until === null ? Infinity : Date.parse(other.effective_until))
      && Date.parse(other.effective_from) < until))
      context.addIssue({ code: CUSTOM_ISSUE, message: `${model.model_name}: overlapping price dates` });
    tiers.set(price.prompt_tokens_above, [...previous, price]);
  }
});

const documentSchema = z.strictObject({
  verified_at: INSTANT, pricing_unit: z.literal(PRICING_UNIT),
  models: z.array(modelSchema).min(1).max(MODEL_LIMIT),
  ollama: z.strictObject({ display_name: z.literal("Ollama"), pricing: z.literal("unknown"),
    models_source: z.literal("configured server /api/tags"), endpoint_kinds: z.tuple([z.literal("local"), z.literal("cloud")]) }),
}).superRefine((document, context) => {
  const seen = new Set<string>();
  for (const model of document.models) {
    const key = `${model.provider}/${model.model_name}`;
    if (seen.has(key)) context.addIssue({ code: CUSTOM_ISSUE, message: `${key}: duplicate model identity` });
    seen.add(key);
  }
});

export type Price = z.infer<typeof priceSchema>;
export type Model = z.infer<typeof modelSchema>;
export type Catalogue = z.infer<typeof documentSchema>;

function providerContextSource(model: Pick<Model, "provider" | "model_name">, sourceUrl: string): boolean {
  const source = new URL(sourceUrl);
  if (model.provider !== OPENROUTER_PROVIDER) source.hash = "";
  return source.href === `${CONTEXT_APIS[model.provider]}${model.model_name}`;
}

export function parseCatalogue(raw: unknown): Catalogue {
  const result = documentSchema.safeParse(raw);
  if (result.success) return result.data;
  const labels = z.object({ models: z.array(z.object({ model_name: z.string().max(256).optional() })).max(MODEL_LIMIT) }).safeParse(raw);
  throw new z.ZodError(result.error.issues.map((issue) => {
    const index = issue.path[1];
    const name = labels.success && typeof index === "number" ? labels.data.models[index]?.model_name : undefined;
    return { ...issue, message: `${name ?? "catalogue"}: ${issue.message}` };
  }));
}

/** Decimal arithmetic preserves all nine fractional USD digits before BIGINT storage. */
export function nanosPerMtok(value: string): string {
  return new Decimal(MONEY.parse(value)).mul(NANOS_PER_USD).toFixed(0);
}

export function priceForRequest(model: Model, promptTokens: number, at: string): Price | null {
  z.int().min(0).max(CONTEXT_LIMIT).parse(promptTokens);
  const instant = Date.parse(INSTANT.parse(at));
  return model.prices.filter((price) => Date.parse(price.effective_from) <= instant
    && (price.effective_until === null || instant < Date.parse(price.effective_until))
    && (price.prompt_tokens_above === 0 || promptTokens > price.prompt_tokens_above))
    .sort((left, right) => right.prompt_tokens_above - left.prompt_tokens_above)[0] ?? null;
}

export async function readCatalogue(path: string | URL): Promise<Catalogue> {
  const file = Bun.file(path);
  if (file.size > MAX_CATALOGUE_BYTES) throw new RangeError("Model catalogue exceeds its byte limit");
  return parseCatalogue(await file.json());
}

export function catalogueReport(catalogue: Catalogue): string[] {
  const verified = catalogue.models.filter((model) => model.context !== null);
  const warnings = catalogue.models.filter((model) => model.context === null)
    .map((model) => `NOT SEEDED ${model.provider}/${model.model_name}: ${model.context_unverified_reason}`);
  return [...warnings, `✓ [models] ${catalogue.models.length} curated identities; ${verified.length} verified contexts; Ollama pricing unknown`];
}
