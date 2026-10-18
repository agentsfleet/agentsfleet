import type { AdminModel, ModelRatesInput } from "@/lib/api/admin_model_library";
import type { CataloguePrice } from "@/lib/api/catalogue-types";
import { z } from "zod";
import { INFERENCE_API, nanosToUsdPerMtok, usdPerMtokToNanos } from "@/lib/api/admin-model-library-types";
import { boundedText, CATALOGUE_LIMITS, instantSchema, priceScheduleIssues, SCHEDULE_ERRORS } from "@/lib/api/catalogue-types";

const INT32_MAX = 2_147_483_647;
const UINT32_MAX = 4_294_967_295;
const POSITIVE_INTEGER_ERROR = "Enter a positive integer within the supported token limit";
const rate = z.string().trim().superRefine((value, context) => {
  try { usdPerMtokToNanos(value); }
  catch (error) { context.addIssue({ code: "custom", message: error instanceof Error ? error.message : "Invalid price" }); }
});
const optionalRate = z.union([z.literal(""), rate]);
const tokenCount = (minimum: number, maximum: number) => z.string().trim().regex(/^\d+$/, POSITIVE_INTEGER_ERROR)
  .refine((value) => Number(value) >= minimum && Number(value) <= maximum, POSITIVE_INTEGER_ERROR);
const requiredEvidence = boundedText(CATALOGUE_LIMITS.evidenceUrl).pipe(z.url({ protocol: /^https?$/ }));
const optionalInstant = z.union([z.literal(""), instantSchema]);
export const priceFormSchema = z.object({
  effective_from: instantSchema, effective_until: optionalInstant,
  prompt_tokens_above: tokenCount(0, INT32_MAX),
  input_usd: rate, cached_usd: rate, output_usd: rate,
  write_usd: optionalRate, write_5m_usd: optionalRate, write_1h_usd: optionalRate,
  source_url: requiredEvidence, verified_at: instantSchema,
  conditions: boundedText(CATALOGUE_LIMITS.conditions),
});
export type PriceFormValues = z.infer<typeof priceFormSchema>;
export const EMPTY_PRICE: PriceFormValues = {
  effective_from: "", effective_until: "", prompt_tokens_above: "0",
  input_usd: "", cached_usd: "", output_usd: "", write_usd: "", write_5m_usd: "", write_1h_usd: "",
  source_url: "", verified_at: "", conditions: "",
};
export const settingsSchema = z.object({
  display_name: boundedText(CATALOGUE_LIMITS.displayName).pipe(z.string().trim().min(1, "Name the model for the catalogue")),
  context_cap_tokens: tokenCount(1, INT32_MAX), context_source_url: requiredEvidence,
  context_verified_at: instantSchema, advanced: z.boolean(),
  prices: z.array(priceFormSchema).min(1, SCHEDULE_ERRORS.count).max(CATALOGUE_LIMITS.prices, SCHEDULE_ERRORS.count),
  update_inference: z.boolean(), api: z.enum(INFERENCE_API), base_url: z.string().trim(),
  dialect: boundedText(CATALOGUE_LIMITS.dialect).pipe(z.string().trim()), max_output_tokens: z.string().trim(),
}).superRefine((values, context) => {
  if (values.prices.some((price) => !priceFormSchema.safeParse(price).success)) return;
  for (const issue of priceScheduleIssues(values.prices.map(priceInput))) {
    context.addIssue({ code: "custom", message: issue.message,
      path: issue.index === null ? ["prices", "root"] : ["prices", issue.index, issue.field] });
  }
  if (values.update_inference) validateInference(values, context);
});
export type ModelSettingsValues = z.infer<typeof settingsSchema>;
export const SETTINGS_DEFAULTS: ModelSettingsValues = {
  display_name: "", context_cap_tokens: "", context_source_url: "", context_verified_at: "", advanced: false,
  prices: [{ ...EMPTY_PRICE }], update_inference: false,
  api: INFERENCE_API.chat, base_url: "", dialect: "", max_output_tokens: "",
};

