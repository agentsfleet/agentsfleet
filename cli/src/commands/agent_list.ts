// `agentsfleet agent list` — paginated table of agents in a workspace.
// Workspace defaults to `current_workspace_id`; `--workspace-id` overrides.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { wsAgentsPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import {
  ConfigError,
  type CliError,
  type UnexpectedError,
} from "../errors/index.ts";

interface AgentListRow {
  readonly [key: string]: unknown;
}

interface AgentListResponse {
  readonly items?: ReadonlyArray<AgentListRow>;
  readonly cursor?: string | null;
}

const FIELD_NAME = "name" as const;
const FIELD_STATUS = "status" as const;
const TYPE_STRING = "string" as const;
const FIELD_AGENTSFLEET_ID = "agent_id" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

const resolveWorkspaceOverride = (
  override: string | undefined,
): Effect.Effect<string, ConfigError | UnexpectedError, Workspaces> =>
  Effect.gen(function* () {
    if (isString(override) && override.length > 0) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    if (!state.current_workspace_id) {
      return yield* Effect.fail(
        new ConfigError({
          detail: "no workspace selected",
          suggestion: "run `agentsfleet workspace use <id>`",
        }),
      );
    }
    return state.current_workspace_id;
  });

const buildPath = (
  wsId: string,
  cursor: string | undefined,
  limit: string | undefined,
): string => {
  const qs = new URLSearchParams();
  if (isString(cursor) && cursor.length > 0) qs.set("cursor", cursor);
  if (isString(limit) && limit.length > 0) qs.set("limit", limit);
  const query = qs.toString();
  return query ? `${wsAgentsPath(wsId)}?${query}` : wsAgentsPath(wsId);
};

export interface ListEffectFlags {
  readonly workspaceId?: string | undefined;
  readonly cursor?: string | undefined;
  readonly limit?: string | undefined;
}

export const listEffectFromFlags = (
  flags: ListEffectFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const wsId = yield* resolveWorkspaceOverride(flags.workspaceId);
    const token = yield* resolveAuthToken;
    const res = yield* http.request<AgentListResponse>({
      path: buildPath(wsId, flags.cursor, flags.limit),
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }

    const items = res.items ?? [];
    if (items.length === 0) {
      yield* output.info("No agents in this workspace.");
      return;
    }

    yield* output.printTable(
      [
        { key: FIELD_NAME, label: "NAME" },
        { key: FIELD_AGENTSFLEET_ID, label: "AGENT" },
        { key: FIELD_STATUS, label: "STATUS" },
      ],
      items.map((z) => ({
        name: String(z[FIELD_NAME] ?? ""),
        agent_id: String(z[FIELD_AGENTSFLEET_ID] ?? z["id"] ?? ""),
        status: String(z[FIELD_STATUS] ?? ""),
      })),
    );
    if (res.cursor) {
      yield* output.info(
        ui.dim(`More available. Next: agentsfleet agent list --cursor ${res.cursor}`),
      );
    }
  });
