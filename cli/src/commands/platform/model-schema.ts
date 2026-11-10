import { Schema } from "effect";
import { instant } from "../../lib/instant.ts";
import { nanosSchema } from "../../lib/nanos.ts";

const text = Schema.String.check(Schema.isMinLength(1));
const optionalNanos = Schema.optionalKey(Schema.NullOr(nanosSchema));
const integer = Schema.Number.check(Schema.isInt32(), Schema.isGreaterThanOrEqualTo(0));
const metadata = Schema.Struct({
  display_name: text.check(Schema.isMaxLength(128)), context_cap_tokens: integer.check(Schema.isGreaterThan(0)),
  context_source_url: text.check(Schema.isMaxLength(2048)), context_verified_at: instant, advanced: Schema.Boolean,
});
const inference = Schema.Struct({
  api: Schema.Literals(["chat", "responses", "messages", "gemini", "ollama"]), base_url: text,
  dialect: Schema.optionalKey(text), max_output_tokens: integer.check(Schema.isGreaterThan(0)),
});
const price = Schema.Struct({
  effective_from: instant, effective_until: Schema.optionalKey(Schema.NullOr(instant)), prompt_tokens_above: integer,
  input_nanos_per_mtok: nanosSchema, cached_input_nanos_per_mtok: nanosSchema, output_nanos_per_mtok: nanosSchema,
  cache_write_nanos_per_mtok: optionalNanos, cache_write_5m_nanos_per_mtok: optionalNanos, cache_write_1h_nanos_per_mtok: optionalNanos,
  source_url: text, verified_at: instant, conditions: Schema.String.check(Schema.isMaxLength(1024)),
});
const prices = Schema.Array(price).check(Schema.isMinLength(1), Schema.isMaxLength(64), Schema.makeFilter(validSchedule));
export const PlatformModelSettingsSchema = Schema.Struct({metadata, prices, inference: Schema.optionalKey(inference)});
export const PlatformModelCreateSchema = Schema.Struct({
  ...PlatformModelSettingsSchema.fields, provider: text.check(Schema.isMaxLength(64)), model_id: text.check(Schema.isMaxLength(256)),
});
export const PlatformModelSchema = Schema.Struct({
  ...PlatformModelCreateSchema.fields, id: text, prices: Schema.Array(price),
  decision: Schema.optionalKey(Schema.Struct({enabled: Schema.Boolean, profile: Schema.Struct({revision: text, api: text})})),
});
export const PlatformModelPageSchema = Schema.Struct({models: Schema.Array(PlatformModelSchema), next_cursor: Schema.NullOr(text)});
export const PlatformModelUpdatedSchema = Schema.Struct({id: text, updated: Schema.Boolean});

function validSchedule(rows: ReadonlyArray<typeof price.Type>): boolean {
  return rows.some((row) => row.prompt_tokens_above === 0) && rows.every((row, index) => {
    const start = epochNanos(row.effective_from);
    const end = row.effective_until ? epochNanos(row.effective_until) : null;
    return (end === null || end > start) && !rows.slice(0, index).some((previous) =>
      previous.prompt_tokens_above === row.prompt_tokens_above
      && (end === null || epochNanos(previous.effective_from) < end)
      && (!previous.effective_until || start < epochNanos(previous.effective_until)));
  });
}

function epochNanos(value: string): bigint {
  const fraction = value.match(/\.(\d+)/u)?.[1] ?? "";
  return BigInt(Date.parse(value.replace(/\.\d+/u, ""))) * 1_000_000n + BigInt(fraction.padEnd(9, "0"));
}
