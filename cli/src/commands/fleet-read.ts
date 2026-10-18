// Validate the fields each fleet view consumes before rendering or JSON success.

import { Effect, Schema } from "effect";
import { ServerError } from "../errors/index.ts";
import { isInstant } from "../lib/instant.ts";
import { nanosSchema } from "../lib/nanos.ts";
import { decimal } from "./chat/protocol.ts";

// FleetSummary in rustd/crates/afd_api_wire/src/fleet/collection.rs.
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

/** Status prints the original body as JSON and the validated rows as text. */
export interface FleetsRead {
  readonly body: Record<string, unknown>;
  readonly fleets: ReadonlyArray<typeof FleetSummarySchema.Type>;
}

export const FLEET_LIST_INVALID = "FLEET_LIST_INVALID";
const BAD_GATEWAY = 502;

function decode<T>(schema: Schema.Codec<T, unknown>, value: unknown): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The fleet response did not match the supported API shape",
    suggestion: "update the CLI and verify that it targets the matching daemon build",
    code: FLEET_LIST_INVALID, status: BAD_GATEWAY, requestId: null,
  })));
}

export const decodeFleetList = (value: unknown) => decode(FleetListSchema, value);
export const decodeFleetPage = (value: unknown) => decode(FleetPageSchema, value);
