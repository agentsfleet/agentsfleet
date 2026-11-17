import { z } from "zod";
import { INFERENCE_API } from "./admin-model-library-types";
import { nanosSchema } from "./nanos";
export { nanosSchema } from "./nanos";

const CONDITIONS_MAX_BYTES = 1024;
export const CATALOGUE_LIMITS = {
  prices: 64, displayName: 128, evidenceUrl: 2048, conditions: CONDITIONS_MAX_BYTES,
  dialect: 64,
} as const;
const FRACTION_DIGITS = 9;
const encoder = new TextEncoder();

export function boundedText(max: number) {
  return z.string().refine((value) => encoder.encode(value).length <= max, `Use at most ${max} bytes`);
}

export const instantSchema = z.iso.datetime({ offset: true }).refine(
  (value) => (value.match(/\.(\d+)/)?.[1]?.length ?? 0) <= FRACTION_DIGITS,
  "Use at most nine fractional second digits",
);
export const metadataSchema = z.object({
  display_name: boundedText(CATALOGUE_LIMITS.displayName).pipe(z.string().min(1)),
  context_cap_tokens: z.int32().positive(),
  context_source_url: boundedText(CATALOGUE_LIMITS.evidenceUrl).pipe(z.string().min(1)),
  context_verified_at: instantSchema,
  advanced: z.boolean(),
});
export const priceSchema = z.object({
  effective_from: instantSchema, effective_until: instantSchema.nullable().default(null),
  prompt_tokens_above: z.int32().nonnegative(),
  input_nanos_per_mtok: nanosSchema, cached_input_nanos_per_mtok: nanosSchema,
  cache_write_nanos_per_mtok: nanosSchema.nullable().default(null),
  cache_write_5m_nanos_per_mtok: nanosSchema.nullable().default(null),
  cache_write_1h_nanos_per_mtok: nanosSchema.nullable().default(null),
  output_nanos_per_mtok: nanosSchema,
  source_url: boundedText(CATALOGUE_LIMITS.evidenceUrl).pipe(z.string().min(1)),
  verified_at: instantSchema, conditions: boundedText(CATALOGUE_LIMITS.conditions),
});
export const inferenceSchema = z.object({
  api: z.enum(INFERENCE_API), base_url: boundedText(CATALOGUE_LIMITS.evidenceUrl).pipe(z.string().min(1)),
  dialect: boundedText(CATALOGUE_LIMITS.dialect).pipe(z.string().min(1)).optional(),
  max_output_tokens: z.uint32().positive(),
});
export type CatalogueMetadata = z.infer<typeof metadataSchema>;
export type CataloguePrice = z.infer<typeof priceSchema>;
export type ModelInference = z.infer<typeof inferenceSchema>;
