// `agentsfleet fleet logs` — paginated turn print for a specific fleet.
// Accepts `<fleet_id>` positional or `--fleet <id>` flag; `--limit`
// (default 20) and `--cursor` for pagination.

import { Effect } from "effect";
import { CliConfig } from "../../services/config.ts";
import { Credentials } from "../../services/credentials.ts";
import { HttpClient } from "../../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../../services/output.ts";
import { Workspaces } from "../../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { isString } from "../../lib/guards.ts";
import { wsFleetTurnsPath } from "../../lib/api-paths.ts";
import { validateRequiredId } from "../../lib/id.ts";
import { ui, EMPTY_CELL, instantCell } from "../../output/index.ts";
import {
  ValidationError,
  type CliError,
} from "../../errors/index.ts";

const LOGS_LISTED = "Fleet turn log" as const;
const DEFAULT_LOGS_LIMIT = "20";
// The detail says what is wrong; the suggestion says what to run. Before
// M171 both carried the same sentence, so the rendered failure repeated
// itself and told the operator nothing new on the second line.
const FLEET_REQUIRED = "--fleet <id> is required";
const USAGE =
  "usage: agentsfleet fleet logs --fleet <id> [--limit <n>] [--cursor <token>]";


interface TurnRow {
  readonly created_at?: string | null;
  readonly response_text?: string | null;
  readonly status?: string | null;
  readonly actor?: string | null;
}

interface LogsResponse {
  readonly items?: ReadonlyArray<TurnRow>;
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
        new ValidationError({ detail: FLEET_REQUIRED, suggestion: USAGE }),
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

export const logsEffectFromFlags = (
  flags: LogsEffectFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
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
    const path = `${wsFleetTurnsPath(wsId, fleetId)}?${qs.toString()}`;

    const token = yield* resolveAuthToken;
    const res = yield* http.request<LogsResponse>({ path, token });

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(LOGS_LISTED, { ...res });
      return;
    }

    const turns = res.items ?? [];
    if (turns.length === 0) {
      yield* output.info("No turns yet.");
      return;
    }

    yield* output.printSection("Turn Stream");
    for (const turn of turns) {
      const ts = instantCell(turn.created_at);
      const summary = turn.response_text
        ? turn.response_text.slice(0, 80)
        : (turn.status ?? "");
      yield* output.info(
        `  ${ui.dim(ts)}  ${turn.actor ?? EMPTY_CELL}  ${summary}`,
      );
    }

    if (res.next_cursor) {
      yield* output.info(
        ui.dim(`  More: agentsfleet fleet logs --cursor=${res.next_cursor}`),
      );
    }
  });
