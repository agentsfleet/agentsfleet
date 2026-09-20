// Hosted Fleet schedules: cron expressions the daemon runs without this CLI.
//
// Every verb takes the Fleet as a positional and the workspace as a flag,
// because a schedule belongs to a Fleet and a Fleet belongs to a workspace —
// the argument order says the same thing the paths do.

import { Option } from "effect";
import { Command } from "effect/unstable/cli";
import {
  scheduleAddEffectFromArgs,
  scheduleListEffectFromArgs,
  scheduleRmEffectFromArgs,
  scheduleStatusEffectFromArgs,
  scheduleSyncEffectFromArgs,
  scheduleUpdateEffectFromArgs,
} from "../../commands/fleet_schedule.ts";
import {
  cronFlag,
  cronOptionalFlag,
  fleetIdArgument,
  messageFlag,
  messageOptionalFlag,
  scheduleIdArgument,
  scheduleStatusFlag,
  timezoneDefaultFlag,
  timezoneFlag,
  workspaceFlag,
} from "./flags.ts";
import { stdoutIsTty } from "./tty.ts";

const opt = Option.getOrUndefined;

const scheduleAddCommand = Command.make("add", {
  fleetId: fleetIdArgument,
  cron: cronFlag,
  message: messageFlag,
  timezone: timezoneDefaultFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Create a hosted schedule for a Fleet"),
  Command.withHandler(({ fleetId, cron, message, timezone, workspace }) =>
    scheduleAddEffectFromArgs(fleetId, {
      cron,
      message,
      timezone: opt(timezone),
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const scheduleListCommand = Command.make("list", {
  fleetId: fleetIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("List hosted schedules for a Fleet"),
  Command.withHandler(({ fleetId, workspace }) =>
    scheduleListEffectFromArgs(fleetId, {
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const scheduleUpdateCommand = Command.make("update", {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  cron: cronOptionalFlag,
  message: messageOptionalFlag,
  timezone: timezoneFlag,
  status: scheduleStatusFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Update a hosted schedule"),
  Command.withHandler(({ fleetId, scheduleId, cron, message, timezone, status, workspace }) =>
    scheduleUpdateEffectFromArgs(fleetId, scheduleId, {
      cron: opt(cron),
      message: opt(message),
      timezone: opt(timezone),
      status: opt(status),
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const scheduleRmCommand = Command.make("rm", {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Remove a hosted schedule"),
  Command.withHandler(({ fleetId, scheduleId, workspace }) =>
    scheduleRmEffectFromArgs(fleetId, scheduleId, {
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const scheduleStatusCommand = Command.make("status", {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show one hosted schedule"),
  Command.withHandler(({ fleetId, scheduleId, workspace }) =>
    scheduleStatusEffectFromArgs(fleetId, scheduleId, {
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const scheduleSyncCommand = Command.make("sync", {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Re-apply a hosted schedule to QStash"),
  Command.withHandler(({ fleetId, scheduleId, workspace }) =>
    scheduleSyncEffectFromArgs(fleetId, scheduleId, {
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

export const scheduleCommand = Command.make("schedule").pipe(
  Command.withDescription("Manage hosted Fleet schedules"),
  Command.withSubcommands([
    scheduleAddCommand,
    scheduleListCommand,
    scheduleUpdateCommand,
    scheduleRmCommand,
    scheduleStatusCommand,
    scheduleSyncCommand,
  ]),
);
