// `zombiectl zombie list` — paginated table of zombies in a workspace.
// Workspace defaults to `current_workspace_id`; `--workspace-id` overrides.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { wsZombiesPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import {
  ConfigError,
  type CliError,
  type UnexpectedError,
} from "../errors/index.ts";

interface ZombieListRow {
  readonly [key: string]: unknown;
}

interface ZombieListResponse {
  readonly items?: ReadonlyArray<ZombieListRow>;
  readonly cursor?: string | null;
}

const resolveWorkspaceOverride = (
  override: string | undefined,
): Effect.Effect<string, ConfigError | UnexpectedError, Workspaces> =>
  Effect.gen(function* () {
    if (typeof override === "string" && override.length > 0) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    if (!state.current_workspace_id) {
      return yield* Effect.fail(
        new ConfigError({
          detail: "no workspace selected",
          suggestion: "run `zombiectl workspace use <id>`",
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
  if (typeof cursor === "string" && cursor.length > 0) qs.set("cursor", cursor);
  if (typeof limit === "string" && limit.length > 0) qs.set("limit", limit);
  const query = qs.toString();
  return query ? `${wsZombiesPath(wsId)}?${query}` : wsZombiesPath(wsId);
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
    const res = yield* http.request<ZombieListResponse>({
      path: buildPath(wsId, flags.cursor, flags.limit),
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }

    const items = res.items ?? [];
    if (items.length === 0) {
      yield* output.info("No zombies in this workspace.");
      return;
    }

    yield* output.printTable(
      [
        { key: "name", label: "NAME" },
        { key: "zombie_id", label: "ZOMBIE" },
        { key: "status", label: "STATUS" },
      ],
      items.map((z) => ({
        name: String(z["name"] ?? ""),
        zombie_id: String(z["zombie_id"] ?? z["id"] ?? ""),
        status: String(z["status"] ?? ""),
      })),
    );
    if (res.cursor) {
      yield* output.info(
        ui.dim(`More available. Next: zombiectl zombie list --cursor ${res.cursor}`),
      );
    }
  });
