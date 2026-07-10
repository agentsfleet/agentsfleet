// Connector inspection commands. These are read-only mirrors of the dashboard
// catalog and per-provider status routes.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import {
  wsConnectorPath,
  wsConnectorsPath,
} from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

interface ConnectorCatalogEntry {
  readonly id?: string;
  readonly archetype?: string;
  readonly display_name?: string;
  readonly configured?: boolean;
  readonly connected?: boolean;
}

type ConnectorStatusResponse = Record<string, unknown>;

const PROVIDER_RE = /^[a-z][a-z0-9_-]{0,63}$/;
const STATUS_CONNECTED = "connected" as const;
const STATUS_NOT_CONNECTED = "not connected" as const;
const SETUP_READY = "configured" as const;
const SETUP_REQUIRED = "admin setup required" as const;

const requireValue = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  value
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const resolveWorkspaceId = (
  override: string | undefined,
): Effect.Effect<string, CliError, Workspaces> =>
  Effect.gen(function* () {
    if (override) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    return yield* requireValue(
      state.current_workspace_id ?? undefined,
      "connector command requires --workspace <id> or an active workspace context",
      "run `agentsfleet workspace use <id>` or pass --workspace <id>",
    );
  });

const requireProvider = (
  raw: string | undefined,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    const provider = yield* requireValue(
      raw,
      "connector status requires <provider>",
      "pass a provider id such as slack or github",
    );
    if (!PROVIDER_RE.test(provider)) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "provider must be lowercase letters, numbers, hyphens, or underscores",
          suggestion: "run `agentsfleet connector list` to see provider ids",
        }),
      );
    }
    return provider;
  });

const primitive = (value: unknown): string | null => {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
};

export const connectorListEffectFromArgs = (
  workspaceIdFlag: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);

    const entries = yield* http.request<ReadonlyArray<ConnectorCatalogEntry>>({
      path: wsConnectorsPath(workspaceId),
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(entries);
      return;
    }
    if (entries.length === 0) {
      yield* output.info("no connectors found");
      return;
    }
    yield* output.printTable(
      [
        { key: "id", label: "PROVIDER" },
        { key: "display_name", label: "NAME" },
        { key: "setup", label: "SETUP" },
        { key: "connection", label: "CONNECTION" },
        { key: "archetype", label: "KIND" },
      ],
      entries.map((entry) => ({
        id: entry.id ?? "",
        display_name: entry.display_name ?? "",
        setup: entry.configured ? SETUP_READY : SETUP_REQUIRED,
        connection: entry.connected ? STATUS_CONNECTED : STATUS_NOT_CONNECTED,
        archetype: entry.archetype ?? "",
      })),
    );
  });

export const connectorStatusEffectFromArgs = (
  workspaceIdFlag: string | undefined,
  providerRaw: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);
    const provider = yield* requireProvider(providerRaw);

    const res = yield* http.request<ConnectorStatusResponse>({
      path: wsConnectorPath(workspaceId, provider),
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }

    const rows = Object.entries(res)
      .map(([field, value]) => ({ field, value: primitive(value) }))
      .filter((row): row is { field: string; value: string } => row.value !== null);
    yield* output.printTable(
      [
        { key: "field", label: "FIELD" },
        { key: "value", label: "VALUE" },
      ],
      [{ field: "provider", value: provider }, ...rows],
    );
  });
