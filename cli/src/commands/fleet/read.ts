// Validate the fields each fleet view consumes before rendering or JSON success.

import { Effect, Schema } from "effect";
import { ServerError } from "../../errors/index.ts";
import { isInstant } from "../../lib/instant.ts";
import { nanosSchema } from "../../lib/nanos.ts";
import { OUTPUT_FORMAT, type Output } from "../../services/output.ts";

const DECIMAL = /^(0|[1-9][0-9]*)$/;
/** A daemon counter: a decimal string, because nanos outgrow a JavaScript number. */
export const decimal = Schema.String.check(Schema.isPattern(DECIMAL));

/** `http_egress` on the fleet row: private by default, and a person may open it. */
export const HTTP_EGRESS = {restricted: "restricted", public: "public"} as const;

// FleetSummary in rustd/crates/daemon/afd_api_wire/src/fleet/collection.rs.
const FleetSummarySchema = Schema.Struct({
  id: Schema.String, name: Schema.String, status: Schema.String,
  events_processed: decimal, budget_used_nanos: nanosSchema,
});
const FleetListSchema = Schema.Struct({ items: Schema.Array(FleetSummarySchema) });
const FleetPageSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({
    ...FleetSummarySchema.fields, created_at: Schema.String.check(Schema.makeFilter(isInstant)),
  })),
  next_cursor: Schema.NullOr(Schema.String),
});
// The `Fleet` that `GET F` and `PATCH F` answer, as far as a command reads it:
// the tag a write must match, and the model and network a person can change.
const FleetSchema = Schema.Struct({
  id: Schema.String, name: Schema.String, status: Schema.String, etag: Schema.String,
  model_ref: Schema.String, http_egress: Schema.Literals(Object.values(HTTP_EGRESS)), package_domains: Schema.Boolean,
});
export type Fleet = typeof FleetSchema.Type;

/** Status prints the original body as JSON and the validated rows as text. */
export interface FleetsRead {
  readonly body: Record<string, unknown>;
  readonly fleets: ReadonlyArray<typeof FleetSummarySchema.Type>;
}

export const FLEET_LIST_INVALID = "FLEET_LIST_INVALID";
export const FLEET_RESPONSE_INVALID = "FLEET_RESPONSE_INVALID";
const BAD_GATEWAY = 502;

function decode<T>(schema: Schema.Codec<T, unknown>, value: unknown, code: string): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The fleet response did not match the supported API shape",
    suggestion: "update the CLI and verify that it targets the matching daemon build",
    code, status: BAD_GATEWAY, requestId: null,
  })));
}

/** JSON for `--json`, and for a stdout that is not a terminal: nobody is reading a table there. */
export const writesJson = (output: Output): boolean =>
  output.format !== OUTPUT_FORMAT.text || !output.stdoutIsTty;

export const decodeFleetList = (value: unknown) => decode(FleetListSchema, value, FLEET_LIST_INVALID);
export const decodeFleetPage = (value: unknown) => decode(FleetPageSchema, value, FLEET_LIST_INVALID);
export const decodeFleet = (value: unknown) => decode(FleetSchema, value, FLEET_RESPONSE_INVALID);
