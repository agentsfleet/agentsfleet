// `agentsfleet fleet triggers list|remove` — what can wake a fleet from outside:
// a webhook, a GitHub App event or a Slack mention. A run adds a trigger with
// its trigger tool when a person asks it to; a person reads and removes them here.

import type { CliConfig } from "../../services/config.ts";
import type { Credentials } from "../../services/credentials.ts";
import type { Workspaces } from "../../services/workspaces.ts";
import { Effect, Schema } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { ServerError, type CliError } from "../../errors/index.ts";
import { wsFleetTriggerPath, wsFleetTriggersPath } from "../../lib/api-paths.ts";
import { isInstant } from "../../lib/instant.ts";
import { EMPTY_CELL } from "../../output/index.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output } from "../../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { writesJson } from "./read.ts";
import { resolveSteerTarget } from "./steer-target.ts";

type FleetServices = CliConfig | Credentials | HttpClient | Output | Workspaces;

const names = Schema.optionalKey(Schema.NullOr(Schema.Array(Schema.String)));
const text = Schema.optionalKey(Schema.NullOr(Schema.String));
// `Trigger` as `GET F/triggers` lists it; each kind fills its own optional fields.
const TriggerPageSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({
    id: Schema.String, kind: Schema.String, source: Schema.String,
    events: names, repositories: names, channel: text, url: text,
    created_at: Schema.String.check(Schema.makeFilter(isInstant)),
  })),
});
type Trigger = typeof TriggerPageSchema.Type["items"][number];

const TRIGGER_SPEC = {
  id: { key: "id" },
  domain: [
    { key: "kind", label: "KIND" },
    { key: "source", label: "SOURCE" },
    { key: "wakes_on", label: "WAKES ON" },
    { key: "url", label: "URL" },
  ],
} as const;
const NO_TRIGGERS =
  "No triggers. Ask the fleet to wake on a webhook, a GitHub event or a Slack mention, and it adds one." as const;
export const TRIGGER_RESPONSE_INVALID = "TRIGGER_RESPONSE_INVALID";

export const triggersListEffect = (
  fleetArgument: string,
): Effect.Effect<void, CliError, FleetServices> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const fleetId = yield* resolveSteerTarget(wsId, fleetArgument, token);
    const body = yield* http.request<unknown>({ path: wsFleetTriggersPath(wsId, fleetId), token });
    const page = yield* Schema.decodeUnknownEffect(TriggerPageSchema)(body).pipe(Effect.mapError(() => new ServerError({
      detail: "The trigger list did not match the supported API shape",
      suggestion: "update the CLI and verify that it targets the matching daemon build",
      code: TRIGGER_RESPONSE_INVALID, status: 502, requestId: null,
    })));
    if (writesJson(output)) return yield* output.printJson(body);
    if (page.items.length === 0) return yield* output.info(NO_TRIGGERS);
    yield* output.printEntityTable(TRIGGER_SPEC, page.items.map((trigger) => ({
      id: trigger.id, kind: trigger.kind, source: trigger.source,
      wakes_on: wakesOn(trigger), url: trigger.url ?? EMPTY_CELL, created_at: trigger.created_at,
    })));
  });

export const triggerRemoveEffect = (
  fleetArgument: string,
  triggerId: string,
): Effect.Effect<void, CliError, FleetServices> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const fleetId = yield* resolveSteerTarget(wsId, fleetArgument, token);
    yield* http.request<unknown>({ path: wsFleetTriggerPath(wsId, fleetId, triggerId), method: HTTP_METHOD.delete, token });
    yield* writesJson(output)
      ? output.printJson({ deleted: true, trigger_id: triggerId })
      : output.success(`Removed trigger ${triggerId}; it no longer wakes the fleet.`);
  });

// What the row listens for: a mention's channel, an App subscription's
// repositories and events, a webhook's events.
function wakesOn(trigger: Trigger): string {
  const parts = [trigger.channel, ...(trigger.repositories ?? []), ...(trigger.events ?? [])]
    .filter((part): part is string => typeof part === "string" && part.length > 0);
  return parts.length > 0 ? parts.join(", ") : EMPTY_CELL;
}
