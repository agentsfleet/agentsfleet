import { Schema } from "effect";
import {DecisionProfileSchema} from "./decision-schema.ts";
import { instant, instantNanos } from "../../lib/instant.ts";
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
const decision = Schema.Struct({enabled: Schema.Boolean, profile: DecisionProfileSchema, prices});
const codingPrices = Schema.Array(price).check(Schema.isMaxLength(64), Schema.makeFilter(rows => rows.length === 0 || validSchedule(rows)));
export const PlatformModelSettingsSchema = Schema.Struct({metadata, prices: codingPrices, inference: Schema.optionalKey(inference), decision: Schema.optionalKey(decision)});
export const PlatformModelCreateSchema = Schema.Struct({
  ...PlatformModelSettingsSchema.fields, provider: text.check(Schema.isMaxLength(64)), model_id: text.check(Schema.isMaxLength(256)),
}).check(Schema.makeFilter(row => row.prices.length > 0 || (row.inference === undefined && row.decision !== undefined)));
export const PlatformModelSchema = Schema.Struct({
  ...PlatformModelCreateSchema.fields, id: text, prices: Schema.Array(price),
  decision: Schema.optionalKey(decision),
});
export const PlatformModelPageSchema = Schema.Struct({models: Schema.Array(PlatformModelSchema), next_cursor: Schema.NullOr(text)});
export const PlatformModelUpdatedSchema = Schema.Struct({id: text, updated: Schema.Boolean});

function validSchedule(rows: ReadonlyArray<typeof price.Type>): boolean {
  return rows.some((row) => row.prompt_tokens_above === 0) && rows.every((row, index) => {
    const start = instantNanos(row.effective_from);
    const end = row.effective_until ? instantNanos(row.effective_until) : null;
    return (end === null || end > start) && !rows.slice(0, index).some((previous) =>
      previous.prompt_tokens_above === row.prompt_tokens_above
      && (end === null || instantNanos(previous.effective_from) < end)
      && (!previous.effective_until || start < instantNanos(previous.effective_until)));
  });
}
