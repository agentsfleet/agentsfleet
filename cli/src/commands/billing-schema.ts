import { Effect, Schema } from "effect";
import { nanosSchema } from "../lib/nanos.ts";
import { instant } from "../lib/instant.ts";
import { ServerError } from "../errors/index.ts";

// Validate the fields this command consumes; unrelated response metadata is ignored.
const billing = Schema.Struct({ balance_nanos: nanosSchema, is_exhausted: Schema.Boolean });
const optionalText = Schema.optionalKey(Schema.NullOr(Schema.String));
const count = Schema.optionalKey(Schema.NullOr(
  Schema.Number.check(Schema.isInt(), Schema.isGreaterThanOrEqualTo(0)),
));
const charge = Schema.Struct({
  fleet_id: optionalText, event_id: optionalText, authoring_attempt_id: optionalText,
  posture: Schema.optionalKey(Schema.String), model: Schema.optionalKey(Schema.String),
  recorded_at: Schema.optionalKey(Schema.NullOr(instant)), charge_type: Schema.String,
  credit_deducted_nanos: nanosSchema, token_count_input: count, token_count_output: count,
});
const charges = Schema.Struct({
  items: Schema.Array(charge), next_cursor: Schema.optionalKey(Schema.NullOr(Schema.String)),
});
export type BillingSnapshot = typeof billing.Type;
export type ChargeRow = typeof charge.Type;

function decode<T>(schema: Schema.Codec<T, unknown>, value: unknown): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The billing response did not match the supported API shape",
    suggestion: "verify that the terminal client targets the matching daemon build",
    code: "BILLING_RESPONSE_INVALID", status: 502, requestId: null,
  })));
}
export const decodeBilling = (value: unknown) => decode(billing, value);
export const decodeCharges = (value: unknown) => decode(charges, value);
