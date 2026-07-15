// Schedule group handler-binding — extracted from handlers-bind.ts to keep the
// central binder below the file length gate.

import type { ActionFrame, ScheduleHandlers } from "./cli-tree-types.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import {
  scheduleAddEffectFromArgs,
  scheduleListEffectFromArgs,
  scheduleRmEffectFromArgs,
  scheduleSyncEffectFromArgs,
  scheduleStatusEffectFromArgs,
  scheduleUpdateEffectFromArgs,
  type ScheduleAddFlags,
  type ScheduleUpdateFlags,
} from "../commands/fleet_schedule.ts";
import type { WrapEFn } from "./handlers-bind-fleet.ts";

const commonFlags = (frame: ActionFrame, stdoutIsTty: boolean) => ({
  workspaceId: optString(frame.parsed.options, "workspace"),
  stdoutIsTty,
});

const addFlags = (frame: ActionFrame, stdoutIsTty: boolean): ScheduleAddFlags => ({
  ...commonFlags(frame, stdoutIsTty),
  cron: optString(frame.parsed.options, FIELD_CRON),
  timezone: optString(frame.parsed.options, FIELD_TIMEZONE),
  message: optString(frame.parsed.options, FIELD_MESSAGE),
});

const updateFlags = (frame: ActionFrame, stdoutIsTty: boolean): ScheduleUpdateFlags => ({
  ...commonFlags(frame, stdoutIsTty),
  cron: optString(frame.parsed.options, FIELD_CRON),
  timezone: optString(frame.parsed.options, FIELD_TIMEZONE),
  message: optString(frame.parsed.options, FIELD_MESSAGE),
  status: optString(frame.parsed.options, "status"),
});

export const buildScheduleHandlers = (
  wrapEFn: WrapEFn,
  stdoutIsTty: () => boolean,
): ScheduleHandlers => ({
  add: wrapEFn("schedule.add", (frame) =>
    scheduleAddEffectFromArgs(frame.parsed.positionals[0], addFlags(frame, stdoutIsTty())),
  ),
  list: wrapEFn("schedule.list", (frame) =>
    scheduleListEffectFromArgs(frame.parsed.positionals[0], commonFlags(frame, stdoutIsTty())),
  ),
  update: wrapEFn("schedule.update", (frame) =>
    scheduleUpdateEffectFromArgs(
      frame.parsed.positionals[0],
      frame.parsed.positionals[1],
      updateFlags(frame, stdoutIsTty()),
    ),
  ),
  rm: wrapEFn("schedule.rm", (frame) =>
    scheduleRmEffectFromArgs(
      frame.parsed.positionals[0],
      frame.parsed.positionals[1],
      commonFlags(frame, stdoutIsTty()),
    ),
  ),
  status: wrapEFn("schedule.status", (frame) =>
    scheduleStatusEffectFromArgs(
      frame.parsed.positionals[0],
      frame.parsed.positionals[1],
      commonFlags(frame, stdoutIsTty()),
    ),
  ),
  sync: wrapEFn("schedule.sync", (frame) =>
    scheduleSyncEffectFromArgs(
      frame.parsed.positionals[0],
      frame.parsed.positionals[1],
      commonFlags(frame, stdoutIsTty()),
    ),
  ),
});

const FIELD_CRON = "cron" as const;
const FIELD_TIMEZONE = "timezone" as const;
const FIELD_MESSAGE = "message" as const;
