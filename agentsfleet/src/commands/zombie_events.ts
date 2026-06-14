// `agentsfleet events <zombie_id>` — newest-first paginated history print.
// Filters: --actor (glob), --since (Go-style duration or RFC 3339),
// --cursor (opaque base64url from a prior `next_cursor`), --limit.
// Default: one line per event with ts + actor + status + preview.
// `--json` (or global jsonMode) emits the raw envelope for piping.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsZombieEventsPath } from "../lib/api-paths.ts";
import { EVENT_STATUS } from "../constants/event-status.ts";
import { ui, type UiTheme } from "../output/index.ts";
import {
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const DEFAULT_LIMIT = 50;
const PREVIEW_MAX = 80;
const TYPE_NUMBER = "number" as const;
const TYPE_STRING = "string" as const;
const LITERAL = "—" as const;

const isNumber = (value: unknown): value is number => typeof value === TYPE_NUMBER;
const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface EventRow {
  readonly created_at?: number | string | null;
  readonly response_text?: string | null;
  readonly status?: string | null;
  readonly actor?: string | null;
}

interface EventsResponse {
  readonly items?: ReadonlyArray<EventRow>;
  readonly next_cursor?: string | null;
}

export interface EventsEffectFlags {
  readonly zombieId?: string | undefined;
  readonly actor?: string | undefined;
  readonly since?: string | undefined;
  readonly cursor?: string | undefined;
  readonly limit?: string | undefined;
  readonly json?: boolean | undefined;
}

const buildQuery = (flags: EventsEffectFlags): string => {
  const qs = new URLSearchParams();
  const limit =
    isString(flags.limit) || isNumber(flags.limit)
      ? String(flags.limit)
      : String(DEFAULT_LIMIT);
  qs.set("limit", limit);
  if (isString(flags.actor) && flags.actor.length > 0) qs.set("actor", flags.actor);
  if (isString(flags.since) && flags.since.length > 0) qs.set("since", flags.since);
  if (isString(flags.cursor) && flags.cursor.length > 0) qs.set("cursor", flags.cursor);
  return qs.toString();
};

const renderStatus = (status: string | null | undefined, theme: UiTheme): string => {
  if (!status) return theme.dim(LITERAL);
  if (status === EVENT_STATUS.PROCESSED) return theme.ok(status);
  if (status === EVENT_STATUS.AGENT_ERROR) return theme.err(status);
  if (status === EVENT_STATUS.GATE_BLOCKED) return theme.warn(status);
  return theme.dim(status);
};

const previewText = (text: string | null | undefined): string => {
  if (!isString(text) || text.length === 0) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > PREVIEW_MAX ? `${oneline.slice(0, PREVIEW_MAX - 3)}…` : oneline;
};

const formatRow = (ev: EventRow): string => {
  const ts =
    isNumber(ev.created_at) && Number.isFinite(ev.created_at)
      ? new Date(ev.created_at).toISOString()
      : LITERAL;
  const status = renderStatus(ev.status, ui);
  const actor = ev.actor || "—";
  const preview = previewText(ev.response_text);
  return `  ${ui.dim(ts)}  ${actor}  ${status}  ${preview}`;
};

export const eventsEffectFromFlags = (
  flags: EventsEffectFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    if (!flags.zombieId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "zombie_id is required",
          suggestion:
            "usage: agentsfleet events <zombie_id> [--actor=glob] [--since=2h] [--cursor=...] [--limit=N] [--json]",
        }),
      );
    }

    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const path = `${wsZombieEventsPath(wsId, flags.zombieId)}?${buildQuery(flags)}`;
    const res = yield* http.request<EventsResponse>({ path, token });

    if (config.jsonMode || flags.json === true) {
      yield* output.printJson(res);
      return;
    }

    const items = res.items ?? [];
    if (items.length === 0) {
      yield* output.info("No events yet.");
      return;
    }

    yield* output.printSection("Events");
    for (const ev of items) {
      yield* output.info(formatRow(ev));
    }
    if (res.next_cursor) {
      yield* output.info(
        ui.dim(`  More: agentsfleet events ${flags.zombieId} --cursor=${res.next_cursor}`),
      );
    }
  });
