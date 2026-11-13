// Fleet CLI top-level command Effects — status / activate / archive / delete. Creation and improvement use terminal chat; list/logs/events/
// steer/secret leaves live in sibling files (fleet_list.ts,
// fleet_logs.ts, fleet_events.ts, fleet_steer.ts, fleet_secret.ts).
//
// Each command yields services from the MainLayer (CliConfig, Output,
// HttpClient, Credentials, Workspaces, Analytics) and emits one of the
// CliError variants on failure. The dispatcher's renderError prints the
// detail + suggestion + (for ServerError) request_id.

import { Effect } from "effect";
import { CliConfig } from "../../services/config.ts";
import { Credentials } from "../../services/credentials.ts";
import { HttpClient } from "../../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../../services/output.ts";
import { Workspaces } from "../../services/workspaces.ts";
import { requireValidId, requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { wsFleetsPath, wsFleetPath } from "../../lib/api-paths.ts";
import { ui, EMPTY_CELL } from "../../output/index.ts";
import { pendingGateCounts } from "../approvals/pending.ts";
import { AGENTSFLEET_STATUS } from "../../constants/fleet-status.ts";
import { formatDollars } from "../../constants/billing.ts";
import { decode, FleetSchema } from "../chat/protocol.ts";
import { decodeFleetList, type FleetsRead } from "./read.ts";
import { IF_MATCH } from "../chat/transport.ts";
import type { CliError } from "../../errors/index.ts";

const FLEET_SHOW_USAGE = "agentsfleet fleet show <fleet_id>" as const;
const FLEET_ID_FIELD = "fleet_id" as const;

type FleetServices = CliConfig | Credentials | HttpClient | Output | Workspaces;

const FLEET_ACTIVATE_USAGE = "agentsfleet fleet activate <fleet_id>" as const;
const FLEET_ARCHIVE_USAGE = "agentsfleet fleet archive <fleet_id>" as const;
const FLEET_DELETE_USAGE = "agentsfleet fleet delete <fleet_id>" as const;

const printFleetRows = (
  fleets: FleetsRead["fleets"],
  waitingByFleet: ReadonlyMap<string, number> | null,
): Effect.Effect<void, never, Output> => Effect.gen(function* () {
  const output = yield* Output;
  yield* output.printSection("Fleets");
  let anyParked = false;
  for (const z of fleets) {
    // An unreadable inbox renders the em dash, not 0: "0 waiting" for a parked
    // Fleet is the answer this column exists to stop being given.
    const waiting =
      waitingByFleet === null
        ? EMPTY_CELL
        : String(waitingByFleet.get(z.id) ?? 0);
    if (waiting !== EMPTY_CELL && waiting !== "0") anyParked = true;
    yield* output.printKeyValue({
      Name: z.name,
      Status: z.status,
      Events: z.events_processed,
      Budget: formatDollars(z.budget_used_nanos),
      Waiting: waiting,
    });
  }
  if (anyParked) {
    yield* output.info(ui.dim(PARKED_HINT));
  }
  if (waitingByFleet === null) {
    yield* output.info(ui.dim(WAITING_UNREADABLE));
  }
});

// Read a selected fleet directly: filtering the paged list can miss it.
const statusEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<void, CliError, FleetServices> => Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;
  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const read: FleetsRead = fleetId === undefined
    ? yield* Effect.flatMap(
        http.request<Record<string, unknown>>({ path: wsFleetsPath(wsId), token }),
        (body) => Effect.map(decodeFleetList(body), (listed): FleetsRead => ({ body, fleets: listed.items })),
      )
    : yield* Effect.flatMap(
        http.request<Record<string, unknown>>({
          path: wsFleetPath(wsId, yield* requireValidId(fleetId, FLEET_ID_FIELD, `usage: ${FLEET_SHOW_USAGE}`)),
          token,
        }),
        (one) => Effect.map(decodeFleetList({ items: [one] }), (listed): FleetsRead => ({
          body: { items: [one] }, fleets: listed.items,
        })),
      );
  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.success(FLEETS_SHOWN, { ...read.body });
    return;
  }
  if (read.fleets.length === 0) {
    yield* output.info(NO_FLEETS_HINT);
    return;
  }
  // The list omits pending approvals; an unreadable inbox must stay unknown.
  yield* printFleetRows(read.fleets, yield* pendingGateCounts(wsId, token));
});

