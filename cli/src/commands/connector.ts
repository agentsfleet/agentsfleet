// Connector inspection commands. These are read-only mirrors of the dashboard
// catalog and per-provider status routes.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  requireValue,
  resolveAuthToken,
  resolveWorkspaceId,
  WORKSPACE_FLAG,
} from "./workspace-guards.ts";
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
const CONNECTORS_LISTED = "Connectors" as const;
const CONNECTOR_SHOWN = "Connector" as const;

const FIELD_PROVIDER = "provider";
const FIELD_STATE = "state";

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
  workspaceFlagValue: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceFlagValue, WORKSPACE_FLAG);

    const entries = yield* http.request<ReadonlyArray<ConnectorCatalogEntry>>({
      path: wsConnectorsPath(workspaceId),
      token,
    });

    const summaries = entries.map(summarizeConnector);
    if (output.format !== OUTPUT_FORMAT.text) {
      // An array, not a record: the payload is what `printJson(summaries)`
      // emitted, so a script indexing position 0 still finds the same row.
      yield* output.success(CONNECTORS_LISTED, summaries);
      return;
    }
    if (summaries.length === 0) {
      yield* output.info("no connectors found");
      return;
    }
    yield* output.printEntityTable(
      {
        name: { key: "display_name", label: "NAME" },
        id: { key: FIELD_PROVIDER, label: "PROVIDER" },
        domain: [
          { key: FIELD_STATE, label: "STATE" },
          { key: "hint", label: "NEXT ACTION" },
          { key: "archetype", label: "KIND" },
        ],
        // The summary carries no instant, so there is no age to report.
        ageKey: null,
      },
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
  workspaceFlagValue: string | undefined,
  providerRaw: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceFlagValue, WORKSPACE_FLAG);
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

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(CONNECTOR_SHOWN, { ...summary });
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
