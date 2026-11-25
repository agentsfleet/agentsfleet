// The fleet group: every verb that starts, reads, steers, changes or ends one
// fleet, and the fleet-scoped resources (triggers, approvals, schedules,
// memory, grants) nested under it.
//
// `fleet create` starts a fleet from its first message and returns at once;
// `fleet steer` talks to it, a prompt when no message is given.

import { Argument, Command, Flag } from "effect/cli";
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
import { triggerDeleteEffect, triggersListEffect } from "../../commands/fleet/triggers.ts";
import { fleetModelUseEffect, fleetNetworkUseEffect, NETWORK_MODES } from "../../commands/fleet/settings.ts";
import { approvalsCommand, grantsCommand } from "./access.command.ts";
import { scheduleCommand } from "./schedule.command.ts";
import { memoryCommand } from "./memory.command.ts";
import { actorFlag, cursorFlag, fleetFlag, fleetIdArgument, fleetIdOptionalArgument, fleetIdOrNameArgument, fleetModelFlag, fleetNameFlag, listLimitFlag, messageArgument, modelRefArgument, opt, optNum, sinceFlag, startingAfterFlag, triggerIdArgument, VERB, workspaceFlag } from "./flags.ts";

// The verb that picks a setting, as `models use` picks the tenant's provider.
const USE = "use" as const;

const createCommand = Command.make(VERB.create, {
  message: messageArgument,
  name: fleetNameFlag,
  model: fleetModelFlag,
}).pipe(
  Command.withDescription("Start a fleet with your first message"),
  guardedHandler(({ message, name, model }) =>
    fleetCreateEffect(opt(message), { name: opt(name), model: opt(model) }),
  ),
);

const fleetShowCommand = Command.make(VERB.show, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Show one fleet's details"),
  guardedHandler(({ fleetId }) => fleetShowEffectFromId(fleetId)),
);

// ── lifecycle verbs ─────────────────────────────────────────────────

const listCommand = Command.make(VERB.list, {
  workspace: workspaceFlag,
  startingAfter: startingAfterFlag,
  limit: listLimitFlag,
}).pipe(
  Command.withDescription("List the fleets in this workspace"),
  guardedHandler(({ workspace: workspaceId, startingAfter, limit }) =>
    listEffectFromFlags({
      workspaceId: opt(workspaceId),
      startingAfter: opt(startingAfter),
      limit: optNum(limit),
    }),
  ),
);

const statusCommand = Command.make("status").pipe(
  Command.withDescription("Show each fleet's state, spending and requests waiting for you"),
  guardedHandler(() => statusEffect),
);

const archiveCommand = Command.make("archive", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Stop a fleet but keep its chat history (can't be undone)"),
  guardedHandler(({ fleetId }) => archiveEffectFromId(fleetId)),
);

const deleteCommand = Command.make(VERB.delete, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Delete a fleet and all its history (can't be undone)"),
  guardedHandler(({ fleetId }) => deleteEffectFromId(fleetId)),
);

const logsCommand = Command.make("logs", {
  fleetId: fleetIdOptionalArgument,
  fleet: fleetFlag,
  limit: listLimitFlag,
  cursor: cursorFlag,
}).pipe(
  Command.withDescription("Show a fleet's latest activity, a page at a time"),
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
  Command.withDescription("Show a fleet's past messages and replies"),
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
    Flag.withDescription("Open the chat prompt even when this is not a terminal"),
    Flag.withHidden,
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("Talk to a fleet: send one message, or chat if you give none"),
  guardedHandler((config) =>
    steerEffectFromArgs(config.fleetId, opt(config.message), {
      forceTty: config[OPT_TTY],
    }),
  ),
);

// ── what wakes a fleet, and what a person changes ───────────────────

const triggersCommand = () => Command.make("triggers").pipe(
  Command.withDescription("See and delete what can wake a fleet (webhooks, GitHub, Slack)"),
  Command.withSubcommands([
    Command.make(VERB.list, { fleetId: fleetIdOrNameArgument }).pipe(
      Command.withDescription("List what can wake a fleet"),
      guardedHandler(({ fleetId }) => triggersListEffect(fleetId)),
    ),
    Command.make(VERB.delete, { fleetId: fleetIdOrNameArgument, triggerId: triggerIdArgument }).pipe(
      Command.withDescription("Delete a trigger so it no longer wakes the fleet"),
      guardedHandler(({ fleetId, triggerId }) => triggerDeleteEffect(fleetId, triggerId)),
    ),
  ]),
);

const modelCommand = () => Command.make("model").pipe(
  Command.withDescription("Choose the AI model a fleet uses"),
  Command.withSubcommands([
    Command.make(USE, { fleetId: fleetIdOrNameArgument, modelRef: modelRefArgument }).pipe(
      Command.withDescription("Use this model for the fleet's next replies"),
      guardedHandler(({ fleetId, modelRef }) => fleetModelUseEffect(fleetId, modelRef)),
    ),
  ]),
);

// One mode word, so the closed setting is as easy to name as the open one.
// `--packages` keeps the library's `--no-packages` negation.
const networkCommand = () => Command.make("network").pipe(
  Command.withDescription("Let a fleet reach any website, or keep it private"),
  Command.withSubcommands([
    Command.make(USE, {
      fleetId: fleetIdOrNameArgument,
      mode: Argument.Literals("mode", NETWORK_MODES).pipe(
        Argument.withDescription("allow-all: any website; private: only the sites of secrets you approved for it")),
      packages: Flag.Boolean("packages").pipe(
        Flag.withDescription("Let it install packages (npm, PyPI and the like); --no-packages blocks them"), Flag.optional),
    }).pipe(
      Command.withDescription("Set a fleet's network to allow-all or private"),
      guardedHandler(({ fleetId, mode, packages }) => fleetNetworkUseEffect(fleetId, { mode, packages: opt(packages) })),
    ),
  ]),
);

export const fleetCommand = () => Command.make("fleet").pipe(
  Command.withDescription(
    // The library indents only a description's first line, so the second
    // carries its own two spaces to sit under it.
    "Start fleets, talk to them and see what they do.\n" +
    '  Start one with: agentsfleet fleet create "<message>".',
  ),
  Command.withShortDescription("Start fleets, talk to them and see what they do"),
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
