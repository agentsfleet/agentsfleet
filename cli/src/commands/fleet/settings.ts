// `agentsfleet fleet model set` and `fleet network set` — what a person changes
// about a fleet after it starts. Each is one PATCH under the tag the read
// before it answered, and prints what the daemon then holds. Only a signed-in
// person may open a fleet's network; the daemon refuses an API key.

import type { CliConfig } from "../../services/config.ts";
import type { Credentials } from "../../services/credentials.ts";
import type { Workspaces } from "../../services/workspaces.ts";
import { Effect } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { ValidationError, type CliError } from "../../errors/index.ts";
import { HEADER_IF_MATCH, wsFleetPath } from "../../lib/api-paths.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output } from "../../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { decodeFleet, HTTP_EGRESS, writesJson, type Fleet } from "./read.ts";
import { resolveSteerTarget } from "./steer-target.ts";

export interface NetworkFlags {
  readonly allowAll: boolean;
  readonly private: boolean;
  /** Undefined leaves the fleet's package registries as they are. */
  readonly packages: boolean | undefined;
}

type FleetServices = CliConfig | Credentials | HttpClient | Output | Workspaces;
type FleetChange = Readonly<Record<string, unknown>>;

const MODEL_USAGE = "usage: agentsfleet fleet model set <fleet_id|@name> <ref>" as const;
const NETWORK_USAGE =
  "usage: agentsfleet fleet network set <fleet_id|@name> --allow-all|--private [--packages|--no-packages]" as const;

export const fleetModelSetEffect = (
  fleetArgument: string,
  modelRef: string,
): Effect.Effect<void, CliError, FleetServices> =>
  modelRef.trim().length === 0
    ? Effect.fail(new ValidationError({ detail: "a model reference is required", suggestion: MODEL_USAGE }))
    : patchFleet(fleetArgument, { model_ref: modelRef },
      (fleet) => `Fleet ${fleet.name} now runs on ${fleet.model_ref}; its next run uses it.`);

export const fleetNetworkSetEffect = (
  fleetArgument: string,
  flags: NetworkFlags,
): Effect.Effect<void, CliError, FleetServices> =>
  flags.allowAll === flags.private
    ? Effect.fail(new ValidationError({ detail: "choose exactly one of --allow-all or --private", suggestion: NETWORK_USAGE }))
    : patchFleet(fleetArgument, {
      http_egress: flags.allowAll ? HTTP_EGRESS.public : HTTP_EGRESS.restricted,
      ...(flags.packages === undefined ? {} : { package_domains: flags.packages }),
    }, (fleet) => `Fleet ${fleet.name}: ${fleet.http_egress === HTTP_EGRESS.public ? "any public host" : "private"}, ` +
      `package registries ${fleet.package_domains ? "allowed" : "blocked"}.`);

const patchFleet = (
  fleetArgument: string,
  change: FleetChange,
  line: (fleet: Fleet) => string,
): Effect.Effect<void, CliError, FleetServices> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const path = wsFleetPath(wsId, yield* resolveSteerTarget(wsId, fleetArgument, token));
    const observed = yield* http.request<unknown>({ path, token }).pipe(Effect.flatMap(decodeFleet));
    const body = yield* http.request<unknown>({
      path, method: HTTP_METHOD.patch, token, headers: { [HEADER_IF_MATCH]: observed.etag }, body: change,
    });
    const fleet = yield* decodeFleet(body);
    yield* writesJson(output) ? output.printJson(body) : output.success(line(fleet));
  });
