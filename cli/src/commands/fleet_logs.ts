// `agentsfleet logs` — paginated event print for a specific fleet.
// Accepts `<fleet_id>` positional or `--fleet <id>` flag; `--limit`
// (default 20) and `--cursor` for pagination.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetEventsPath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import { ui } from "../output/index.ts";
import {
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const DEFAULT_LOGS_LIMIT = "20";
const USAGE = "logs requires --fleet <id>";
const TYPE_STRING = "string" as const;
const LITERAL = "—" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface EventRow {
  readonly created_at?: number | string | null;
  readonly response_text?: string | null;
  readonly status?: string | null;
  readonly actor?: string | null;
}

interface LogsResponse {
  readonly items?: ReadonlyArray<EventRow>;
  readonly next_cursor?: string | null;
}

export interface LogsEffectFlags {
  readonly fleetId?: string | undefined;
  readonly cursor?: string | undefined;
  readonly limit?: string | undefined;
}

const requireFleetId = (
  raw: string | undefined,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!raw) {
      return yield* Effect.fail(
        new ValidationError({ detail: USAGE, suggestion: USAGE }),
      );
    }
    const check = validateRequiredId(raw, "fleet_id");
    if (!check.ok) {
      return yield* Effect.fail(
        new ValidationError({ detail: check.message, suggestion: USAGE }),
      );
    }
    return raw;
  });

const formatTimestamp = (raw: number | string | null | undefined): string =>
  raw ? new Date(raw).toISOString() : LITERAL;

export const logsEffectFromFlags = (
  flags: LogsEffectFlags,
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
    const fleetId = yield* requireFleetId(flags.fleetId);

    const limit =
      isString(flags.limit) && flags.limit.length > 0
        ? flags.limit
        : DEFAULT_LOGS_LIMIT;
    const qs = new URLSearchParams();
    qs.set("limit", limit);
    if (isString(flags.cursor) && flags.cursor.length > 0) {
      qs.set("cursor", flags.cursor);
    }
    const path = `${wsFleetEventsPath(wsId, fleetId)}?${qs.toString()}`;

    const token = yield* resolveAuthToken;
    const res = yield* http.request<LogsResponse>({ path, token });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }

    const events = res.items ?? [];
    if (events.length === 0) {
      yield* output.info("No events yet.");
      return;
    }

    yield* output.printSection("Event Stream");
    for (const evt of events) {
      const ts = formatTimestamp(evt.created_at);
      const summary = evt.response_text
        ? evt.response_text.slice(0, 80)
        : (evt.status ?? "");
      yield* output.info(
        `  ${ui.dim(ts)}  ${evt.actor ?? LITERAL}  ${summary}`,
      );
    }

    if (res.next_cursor) {
      yield* output.info(
        ui.dim(`  More: agentsfleet logs --cursor=${res.next_cursor}`),
      );
    }
  });
