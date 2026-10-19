import { Effect, Schema } from "effect";
import { instant } from "./instant.ts";
import { nanosSchema } from "./nanos.ts";
import { ServerError } from "../errors/index.ts";

const optionalNanos = Schema.optionalKey(Schema.NullOr(nanosSchema));
const int32 = Schema.Number.check(Schema.isInt32(), Schema.isGreaterThanOrEqualTo(0));
const price = Schema.Struct({
  effective_from: instant, effective_until: Schema.optionalKey(Schema.NullOr(instant)),
  prompt_tokens_above: int32,
  input_nanos_per_mtok: nanosSchema, cached_input_nanos_per_mtok: nanosSchema,
  output_nanos_per_mtok: nanosSchema, cache_write_nanos_per_mtok: optionalNanos,
  cache_write_5m_nanos_per_mtok: optionalNanos, cache_write_1h_nanos_per_mtok: optionalNanos,
  source_url: Schema.String, verified_at: instant, conditions: Schema.String,
});
const model = Schema.Struct({
  id: Schema.String, provider: Schema.String, display_name: Schema.String,
  context_cap_tokens: int32, context_source_url: Schema.String, context_verified_at: instant,
  advanced: Schema.Boolean, prices: Schema.Array(price),
  input_nanos_per_mtok: nanosSchema, cached_input_nanos_per_mtok: nanosSchema,
  output_nanos_per_mtok: nanosSchema,
});
const page = Schema.Struct({
  version: Schema.String, models: Schema.Array(model),
  total: Schema.optionalKey(Schema.Null),
  next_cursor: Schema.optionalKey(Schema.NullOr(Schema.String)),
});
export type LibraryModel = typeof model.Type;

export function decodeCatalogue(value: unknown) {
  return Schema.decodeUnknownEffect(page)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The model catalogue response did not match the supported API shape",
    suggestion: "verify that the terminal client targets the matching daemon build",
    code: "MODEL_CATALOGUE_RESPONSE_INVALID", status: 502, requestId: null,
  })));
}
