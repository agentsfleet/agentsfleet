// `agentsfleet schedule ...` — explicit hosted schedules for a Fleet.
//
// Human terminal output is concise tables/success lines. `--json` or a
// redirected stdout emits the API envelope or schedule object verbatim.

import { Effect, type Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  wsFleetSchedulePath,
  wsFleetScheduleSyncPath,
  wsFleetSchedulesPath,
} from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
  type UnexpectedError,
} from "../errors/index.ts";
import { resolveAuthToken } from "./workspace-guards.ts";

const TYPE_STRING = "string" as const;
const STATUS_ACTIVE = "active" as const;
const STATUS_PAUSED = "paused" as const;
const FIELD_SCHEDULE_ID = "schedule_id" as const;
const FIELD_FLEET_ID = "fleet_id" as const;
const FIELD_CRON = "cron" as const;
const FIELD_TIMEZONE = "timezone" as const;
const FIELD_MESSAGE = "message" as const;
const FIELD_DESIRED_STATUS = "desired_status" as const;
const FIELD_SYNC_STATUS = "sync_status" as const;
const DEFAULT_TIMEZONE = "UTC" as const;
const METHOD_POST = "POST" as const;

const USAGE_ADD =
  "usage: agentsfleet schedule add <fleet_id> --cron <expr> --message <text> [--timezone <tz>]";
const USAGE_LIST = "usage: agentsfleet schedule list <fleet_id>";
const USAGE_UPDATE =
  "usage: agentsfleet schedule update <fleet_id> <schedule_id> [--cron <expr>] [--message <text>] [--timezone <tz>] [--status active|paused]";
const USAGE_RM = "usage: agentsfleet schedule rm <fleet_id> <schedule_id>";
const USAGE_SYNC = "usage: agentsfleet schedule sync <fleet_id> <schedule_id>";
const USAGE_STATUS = "usage: agentsfleet schedule status <fleet_id> <schedule_id>";

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface ScheduleRow {
  readonly schedule_id?: string | null;
  readonly fleet_id?: string | null;
  readonly source?: string | null;
  readonly cron?: string | null;
  readonly timezone?: string | null;
  readonly message?: string | null;
  readonly desired_status?: string | null;
  readonly sync_status?: string | null;
  readonly generation?: number | null;
  readonly last_error?: string | null;
  readonly created_at?: number | null;
  readonly updated_at?: number | null;
}

interface ScheduleListResponse {
  readonly items?: ReadonlyArray<ScheduleRow>;
  readonly total?: number;
  readonly next_cursor?: string | null;
}

interface ScheduleCommonFlags {
  readonly workspaceId?: string | undefined;
  readonly stdoutIsTty?: boolean | undefined;
}

export interface ScheduleAddFlags extends ScheduleCommonFlags {
  readonly cron?: string | undefined;
  readonly timezone?: string | undefined;
  readonly message?: string | undefined;
}

export interface ScheduleUpdateFlags extends ScheduleCommonFlags {
  readonly cron?: string | undefined;
  readonly timezone?: string | undefined;
  readonly message?: string | undefined;
  readonly status?: string | undefined;
}

const requireText = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  isString(value) && value.length > 0
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const requireId = (
  value: string | undefined,
  fieldName: string,
  usage: string,
): Effect.Effect<string, ValidationError> => {
  if (!isString(value) || value.length === 0) {
    return Effect.fail(new ValidationError({ detail: `${fieldName} is required`, suggestion: usage }));
  }
  const check = validateRequiredId(value, fieldName);
  if (!check.ok) {
    return Effect.fail(new ValidationError({ detail: check.message, suggestion: "pass a valid uuidv7" }));
  }
  return Effect.succeed(value);
};

const resolveWorkspace = (
  override: string | undefined,
): Effect.Effect<string, ConfigError | UnexpectedError | ValidationError, Workspaces> =>
  Effect.gen(function* () {
    if (isString(override) && override.length > 0) {
    return yield* requireId(override, "workspace_id", "pass --workspace <workspace_id>");
    }
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    if (!state.current_workspace_id) {
      return yield* Effect.fail(
        new ConfigError({
          detail: "no workspace selected",
          suggestion: "run `agentsfleet workspace use <id>` or pass --workspace <id>",
        }),
      );
    }
    return state.current_workspace_id;
  });

const parseStatus = (value: string | undefined): Effect.Effect<string | undefined, ValidationError> => {
  if (!isString(value) || value.length === 0) return Effect.succeed(undefined);
  if (value === STATUS_ACTIVE || value === STATUS_PAUSED) return Effect.succeed(value);
  return Effect.fail(
    new ValidationError({
      detail: "status must be active or paused",
      suggestion: USAGE_UPDATE,
    }),
  );
};

const machineOutput = (config: CliConfig, stdoutIsTty: boolean | undefined): boolean =>
  config.jsonMode || stdoutIsTty === false;

const scheduleIdOf = (row: ScheduleRow): string => row.schedule_id ?? "";

