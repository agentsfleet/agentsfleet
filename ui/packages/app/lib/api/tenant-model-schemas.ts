import { z } from "zod";
import { instantSchema, nanosSchema } from "./catalogue-types";
import { SECRET_KIND } from "./secrets-types";

const rates = {
  input_nanos_per_mtok: nanosSchema.optional(), cached_input_nanos_per_mtok: nanosSchema.optional(),
  output_nanos_per_mtok: nanosSchema.optional(),
};
export const tenantModelEntrySchema = z.object({
  id: z.string(), model_id: z.string(), display_name: z.string().nullable(), secret_ref: z.string(),
  provider: z.string().optional(), kind: z.enum(SECRET_KIND), base_url: z.string().optional(),
  has_key: z.boolean(), context_cap_tokens: z.uint32().optional(), ...rates,
  active: z.boolean(), created_at: instantSchema,
});
export const tenantPlatformDefaultSchema = z.object({
  provider: z.string(), model: z.string(), display_name: z.string().nullable(), context_cap_tokens: z.uint32(), ...rates,
});
export const tenantModelPageSchema = z.object({
  models: z.array(tenantModelEntrySchema), total: z.number().int().nonnegative().nullable(), next_cursor: z.string().nullable(),
  platform_default_available: z.boolean(), platform_default: tenantPlatformDefaultSchema.optional(),
});
export const storedModelEntrySchema = z.object({ id: z.string(), model_id: z.string(), secret_ref: z.string(), created_at: instantSchema });
