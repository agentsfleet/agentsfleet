// The fleet group: every verb that starts, reads, steers, changes or ends one
// fleet, and the fleet-scoped resources (triggers, approvals, schedules,
// memory, grants) nested under it.
//
// `fleet create` and `chat` both start a fleet from its first message; `chat`
// keeps talking, `fleet create` returns at once.

import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { OPT_TTY } from "../../constants/cli-flags.ts";
import {
  archiveEffectFromId,
  deleteEffectFromId,
  statusEffect,
  fleetShowEffectFromId,
} from "../../commands/fleet/fleet.ts";
import { fleetCreateEffect } from "../../commands/fleet/create.ts";
import { listEffectFromFlags } from "../../commands/fleet/list.ts";
import { logsEffectFromFlags } from "../../commands/fleet/logs.ts";
import { turnsEffectFromFlags } from "../../commands/fleet/turns.ts";
import { steerEffectFromArgs } from "../../commands/fleet/steer.ts";
import { triggerRemoveEffect, triggersListEffect } from "../../commands/fleet/triggers.ts";
import { fleetModelSetEffect, fleetNetworkSetEffect } from "../../commands/fleet/settings.ts";
import { approvalsCommand, grantsCommand } from "./access.command.ts";
import { scheduleCommand } from "./schedule.command.ts";
import { memoryCommand } from "./memory.command.ts";
import { actorFlag, cursorFlag, fleetFlag, fleetIdArgument, fleetIdOptionalArgument, fleetIdOrNameArgument, fleetModelFlag, fleetNameFlag, listLimitFlag, messageArgument, modelRefArgument, opt, optNum, sinceFlag, startingAfterFlag, triggerIdArgument, VERB, workspaceFlag } from "./flags.ts";

const SET = "set" as const;

const createCommand = Command.make(VERB.create, {
  message: messageArgument,
  name: fleetNameFlag,
  model: fleetModelFlag,
}).pipe(
  Command.withDescription("Start a fleet from its first message"),
  guardedHandler(({ message, name, model }) =>
    fleetCreateEffect(opt(message), { name: opt(name), model: opt(model) }),
  ),
);

const fleetShowCommand = Command.make(VERB.show, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Show one fleet"),
  guardedHandler(({ fleetId }) => fleetShowEffectFromId(fleetId)),
);

// ── lifecycle verbs ─────────────────────────────────────────────────

const listCommand = Command.make(VERB.list, {
  workspace: workspaceFlag,
  startingAfter: startingAfterFlag,
  limit: listLimitFlag,
}).pipe(
  Command.withDescription("List fleets in the active workspace (paginated)"),
  guardedHandler(({ workspace: workspaceId, startingAfter, limit }) =>
    listEffectFromFlags({
      workspaceId: opt(workspaceId),
      startingAfter: opt(startingAfter),
      limit: optNum(limit),
    }),
  ),
);

const statusCommand = Command.make("status").pipe(
  Command.withDescription("Show status for every fleet in the active workspace"),
  guardedHandler(() => statusEffect),
);

const archiveCommand = Command.make("archive", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Archive: cancel its work and keep its history (irreversible)"),
  guardedHandler(({ fleetId }) => archiveEffectFromId(fleetId)),
);

const deleteCommand = Command.make(VERB.delete, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Delete the fleet in any status (irreversible)"),
  guardedHandler(({ fleetId }) => deleteEffectFromId(fleetId)),
);

const logsCommand = Command.make("logs", {
  fleetId: fleetIdOptionalArgument,
  fleet: fleetFlag,
  limit: listLimitFlag,
  cursor: cursorFlag,
}).pipe(
  Command.withDescription("Tail fleet activity"),
  guardedHandler(({ fleetId, fleet, limit, cursor }) =>
    logsEffectFromFlags({
      fleetId: opt(fleet) ?? opt(fleetId),
      limit: optNum(limit),
      cursor: opt(cursor),
    }),
  ),
);