const printSchedule = (
  output: Output,
  config: CliConfig,
  row: ScheduleRow,
  stdoutIsTty: boolean | undefined,
  verb: string,
): Effect.Effect<void> =>
  machineOutput(config, stdoutIsTty)
    ? output.printJson(row)
    : output.success(
        `${verb} ${scheduleIdOf(row)} (${row.desired_status ?? "-"}, sync=${row.sync_status ?? "-"})`,
      );

const scheduleContext = (
  flags: ScheduleCommonFlags,
): Effect.Effect<
  { wsId: string; token: Redacted.Redacted<string>; config: CliConfig; output: Output; http: HttpClient },
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    return {
      wsId: yield* resolveWorkspace(flags.workspaceId),
      token: yield* resolveAuthToken,
      config: yield* CliConfig,
      output: yield* Output,
      http: yield* HttpClient,
    };
  });

export const scheduleAddEffectFromArgs = (
  fleetIdRaw: string | undefined,
  flags: ScheduleAddFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_ADD);
    const cron = yield* requireText(flags.cron, "--cron <expr> is required", USAGE_ADD);
    const message = yield* requireText(flags.message, "--message <text> is required", USAGE_ADD);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<ScheduleRow>({
      path: wsFleetSchedulesPath(ctx.wsId, fleetId),
      method: METHOD_POST,
      token: ctx.token,
      body: { cron, timezone: flags.timezone ?? DEFAULT_TIMEZONE, message },
    });
    yield* printSchedule(ctx.output, ctx.config, row, flags.stdoutIsTty, "created");
  });

export const scheduleListEffectFromArgs = (
  fleetIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_LIST);
    const ctx = yield* scheduleContext(flags);
    const res = yield* ctx.http.request<ScheduleListResponse>({
      path: wsFleetSchedulesPath(ctx.wsId, fleetId),
      token: ctx.token,
    });
    if (machineOutput(ctx.config, flags.stdoutIsTty)) {
      yield* ctx.output.printJson(res);
      return;
    }
    const items = Array.isArray(res.items) ? res.items : [];
    if (items.length === 0) {
      yield* ctx.output.info("No schedules for this Fleet.");
      return;
    }
    yield* ctx.output.printTable(
      [
        { key: FIELD_SCHEDULE_ID, label: "SCHEDULE_ID" },
        { key: FIELD_CRON, label: "CRON" },
        { key: FIELD_TIMEZONE, label: "TIMEZONE" },
        { key: FIELD_DESIRED_STATUS, label: "DESIRED" },
        { key: FIELD_SYNC_STATUS, label: "SYNC" },
        { key: FIELD_MESSAGE, label: "MESSAGE" },
      ],
      items.map((row) => ({
        [FIELD_SCHEDULE_ID]: row.schedule_id ?? "",
        [FIELD_CRON]: row.cron ?? "",
        [FIELD_TIMEZONE]: row.timezone ?? "",
        [FIELD_DESIRED_STATUS]: row.desired_status ?? "",
        [FIELD_SYNC_STATUS]: row.sync_status ?? "",
        [FIELD_MESSAGE]: row.message ?? "",
      })),
    );
  });

export const scheduleUpdateEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleUpdateFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_UPDATE);
    const scheduleId = yield* requireId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_UPDATE);
    const desiredStatus = yield* parseStatus(flags.status);
    const body = {
      ...(flags.cron !== undefined ? { cron: flags.cron } : {}),
      ...(flags.timezone !== undefined ? { timezone: flags.timezone } : {}),
      ...(flags.message !== undefined ? { message: flags.message } : {}),
      ...(desiredStatus !== undefined ? { desired_status: desiredStatus } : {}),
    };
    if (Object.keys(body).length === 0) {
      return yield* Effect.fail(new ValidationError({ detail: "no schedule fields provided", suggestion: USAGE_UPDATE }));
    }
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<ScheduleRow>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      method: "PATCH",
      token: ctx.token,
      body,
    });
    yield* printSchedule(ctx.output, ctx.config, row, flags.stdoutIsTty, "updated");
  });

export const scheduleRmEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_RM);
    const scheduleId = yield* requireId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_RM);
    const ctx = yield* scheduleContext(flags);
    yield* ctx.http.request<unknown>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      method: "DELETE",
      token: ctx.token,
    });
    if (machineOutput(ctx.config, flags.stdoutIsTty)) {
      yield* ctx.output.printJson({ deleted: true, schedule_id: scheduleId });
    } else {
      yield* ctx.output.success(`removed ${scheduleId}`);
    }
  });

export const scheduleStatusEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_STATUS);
    const scheduleId = yield* requireId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_STATUS);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<ScheduleRow>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      token: ctx.token,
    });
    yield* printSchedule(ctx.output, ctx.config, row, flags.stdoutIsTty, "schedule");
  });

export const scheduleSyncEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireId(fleetIdRaw, FIELD_FLEET_ID, USAGE_SYNC);
    const scheduleId = yield* requireId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_SYNC);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<ScheduleRow>({
      path: wsFleetScheduleSyncPath(ctx.wsId, fleetId, scheduleId),
      method: METHOD_POST,
      token: ctx.token,
    });
    yield* printSchedule(ctx.output, ctx.config, row, flags.stdoutIsTty, "synced");
  });
