// `agentsfleet schedule ...` — explicit hosted schedules for a Fleet.
//
// Human terminal output is concise tables/success lines. `--json` or a
// redirected stdout emits the API envelope or schedule object verbatim.

import { Effect, type Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  wsFleetSchedulePath,
  wsFleetScheduleSyncPath,
  wsFleetSchedulesPath,
} from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import {
  requireValidId,
  resolveAuthToken,
  resolveWorkspaceId,
  WORKSPACE_FLAG,
} from "./workspace-guards.ts";
import { decodeSchedule, decodeSchedules, scheduleRows, scheduleSummary } from "./fleet-schedule-read.ts";
import { isString } from "../lib/guards.ts";
import { HTTP_METHOD } from "../constants/http-method.ts";
const STATUS_ACTIVE = "active" as const;
const STATUS_PAUSED = "paused" as const;
const FIELD_SCHEDULE_ID = "schedule_id" as const;
const FIELD_FLEET_ID = "fleet_id" as const;
const FIELD_DESCRIPTION = "description" as const;
const FIELD_NEXT_RUN = "next_run_at" as const;
const FIELD_MESSAGE = "message" as const;
const FIELD_STATUS = "status" as const;
const DEFAULT_TIMEZONE = "UTC" as const;

const USAGE_CREATE =
  "usage: agentsfleet schedule create <fleet_id> --cron <expr> --message <text> [--timezone <tz>]";
const USAGE_LIST = "usage: agentsfleet schedule list <fleet_id>";
const USAGE_UPDATE =
  "usage: agentsfleet schedule update <fleet_id> <schedule_id> [--cron <expr>] [--message <text>] [--timezone <tz>] [--status active|paused]";
const USAGE_DELETE = "usage: agentsfleet schedule delete <fleet_id> <schedule_id>";
const USAGE_SYNC = "usage: agentsfleet schedule sync <fleet_id> <schedule_id>";
const USAGE_SHOW = "usage: agentsfleet schedule show <fleet_id> <schedule_id>";


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

const machineOutput = (output: Output, stdoutIsTty: boolean | undefined): boolean =>
  output.format !== OUTPUT_FORMAT.text || stdoutIsTty === false;

const printSchedule = (
  output: Output,
  body: unknown,
  stdoutIsTty: boolean | undefined,
  verb: string,
): Effect.Effect<void, CliError> => Effect.gen(function* () {
  const row = yield* decodeSchedule(body);
  yield* machineOutput(output, stdoutIsTty)
    ? output.printJson(body)
    : output.success(`${verb} ${row.schedule_id}: ${scheduleSummary(row)}`);
});

const scheduleContext = (
  flags: ScheduleCommonFlags,
): Effect.Effect<
  { wsId: string; token: Redacted.Redacted<string>; config: CliConfig; output: Output; http: HttpClient },
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    return {
      wsId: yield* resolveWorkspaceId(flags.workspaceId, WORKSPACE_FLAG),
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
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_CREATE);
    const cron = yield* requireText(flags.cron, "--cron <expr> is required", USAGE_CREATE);
    const message = yield* requireText(flags.message, "--message <text> is required", USAGE_CREATE);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<unknown>({
      path: wsFleetSchedulesPath(ctx.wsId, fleetId),
      method: HTTP_METHOD.post,
      token: ctx.token,
      body: { cron, timezone: flags.timezone ?? DEFAULT_TIMEZONE, message },
    });
    yield* printSchedule(ctx.output, row, flags.stdoutIsTty, "created");
  });

export const scheduleListEffectFromArgs = (
  fleetIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_LIST);
    const ctx = yield* scheduleContext(flags);
    const res = yield* ctx.http.request<unknown>({
      path: wsFleetSchedulesPath(ctx.wsId, fleetId),
      token: ctx.token,
    });
    const {schedules: items} = yield* decodeSchedules(res);
    if (machineOutput(ctx.output, flags.stdoutIsTty)) {
      yield* ctx.output.printJson(res);
      return;
    }
    if (items.length === 0) {
      yield* ctx.output.info("No schedules for this Fleet.");
      return;
    }
    yield* ctx.output.printEntityTable(
      {
        // A hosted schedule has no name — its identity is the identifier.
        id: { key: FIELD_SCHEDULE_ID, label: "SCHEDULE_ID" },
        domain: [
          { key: FIELD_DESCRIPTION, label: "SCHEDULE" },
          { key: FIELD_STATUS, label: "STATUS" },
          { key: FIELD_NEXT_RUN, label: "NEXT RUN" },
          { key: FIELD_MESSAGE, label: "MESSAGE" },
        ],
      },
      scheduleRows(items),
    );
  });

export const scheduleUpdateEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleUpdateFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_UPDATE);
    const scheduleId = yield* requireValidId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_UPDATE);
    const desiredStatus = yield* parseStatus(flags.status);
    const body = {
      ...(flags.cron !== undefined ? { cron: flags.cron } : {}),
      ...(flags.timezone !== undefined ? { timezone: flags.timezone } : {}),
      ...(flags.message !== undefined ? { message: flags.message } : {}),
      ...(desiredStatus !== undefined ? { paused: desiredStatus === STATUS_PAUSED } : {}),
    };
    if (Object.keys(body).length === 0) {
      return yield* Effect.fail(new ValidationError({ detail: "no schedule fields provided", suggestion: USAGE_UPDATE }));
    }
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<unknown>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      method: "PATCH",
      token: ctx.token,
      body,
    });
    yield* printSchedule(ctx.output, row, flags.stdoutIsTty, "updated");
  });

export const scheduleRmEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_DELETE);
    const scheduleId = yield* requireValidId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_DELETE);
    const ctx = yield* scheduleContext(flags);
    yield* ctx.http.request<unknown>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      method: "DELETE",
      token: ctx.token,
    });
    if (machineOutput(ctx.output, flags.stdoutIsTty)) {
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
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_SHOW);
    const scheduleId = yield* requireValidId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_SHOW);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<unknown>({
      path: wsFleetSchedulePath(ctx.wsId, fleetId, scheduleId),
      token: ctx.token,
    });
    yield* printSchedule(ctx.output, row, flags.stdoutIsTty, "schedule");
  });

export const scheduleSyncEffectFromArgs = (
  fleetIdRaw: string | undefined,
  scheduleIdRaw: string | undefined,
  flags: ScheduleCommonFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, USAGE_SYNC);
    const scheduleId = yield* requireValidId(scheduleIdRaw, FIELD_SCHEDULE_ID, USAGE_SYNC);
    const ctx = yield* scheduleContext(flags);
    const row = yield* ctx.http.request<unknown>({
      path: wsFleetScheduleSyncPath(ctx.wsId, fleetId, scheduleId),
      method: HTTP_METHOD.post,
      token: ctx.token,
    });
    yield* printSchedule(ctx.output, row, flags.stdoutIsTty, "synced");
  });