function validateInference(values: ModelSettingsValues, context: z.RefinementCtx) {
  const url = z.url().safeParse(values.base_url);
  if (!url.success || !safeEndpoint(url.data)) {
    context.addIssue({ code: "custom", path: ["base_url"], message: "Use an HTTPS base URL without credentials, query or fragment" });
  }
  if (!tokenCount(1, UINT32_MAX).safeParse(values.max_output_tokens).success
    || Number(values.max_output_tokens) > Number(values.context_cap_tokens)) {
    context.addIssue({ code: "custom", path: ["max_output_tokens"], message: "Output limit must be positive and fit in the model context window" });
  }
  if (values.dialect && (values.api === INFERENCE_API.gemini || values.api === INFERENCE_API.ollama)) {
    context.addIssue({ code: "custom", path: ["dialect"], message: "Native Gemini and Ollama APIs do not take a dialect" });
  }
}

function safeEndpoint(value: string): boolean {
  const url = new URL(value);
  return value.length <= CATALOGUE_LIMITS.evidenceUrl && url.protocol === "https:" && !url.username && !url.password && !url.search && !url.hash;
}

function priceInput(value: PriceFormValues): CataloguePrice {
  return {
    effective_from: value.effective_from, effective_until: value.effective_until || null,
    prompt_tokens_above: Number(value.prompt_tokens_above),
    input_nanos_per_mtok: usdPerMtokToNanos(value.input_usd),
    cached_input_nanos_per_mtok: usdPerMtokToNanos(value.cached_usd),
    output_nanos_per_mtok: usdPerMtokToNanos(value.output_usd),
    cache_write_nanos_per_mtok: value.write_usd === "" ? null : usdPerMtokToNanos(value.write_usd),
    cache_write_5m_nanos_per_mtok: value.write_5m_usd === "" ? null : usdPerMtokToNanos(value.write_5m_usd),
    cache_write_1h_nanos_per_mtok: value.write_1h_usd === "" ? null : usdPerMtokToNanos(value.write_1h_usd),
    source_url: value.source_url, verified_at: value.verified_at, conditions: value.conditions,
  };
}

function priceValues(price: CataloguePrice): PriceFormValues {
  return {
    effective_from: price.effective_from, effective_until: price.effective_until ?? "",
    prompt_tokens_above: String(price.prompt_tokens_above),
    input_usd: nanosToUsdPerMtok(price.input_nanos_per_mtok), cached_usd: nanosToUsdPerMtok(price.cached_input_nanos_per_mtok),
    output_usd: nanosToUsdPerMtok(price.output_nanos_per_mtok),
    write_usd: price.cache_write_nanos_per_mtok === null ? "" : nanosToUsdPerMtok(price.cache_write_nanos_per_mtok),
    write_5m_usd: price.cache_write_5m_nanos_per_mtok === null ? "" : nanosToUsdPerMtok(price.cache_write_5m_nanos_per_mtok),
    write_1h_usd: price.cache_write_1h_nanos_per_mtok === null ? "" : nanosToUsdPerMtok(price.cache_write_1h_nanos_per_mtok),
    source_url: price.source_url, verified_at: price.verified_at, conditions: price.conditions,
  };
}

export function valuesFromModel(model: AdminModel): ModelSettingsValues {
  return {
    ...model.metadata, context_cap_tokens: String(model.metadata.context_cap_tokens), prices: model.prices.map(priceValues),
    update_inference: model.inference !== undefined, api: model.inference?.api ?? INFERENCE_API.chat,
    base_url: model.inference?.base_url ?? "", dialect: model.inference?.dialect ?? "",
    max_output_tokens: model.inference ? String(model.inference.max_output_tokens) : "",
  };
}

export function modelSettings(values: ModelSettingsValues): ModelRatesInput {
  return {
    metadata: { display_name: values.display_name, context_cap_tokens: Number(values.context_cap_tokens),
      context_source_url: values.context_source_url, context_verified_at: values.context_verified_at, advanced: values.advanced },
    prices: values.prices.map(priceInput),
    ...(values.update_inference ? { inference: { api: values.api, base_url: values.base_url,
      max_output_tokens: Number(values.max_output_tokens), ...(values.dialect ? { dialect: values.dialect } : {}) } } : {}),
  };
}
