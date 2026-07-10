// External Fleet Key CLI commands — Effect-shaped.
//
// Manages agt_a API keys issued to LangGraph/CrewAI/Composio fleets.
// The raw key is shown once at creation and cannot be retrieved again.
//
// agentsfleet fleet-key create    --workspace <ws> --fleet <id> --name <name> [--description <desc>]
// agentsfleet fleet-key list   --workspace <ws>
// agentsfleet fleet-key delete --workspace <ws> <fleet_key_id>

import { Effect } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { WORKSPACES_PATH } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

interface FleetKeyResponse {
  readonly fleet_key_id?: string;
  readonly key?: string;
  readonly created_at?: number | string | null;
}

interface FleetRow {
  readonly fleet_key_id?: string;
  readonly name?: string;
  readonly description?: string;
  readonly last_used_at?: number | string | null;
}

interface FleetListResponse {
  readonly items?: ReadonlyArray<FleetRow>;
}

export interface FleetAddArgs {
  readonly workspaceId: string | undefined;
  readonly fleetId: string | undefined;
  readonly name: string | undefined;
  readonly description: string | undefined;
}

const requireFlag = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  value
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const requireValidId = (
  value: string,
  fieldName: string,
): Effect.Effect<string, ValidationError> => {
  const check = validateRequiredId(value, fieldName);
  if (!check.ok) {
    return Effect.fail(
      new ValidationError({
        detail: check.message,
        suggestion: "pass a valid uuidv7",
      }),
    );
  }
  return Effect.succeed(value);
};

const resolveWorkspaceId = (
  override: string | undefined,
): Effect.Effect<string, CliError, Workspaces> =>
  Effect.gen(function* () {
    if (override) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    return yield* requireFlag(
      state.current_workspace_id ?? undefined,
      "fleet-key command requires --workspace <id> or an active workspace context",
      "run `agentsfleet workspace use <id>` or pass --workspace <id>",
    );
  });

const fleetKeysPath = (workspaceId: string): string =>
  `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/fleet-keys`;

const fleetKeyPath = (workspaceId: string, fleetKeyId: string): string =>
  `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/fleet-keys/${encodeURIComponent(fleetKeyId)}`;

export const fleetAddEffectFromArgs = (
  args: FleetAddArgs,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(args.workspaceId);
    const fleetId = yield* requireFlag(
      args.fleetId,
      "fleet-key create requires --fleet <id>",
      "pass --fleet <fleet_id>",
    );
    const name = yield* requireFlag(
      args.name,
      "fleet-key create requires --name <name>",
      "pass --name <name>",
    );
    const description = args.description ?? "";

    const res = yield* http.request<FleetKeyResponse>({
      path: fleetKeysPath(workspaceId),
      method: "POST",
      body: { fleet_id: fleetId, name, description },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    yield* output.success(`Fleet key created: ${res.fleet_key_id ?? ""}`);
    yield* output.info("");
    // The shown-once warning belongs on stdout next to the key.
    // Output.warn would route to stderr; surface this as an info line
    // so the integration test (which reads stdout) still sees it.
    yield* output.info("API Key (shown once — store securely):");
    yield* output.info(`  ${res.key ?? ""}`);
    yield* output.info("");
    yield* output.info(`Use as: Authorization: Bearer <key>`);
    yield* output.info(`Authenticated fleet: ${fleetId}`);
    yield* output.info("");
    yield* output.printTable(
      [
        { key: "label", label: "" },
        { key: "value", label: "" },
      ],
      [
        { label: AGENT_ID_2, value: res.fleet_key_id ?? "" },
        { label: "fleet_id", value: fleetId },
        { label: FIELD_NAME, value: name },
        {
          label: "created_at",
          value: res.created_at ? new Date(res.created_at).toISOString() : "—",
        },
      ],
    );
  });

export const fleetListEffectFromArgs = (
  workspaceIdFlag: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);

    const res = yield* http.request<FleetListResponse>({
      path: fleetKeysPath(workspaceId),
      token,
    });
    const fleets = res.items ?? [];

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    if (fleets.length === 0) {
      yield* output.info("no fleet keys found");
      return;
    }
    yield* output.printTable(
      [
        { key: FIELD_NAME, label: "NAME" },
        { key: "description", label: "DESCRIPTION" },
        { key: "last_used_at", label: "LAST_USED" },
        { key: AGENT_ID_2, label: "AGENT_KEY_ID" },
      ],
      fleets.map((a) => ({
        name: a.name ?? "",
        description: a.description ?? "",
        last_used_at: a.last_used_at
          ? new Date(a.last_used_at).toISOString()
          : "never",
        fleet_key_id: a.fleet_key_id ?? "",
      })),
    );
  });

export const fleetDeleteEffectFromArgs = (
  workspaceIdFlag: string | undefined,
  fleetKeyIdPositional: string | undefined,
  fleetKeyIdFlag: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);
    yield* requireValidId(workspaceId, "workspace_id");
    const fleetKeyIdRaw = yield* requireFlag(
      fleetKeyIdPositional ?? fleetKeyIdFlag,
      "fleet-key delete requires <fleet_key_id>",
      "pass <fleet_key_id> as positional or --fleet-key-id <id>",
    );
    const fleetKeyId = yield* requireValidId(fleetKeyIdRaw, "key_id");

    yield* http.request<unknown>({
      path: fleetKeyPath(workspaceId, fleetKeyId),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ deleted: true, fleet_key_id: fleetKeyId });
    } else {
      yield* output.success(
        `Fleet key ${fleetKeyId} deleted. Key immediately invalidated.`,
      );
    }
  });
const AGENT_ID_2 = "fleet_key_id" as const;
const FIELD_NAME = "name" as const;
