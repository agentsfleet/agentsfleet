// Hosted Fleet schedules: cron expressions the daemon runs without this CLI.
//
// Every verb takes the Fleet as a positional and the workspace as a flag,
// because a schedule belongs to a Fleet and a Fleet belongs to a workspace —
// the argument order says the same thing the paths do.

import { Effect, Option } from "effect";
import { Command } from "effect/unstable/cli";
import { guardedHandler } from "./guarded-handler.ts";
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

// `schedule` spoke a private dialect: add where every other collection says
// create, rm where the rest says delete, and status where a single-resource
// read is called show. No alias is kept for the retired spellings — a
// compatibility verb is what the rules forbid at this version, and a retired
// spelling answers as the unknown subcommand it now is.
const CREATE = "create" as const;
const DELETE = "delete" as const;
const SHOW = "show" as const;
const LIST = "list" as const;
const UPDATE = "update" as const;
const SYNC = "sync" as const;

const scheduleCreateCommand = Command.make(CREATE, {
  fleetId: fleetIdArgument,
  cron: cronFlag,
  message: messageFlag,
  timezone: timezoneDefaultFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Create a hosted schedule for a Fleet"),
  guardedHandler(({ fleetId, cron, message, timezone, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleAddEffectFromArgs(  fleetId, {
        cron,
        message,
        timezone: opt(timezone),
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

const scheduleListCommand = Command.make(LIST, {
  fleetId: fleetIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("List hosted schedules for a Fleet"),
  guardedHandler(({ fleetId, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleListEffectFromArgs(  fleetId, {
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

const scheduleUpdateCommand = Command.make(UPDATE, {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  cron: cronOptionalFlag,
  message: messageOptionalFlag,
  timezone: timezoneFlag,
  status: scheduleStatusFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Update a hosted schedule"),
  guardedHandler(({ fleetId, scheduleId, cron, message, timezone, status, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleUpdateEffectFromArgs(  fleetId, scheduleId, {
        cron: opt(cron),
        message: opt(message),
        timezone: opt(timezone),
        status: opt(status),
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

const scheduleDeleteCommand = Command.make(DELETE, {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Delete a hosted schedule"),
  guardedHandler(({ fleetId, scheduleId, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleRmEffectFromArgs(  fleetId, scheduleId, {
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

const scheduleShowCommand = Command.make(SHOW, {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show one hosted schedule"),
  guardedHandler(({ fleetId, scheduleId, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleStatusEffectFromArgs(  fleetId, scheduleId, {
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

const scheduleSyncCommand = Command.make(SYNC, {
  fleetId: fleetIdArgument,
  scheduleId: scheduleIdArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Re-apply a hosted schedule"),
  guardedHandler(({ fleetId, scheduleId, workspace }) =>
    Effect.gen(function* () {
      const isTty = yield* stdoutIsTty;
      return yield* scheduleSyncEffectFromArgs(  fleetId, scheduleId, {
        workspaceId: opt(workspace),
        stdoutIsTty: isTty,
      });
    }),
  ),
);

export const scheduleCommand = Command.make("schedule").pipe(
  Command.withDescription("Manage hosted Fleet schedules"),
  Command.withSubcommands([
    scheduleCreateCommand,
    scheduleListCommand,
    scheduleUpdateCommand,
    scheduleDeleteCommand,
    scheduleShowCommand,
    scheduleSyncCommand,
  ]),
);
