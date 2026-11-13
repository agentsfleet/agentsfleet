// The fleet group: every verb that reads, steers or ends one fleet, and the
// fleet-scoped resources (approvals, schedules, memory, grants) nested under it.
//
// `chat --fleet <fleet_id>` improves a fleet's setup; `fleet steer` sends it a
// runtime instruction. The two never stand in for each other.

import { Option } from "effect";
import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { OPT_TTY } from "../../constants/cli-flags.ts";
import {
  activateEffectFromId,
  archiveEffectFromId,
  deleteEffectFromId,
  statusEffect,
  fleetShowEffectFromId,
} from "../../commands/fleet/fleet.ts";
import { listEffectFromFlags } from "../../commands/fleet/list.ts";
import { logsEffectFromFlags } from "../../commands/fleet/logs.ts";
import { eventsEffectFromFlags } from "../../commands/fleet/events.ts";
import { steerEffectFromArgs } from "../../commands/fleet/steer.ts";
import { approvalsCommand, grantsCommand } from "./access.command.ts";
import { scheduleCommand } from "./schedule.command.ts";
import { memoryCommand } from "./memory.command.ts";
import {
  actorFlag,
  cursorFlag,
  eventsLimitFlag,
  fleetFlag,
  fleetIdArgument,
  fleetIdOptionalArgument,
  listLimitFlag,
  messageArgument,
  sinceFlag,
  startingAfterFlag,
  workspaceFlag,
} from "./flags.ts";

const opt = Option.getOrUndefined;

// The command Effects declare a page size as a string, which is the spelling
// the wire uses. The flag has already refused anything outside the bound, so
// this only converts back.
const optNum = (value: Option.Option<number>): string | undefined =>
  Option.match(value, { onNone: () => undefined, onSome: (n) => String(n) });

const LIST = "list" as const;
const DELETE = "delete" as const;
const SHOW = "show" as const;

const fleetShowCommand = Command.make(SHOW, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Show one fleet"),
  guardedHandler(({ fleetId }) => fleetShowEffectFromId(fleetId)),
);

// ── lifecycle verbs ─────────────────────────────────────────────────

const listCommand = Command.make(LIST, {
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

const activateCommand = Command.make("activate", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Re-check setup and activate once every check passes"),
  guardedHandler(({ fleetId }) => activateEffectFromId(fleetId)),
);

const archiveCommand = Command.make("archive", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Archive: cancel its work and keep its history (irreversible)"),
  guardedHandler(({ fleetId }) => archiveEffectFromId(fleetId)),
);

const deleteCommand = Command.make(DELETE, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Delete the fleet in any status (irreversible)"),
  guardedHandler(({ fleetId }) => deleteEffectFromId(fleetId)),
);

const logsCommand = Command.make("logs", {
  fleetId: fleetIdOptionalArgument,
  fleet: fleetFlag,
  limit: eventsLimitFlag,
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

const eventsCommand = Command.make("events", {
  fleetId: fleetIdArgument,
  actor: actorFlag,
  since: sinceFlag,
  cursor: cursorFlag,
  limit: eventsLimitFlag,
}).pipe(
  Command.withDescription("Page through historical events"),
  guardedHandler(({ fleetId, actor, since, cursor, limit }) =>
    eventsEffectFromFlags({
      fleetId,
      actor: opt(actor),
      since: opt(since),
      cursor: opt(cursor),
      limit: optNum(limit),
    }),
  ),
);

const steerCommand = Command.make("steer", {
  fleetId: fleetIdArgument,
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

export const fleetCommand = () => Command.make("fleet").pipe(
  Command.withDescription(
    // The library indents only a description's first line, so the second
    // carries its own two spaces to sit under it.
    "Run, steer and inspect fleets.\n" +
    "  Improve a fleet's setup through agentsfleet chat --fleet <fleet_id>.",
  ),
  Command.withShortDescription("Run, steer and inspect fleets"),
  Command.withSubcommands([
    listCommand,
    fleetShowCommand,
    statusCommand,
    activateCommand,
    archiveCommand,
    deleteCommand,
    steerCommand,
    logsCommand,
    eventsCommand,
    approvalsCommand(),
    scheduleCommand(),
    memoryCommand(),
    grantsCommand(),
  ]),
);
