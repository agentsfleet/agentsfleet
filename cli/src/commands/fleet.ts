// Fleet CLI top-level command Effects — status / stop / resume / kill /
// delete. Install + update live in fleet_install.ts; list/logs/events/
// steer/secret leaves live in sibling files (fleet_list.ts,
// fleet_logs.ts, fleet_events.ts, fleet_steer.ts, fleet_secret.ts).
//
// Each command yields services from the MainLayer (CliConfig, Output,
// HttpClient, Credentials, Workspaces, Analytics) and emits one of the
// CliError variants on failure. The dispatcher's renderError prints the
// detail + suggestion + (for ServerError) request_id.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetsPath, wsFleetPath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  AGENTSFLEET_STATUS,
  type FleetMutationStatus,
} from "../constants/fleet-status.ts";
import { formatDollars } from "../constants/billing.ts";
import {
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const STATUS_PAST_TENSE: Record<FleetMutationStatus, string> = {
  [AGENTSFLEET_STATUS.STOPPED]: "stopped",
  [AGENTSFLEET_STATUS.ACTIVE]: "resumed",
  [AGENTSFLEET_STATUS.KILLED]: "killed",
};

const STATUS_VERB: Record<FleetMutationStatus, string> = {
  [AGENTSFLEET_STATUS.STOPPED]: "stop",
  [AGENTSFLEET_STATUS.ACTIVE]: "resume",
  [AGENTSFLEET_STATUS.KILLED]: "kill",
};

interface FleetListItem {
  readonly name?: string;
  readonly status?: string;
  readonly events_processed?: number;
  readonly budget_used_nanos?: number | null;
}

interface FleetListResponse {
  readonly items?: ReadonlyArray<FleetListItem>;
}

const requireFleetId = (
  fleetId: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!fleetId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "fleet_id is required",
          suggestion: `usage: ${usage}`,
        }),
      );
    }
    const check = validateRequiredId(fleetId, "fleet_id");
    if (!check.ok) {
      return yield* Effect.fail(
        new ValidationError({ detail: check.message, suggestion: `usage: ${usage}` }),
      );
    }
    return fleetId;
  });

export const statusEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const res = yield* http.request<FleetListResponse>({
    path: wsFleetsPath(wsId),
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }

  const fleets = res.items ?? [];
  if (fleets.length === 0) {
    yield* output.info(
      "No fleets running. Install one with: agentsfleet install --from <path>",
    );
    return;
  }

  yield* output.printSection("Fleets");
  for (const z of fleets) {
    yield* output.printKeyValue({
      Name: z.name ?? "",
      Status: z.status ?? "",
      Events: String(z.events_processed ?? 0),
      Budget: formatDollars(z.budget_used_nanos),
    });
  }
});

const setStatusEffect = (
  fleetId: string | undefined,
  status: FleetMutationStatus,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const verb = STATUS_VERB[status];
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireFleetId(fleetId, `agentsfleet ${verb} <fleet_id>`);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<unknown>({
      path: wsFleetPath(wsId, id),
      method: "PATCH",
      body: { status },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
    } else {
      yield* output.success(`${id} ${STATUS_PAST_TENSE[status]}.`);
    }
  });

export const stopEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(fleetId, AGENTSFLEET_STATUS.STOPPED);

export const resumeEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(fleetId, AGENTSFLEET_STATUS.ACTIVE);

export const killEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(fleetId, AGENTSFLEET_STATUS.KILLED);

export const deleteEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireFleetId(fleetId, "agentsfleet delete <fleet_id>");
    const token = yield* resolveAuthToken;

    yield* http.request<unknown>({
      path: wsFleetPath(wsId, id),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ fleet_id: id, deleted: true });
    } else {
      yield* output.success(`${id} deleted.`);
    }
  });
