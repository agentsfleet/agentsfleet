import { z } from "zod";
import { INFERENCE_API } from "./admin-model-library-types";
import { nanosSchema } from "./nanos";
export { nanosSchema } from "./nanos";

const FRACTION_DIGITS = 9;

export const instantSchema = z.iso.datetime({ offset: true }).refine(
  (value) => (value.match(/\.(\d+)/)?.[1]?.length ?? 0) <= FRACTION_DIGITS,
  "Use at most nine fractional second digits",
);
export const metadataSchema = z.object({
  display_name: z.string().min(1),
  context_cap_tokens: z.int32().positive(),
  context_source_url: z.string().min(1),
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
  source_url: z.string().min(1),
  verified_at: instantSchema, conditions: z.string(),
});
export const inferenceSchema = z.object({
  api: z.enum(INFERENCE_API), base_url: z.string().min(1),
  dialect: z.string().min(1).optional(),
  max_output_tokens: z.uint32().positive(),
});
export type CatalogueMetadata = z.infer<typeof metadataSchema>;
export type CataloguePrice = z.infer<typeof priceSchema>;
export type ModelInference = z.infer<typeof inferenceSchema>;
