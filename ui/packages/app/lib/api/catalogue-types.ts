import { z } from "zod";
import { INFERENCE_API } from "./admin-model-library-types";
import { nanosSchema } from "./nanos";
export { nanosSchema } from "./nanos";

const CONDITIONS_MAX_BYTES = 1024;
export const CATALOGUE_LIMITS = {
  prices: 64, displayName: 128, evidenceUrl: 2048, conditions: CONDITIONS_MAX_BYTES,
  provider: 64, modelId: 256, dialect: 64,
} as const;
export const BASE_TIER = 0;
export const SCHEDULE_ERRORS = {
  count: "Provide between 1 and 64 prices",
  base: "Include a base tier with Prompt tokens above set to 0",
  reversed: "Effective until must be after effective from",
  overlap: "Date ranges for the same prompt threshold must not overlap",
} as const;
const NANOS_PER_MILLISECOND = 1_000_000n;
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
export type ScheduleIssue = { index: number | null; field: "effective_until" | "effective_from"; message: string };

function epochNanos(value: string): bigint {
  const fraction = value.match(/\.(\d+)/)?.[1] ?? "";
  return BigInt(Date.parse(value.replace(/\.\d+/, ""))) * NANOS_PER_MILLISECOND
    + BigInt(fraction.padEnd(FRACTION_DIGITS, "0"));
}

export function priceScheduleIssues(prices: CataloguePrice[]): ScheduleIssue[] {
  const issues: ScheduleIssue[] = [];
  if (!prices.some((price) => price.prompt_tokens_above === BASE_TIER)) {
    issues.push({ index: null, field: "effective_from", message: SCHEDULE_ERRORS.base });
  }
  for (const [index, price] of prices.entries()) {
    const start = epochNanos(price.effective_from);
    const end = price.effective_until === null ? null : epochNanos(price.effective_until);
    if (end !== null && end <= start) issues.push({ index, field: "effective_until", message: SCHEDULE_ERRORS.reversed });
    const overlaps = prices.slice(0, index).some((previous) => previous.prompt_tokens_above === price.prompt_tokens_above
      && (end === null || epochNanos(previous.effective_from) < end)
      && (previous.effective_until === null || start < epochNanos(previous.effective_until)));
    if (overlaps) issues.push({ index, field: "effective_from", message: SCHEDULE_ERRORS.overlap });
  }
  return issues;
}

export const priceScheduleSchema = z.array(priceSchema).min(1, SCHEDULE_ERRORS.count)
  .max(CATALOGUE_LIMITS.prices, SCHEDULE_ERRORS.count).superRefine((prices, context) => {
    if (prices.some((price) => !priceSchema.safeParse(price).success)) return;
    for (const issue of priceScheduleIssues(prices)) {
      context.addIssue({ code: "custom", message: issue.message,
        path: issue.index === null ? [] : [issue.index, issue.field] });
    }
  });
export const modelSettingsSchema = z.object({
  metadata: metadataSchema, prices: priceScheduleSchema, inference: inferenceSchema.optional(),
});
export const adminModelSchema = modelSettingsSchema.extend({ id: z.string(), provider: z.string(), model_id: z.string() });
export const adminModelListSchema = z.object({ models: z.array(adminModelSchema) });