const turnsCommand = Command.make("turns", {
  fleetId: fleetIdArgument,
  actor: actorFlag,
  since: sinceFlag,
  cursor: cursorFlag,
  limit: listLimitFlag,
}).pipe(
  Command.withDescription("Page through historical turns"),
  guardedHandler(({ fleetId, actor, since, cursor, limit }) =>
    turnsEffectFromFlags({
      fleetId,
      actor: opt(actor),
      since: opt(since),
      cursor: opt(cursor),
      limit: optNum(limit),
    }),
  ),
);

const steerCommand = Command.make("steer", {
  // An id or `@name`; the handler looks a name up before the stream opens.
  fleetId: fleetIdOrNameArgument,
  message: messageArgument,
  // Hidden, because it exists for the tests and for a terminal the runtime
  // misreports — not for anyone to type on purpose.
  [OPT_TTY]: Flag.Boolean(OPT_TTY).pipe(
    Flag.withDescription("Force terminal prompt mode for steer"),
    Flag.withHidden,
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("Send a message; stream the response"),
  guardedHandler((config) =>
    steerEffectFromArgs(config.fleetId, opt(config.message), {
      forceTty: config[OPT_TTY],
    }),
  ),
);

// ── what wakes a fleet, and what a person changes ───────────────────

const triggersCommand = () => Command.make("triggers").pipe(
  Command.withDescription("List and remove what wakes a fleet from outside"),
  Command.withSubcommands([
    Command.make(VERB.list, { fleetId: fleetIdOrNameArgument }).pipe(
      Command.withDescription("List a fleet's triggers"),
      guardedHandler(({ fleetId }) => triggersListEffect(fleetId)),
    ),
    Command.make("remove", { fleetId: fleetIdOrNameArgument, triggerId: triggerIdArgument }).pipe(
      Command.withDescription("Remove a trigger so it no longer wakes the fleet"),
      guardedHandler(({ fleetId, triggerId }) => triggerRemoveEffect(fleetId, triggerId)),
    ),
  ]),
);

const modelCommand = () => Command.make("model").pipe(
  Command.withDescription("Change the model a fleet runs on"),
  Command.withSubcommands([
    Command.make(SET, { fleetId: fleetIdOrNameArgument, ref: modelRefArgument }).pipe(
      Command.withDescription("Run the fleet's next turns on this model"),
      guardedHandler(({ fleetId, ref }) => fleetModelSetEffect(fleetId, ref)),
    ),
  ]),
);

// Two switches rather than one value, so the closed setting is as easy to name
// as the open one. `--packages` keeps the library's `--no-packages` negation.
const networkCommand = () => Command.make("network").pipe(
  Command.withDescription("Open or close a fleet's network"),
  Command.withSubcommands([
    Command.make(SET, {
      fleetId: fleetIdOrNameArgument,
      allowAll: Flag.Boolean("allow-all").pipe(
        Flag.withDescription("Let the fleet's HTTP tools reach any public host"),
        Flag.withoutNegation, Flag.withDefault(false)),
      private: Flag.Boolean("private").pipe(
        Flag.withDescription("Reach only the hosts of the fleet's approved secrets"),
        Flag.withoutNegation, Flag.withDefault(false)),
      packages: Flag.Boolean("packages").pipe(
        Flag.withDescription("Allow package registries; --no-packages blocks them"), Flag.optional),
    }).pipe(
      Command.withDescription("Set the fleet's network: --allow-all or --private, with or without packages"),
      guardedHandler(({ fleetId, allowAll, private: closed, packages }) =>
        fleetNetworkSetEffect(fleetId, { allowAll, private: closed, packages: opt(packages) })),
    ),
  ]),
);

export const fleetCommand = () => Command.make("fleet").pipe(
  Command.withDescription(
    // The library indents only a description's first line, so the second
    // carries its own two spaces to sit under it.
    "Start, steer and inspect fleets.\n" +
    '  Start one from its first message: agentsfleet fleet create "<message>".',
  ),
  Command.withShortDescription("Start, steer and inspect fleets"),
  Command.withSubcommands([
    createCommand,
    listCommand,
    fleetShowCommand,
    statusCommand,
    archiveCommand,
    deleteCommand,
    steerCommand,
    logsCommand,
    turnsCommand,
    triggersCommand(),
    modelCommand(),
    networkCommand(),
    approvalsCommand(),
    scheduleCommand(),
    memoryCommand(),
    grantsCommand(),
  ]),
);
