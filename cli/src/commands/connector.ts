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
import {
  summarizeConnector,
  summarizeStatus,
  type ConnectorCatalogEntry,
} from "../services/connectors.ts";

type ConnectorStatusResponse = Record<string, unknown>;

const PROVIDER_RE = /^[a-z][a-z0-9_-]{0,63}$/;
const CONTROL_BYTES_RE = /[\u0000-\u001f\u007f-\u009f]/g;
const CONNECTOR_LIST_HINT = "run `agentsfleet connector list` to see provider ids";
const FIELD_PROVIDER = "provider";
const FIELD_STATE = "state";

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
          suggestion: CONNECTOR_LIST_HINT,
        }),
      );
    }
    return provider;
  });

const cleanTableCell = (value: string): string => value.replace(CONTROL_BYTES_RE, "");

const primitive = (value: unknown, clean: boolean): string | null => {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return clean ? cleanTableCell(value) : value;
  if (typeof value === "number" || typeof value === "boolean") {
    const rendered = String(value);
    return clean ? cleanTableCell(rendered) : rendered;
  }
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

    const summaries = entries.map(summarizeConnector);
    if (config.jsonMode) {
      yield* output.printJson(summaries);
      return;
    }
    if (summaries.length === 0) {
      yield* output.info("no connectors found");
      return;
    }
    yield* output.printTable(
      [
        { key: FIELD_PROVIDER, label: "PROVIDER" },
        { key: "display_name", label: "NAME" },
        { key: FIELD_STATE, label: "STATE" },
        { key: "hint", label: "NEXT ACTION" },
        { key: "archetype", label: "KIND" },
      ],
      summaries.map((entry) => ({
        provider: cleanTableCell(entry.provider),
        display_name: cleanTableCell(entry.display_name),
        state: entry.state,
        hint: cleanTableCell(entry.hint ?? "-"),
        archetype: cleanTableCell(entry.archetype),
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

    const entries = yield* http.request<ReadonlyArray<ConnectorCatalogEntry>>({
      path: wsConnectorsPath(workspaceId),
      token,
    });
    const entry = entries.find((candidate) => candidate.id === provider);
    if (!entry) {
      return yield* Effect.fail(new ValidationError({
        detail: `unknown connector provider: ${provider}`,
        suggestion: CONNECTOR_LIST_HINT,
      }));
    }
    const res = entry.configured
      ? yield* http.request<ConnectorStatusResponse>({
          path: wsConnectorPath(workspaceId, provider),
          token,
        })
      : null;
    const summary = summarizeStatus(entry, res);

    if (config.jsonMode) {
      yield* output.printJson(summary);
      return;
    }

    const rows = Object.entries(summary.details)
      .map(([field, value]) => ({ field: cleanTableCell(field), value: primitive(value, true) }))
      .filter((row): row is { field: string; value: string } => row.value !== null);
    yield* output.printTable(
      [
        { key: "field", label: "FIELD" },
        { key: "value", label: "VALUE" },
      ],
      [
        { field: FIELD_PROVIDER, value: provider },
        { field: FIELD_STATE, value: summary.state },
        ...(summary.hint ? [{ field: "next_action", value: cleanTableCell(summary.hint) }] : []),
        ...rows.filter((row) => row.field !== "status"),
      ],
    );
  });