/** `agentsfleet fleet status` — every fleet in the active workspace. */
export const statusEffect = statusEffectFromId(undefined);

/** `agentsfleet fleet show <fleet_id>` — that one fleet. */
export const fleetShowEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<void, CliError, FleetServices> => statusEffectFromId(fleetId);

type LifecycleTarget = typeof AGENTSFLEET_STATUS.ACTIVE | typeof AGENTSFLEET_STATUS.ARCHIVED;

// One PATCH under the tag the GET just read. The line reports the status the
// server answered, never the one asked for: activate leaves a fleet whose
// setup still waits `installing`, and the reader needs the next step for that.
const setStatusEffect = (
  fleetId: string | undefined,
  status: LifecycleTarget,
  usage: string,
): Effect.Effect<void, CliError, FleetServices> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireValidId(fleetId, FLEET_ID_FIELD, `usage: ${usage}`);
    const token = yield* resolveAuthToken;
    const path = wsFleetPath(wsId, id);
    const observed = yield* http.request<unknown>({ path, token }).pipe(
      Effect.flatMap((body) => decode(FleetSchema, body)),
    );
    const res = yield* http.request<unknown>({
      path,
      method: "PATCH",
      body: { status },
      headers: {[IF_MATCH]: observed.etag},
      token,
      retry: null,
    }).pipe(Effect.flatMap((body) => decode(FleetSchema, body)));
    const line = res.status === AGENTSFLEET_STATUS.INSTALLING
      ? `${id} ${res.status}; setup is still waiting. See what it needs with: agentsfleet fleet show ${id}`
      : `${id} ${res.status}.`;
    yield* output.success(line, { ...res });
  });

// Activate re-checks a fleet's setup and activates it once every check
// passes: the step to take after fixing a missing secret, connector, model or
// balance. An approved grant runs the same step on its own.
export const activateEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<void, CliError, FleetServices> =>
  setStatusEffect(fleetId, AGENTSFLEET_STATUS.ACTIVE, FLEET_ACTIVATE_USAGE);

// Archive is terminal: the server cancels the fleet's live work and keeps its
// history, and the fleet never runs again.
export const archiveEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<void, CliError, FleetServices> =>
  setStatusEffect(fleetId, AGENTSFLEET_STATUS.ARCHIVED, FLEET_ARCHIVE_USAGE);

export const deleteEffectFromId = (
  fleetId: string | undefined,
): Effect.Effect<void, CliError, FleetServices> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireValidId(fleetId, FLEET_ID_FIELD, `usage: ${FLEET_DELETE_USAGE}`);
    const token = yield* resolveAuthToken;

    const path = wsFleetPath(wsId, id);
    const observed = yield* http.request<unknown>({path, token}).pipe(
      Effect.flatMap((body) => decode(FleetSchema, body)),
    );
    yield* http.request<unknown>({
      path,
      method: "DELETE",
      headers: {[IF_MATCH]: observed.etag},
      token,
      retry: null,
    });

    yield* output.success(`${id} deleted.`, { fleet_id: id, deleted: true });
  });

const FLEETS_SHOWN = "Fleet status" as const;

const NO_FLEETS_HINT =
  "No fleets running. Create reviewed setup with: agentsfleet chat" as const;

// A Fleet with a waiting gate does nothing until someone decides it, so the
// status that reports the wait also names the command that ends it.
const PARKED_HINT =
  "Some fleets are waiting on approval. Review with: agentsfleet fleet approvals list" as const;

// The waiting count when the approvals inbox could not be read at all.
// Names what happened, never why. The read fails the same way for a missing
// `approval:read` scope, a timeout, and a daemon that is down, and this line
// cannot tell them apart — blaming scopes would send an operator to re-auth
// while the service is simply unavailable. `approvals list` fails loudly with
// the daemon's own sentence, so pointing there hands over the real cause
// instead of guessing at it.
const WAITING_UNREADABLE =
  "Waiting counts unavailable — the approval inbox could not be read. See why with: agentsfleet fleet approvals list" as const;
