// Fleet lifecycle, reusable native setup and the workspace vault.

import { Option } from "effect";
import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { OPT_TTY } from "../../constants/cli-flags.ts";
import {
  deleteEffectFromId,
  killEffectFromId,
  resumeEffectFromId,
  statusEffect,
  fleetShowEffectFromId,
  stopEffectFromId,
} from "../../commands/fleet.ts";
import { listEffectFromFlags } from "../../commands/fleet_list.ts";
import { libraryEffectFromOptions } from "../../commands/fleet_library.ts";
import { libraryListEffectFromOptions } from "../../commands/fleet_library_list.ts";
import { libraryDeleteEffectFromArgs } from "../../commands/fleet_library_delete.ts";
import { modelsEffectFromFlags } from "../../commands/models.ts";
import { logsEffectFromFlags } from "../../commands/fleet_logs.ts";
import { eventsEffectFromFlags } from "../../commands/fleet_events.ts";
import { steerEffectFromArgs } from "../../commands/fleet_steer.ts";
import {
  secretAddEffectFromFlags,
  secretDeleteEffectFromName,
  secretShowEffectFromName,
  secretUpdateEffectFromFlags,
} from "../../commands/fleet_secret.ts";
import { secretListEffect } from "../../commands/fleet_secret_list.ts";
import {
  actorFlag,
  apiKeyFlag,
  baseUrlFlag,
  cursorFlag,
  dataFlag,
  dataReplacementFlag,
  entryIdArgument,
  eventsLimitFlag,
  fleetFlag,
  fleetIdArgument,
  fleetIdOptionalArgument,
  listLimitFlag,
  libraryLimitFlag,
  messageArgument,
  modelFlag,
  providerFlag,
  secretNameArgument,
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
const UPDATE = "update" as const;
const SHOW = "show" as const;
const libraryPaging = { startingAfter: startingAfterFlag, limit: libraryLimitFlag };
const libraryListCommand = Command.make(LIST, libraryPaging).pipe(
  Command.withDescription("List this workspace's published Fleet libraries"),
  guardedHandler(({ startingAfter, limit }) => libraryListEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
);

const libraryDeleteCommand = Command.make(DELETE, {
  entryId: entryIdArgument,
}).pipe(
  Command.withDescription("Remove a published Fleet library owned by this workspace"),
  guardedHandler(({ entryId }) => libraryDeleteEffectFromArgs(entryId)),
);

export const libraryCommand = Command.make("library", libraryPaging).pipe(
  Command.withDescription("Browse this workspace's Fleet library gallery"),
  guardedHandler(({ startingAfter, limit }) => libraryEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
  Command.withSubcommands([libraryListCommand, libraryDeleteCommand]),
);

export const modelsCommand = Command.make("models", { provider: providerFlag }).pipe(
  Command.withDescription("List the model catalogue this server serves"),
  guardedHandler(({ provider }) => modelsEffectFromFlags({ provider: opt(provider) })),
);

const fleetShowCommand = Command.make(SHOW, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Show one fleet"),
  guardedHandler(({ fleetId }) => fleetShowEffectFromId(fleetId)),
);

export const fleetCommand = Command.make("fleet").pipe(
  Command.withDescription(
    "Inspect fleet setup. Improve it through agentsfleet chat --fleet <fleet_id>.\n" +
    "Lifecycle commands are listed by agentsfleet --help.",
  ),
  Command.withShortDescription("Fleet management subcommands"),
  Command.withSubcommands([fleetShowCommand]),
);

// ── lifecycle verbs ─────────────────────────────────────────────────

export const listCommand = Command.make(LIST, {
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

export const statusCommand = Command.make("status").pipe(
  Command.withDescription("Show status for every fleet in the active workspace"),
  guardedHandler(() => statusEffect),
);

export const stopCommand = Command.make("stop", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Halt the running session (resumable)"),
  guardedHandler(({ fleetId }) => stopEffectFromId(fleetId)),
);

export const resumeCommand = Command.make("resume", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Resume from stopped or auto-paused"),
  guardedHandler(({ fleetId }) => resumeEffectFromId(fleetId)),
);

export const killCommand = Command.make("kill", { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Mark terminal (irreversible)"),
  guardedHandler(({ fleetId }) => killEffectFromId(fleetId)),
);

export const deleteCommand = Command.make(DELETE, { fleetId: fleetIdArgument }).pipe(
  Command.withDescription("Hard-delete a killed fleet"),
  guardedHandler(({ fleetId }) => deleteEffectFromId(fleetId)),
);

export const logsCommand = Command.make("logs", {
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

export const eventsCommand = Command.make("events", {
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

export const steerCommand = Command.make("steer", {
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

// ── the vault ───────────────────────────────────────────────────────

// Two ways to supply the body: the generic `--data` blob, or the typed
// provider flags composing the same JSON object. `--base-url` is checked at
// parse time so a non-https endpoint costs no request; `--provider` cannot be,
// because its accepted set is whatever this server's catalogue serves.
const secretCreateCommand = Command.make("create", {
  name: secretNameArgument,
  data: dataFlag,
  provider: providerFlag,
  baseUrl: baseUrlFlag,
  apiKey: apiKeyFlag,
  model: modelFlag,
}).pipe(
  Command.withDescription("Store a secret JSON object"),
  guardedHandler(({ name, data, provider, baseUrl, apiKey, model }) =>
    secretAddEffectFromFlags({
      name,
      data: opt(data),
      provider: opt(provider),
      baseUrl: opt(baseUrl),
      apiKey: opt(apiKey),
      model: opt(model),
    }),
  ),
);

// Replaces the stored body in place: the name stays claimed for the whole
// call, so fleets that require it keep resolving. Delete-then-create also
// replaces a value, but leaves a window where the name does not exist.
const secretUpdateCommand = Command.make(UPDATE, {
  name: secretNameArgument,
  data: dataReplacementFlag,
  provider: providerFlag,
  baseUrl: baseUrlFlag,
  apiKey: apiKeyFlag,
  model: modelFlag,
}).pipe(
  Command.withDescription("Replace a secret's stored body without releasing the name"),
  guardedHandler(({ name, data, provider, baseUrl, apiKey, model }) =>
    secretUpdateEffectFromFlags({
      name,
      data: opt(data),
      provider: opt(provider),
      baseUrl: opt(baseUrl),
      apiKey: opt(apiKey),
      model: opt(model),
    }),
  ),
);

const secretShowCommand = Command.make("show", { name: secretNameArgument }).pipe(
  Command.withDescription("Confirm a secret exists (never echoes secret bytes)"),
  guardedHandler(({ name }) => secretShowEffectFromName(name)),
);

const secretListCommand = Command.make(LIST).pipe(
  Command.withDescription("List secrets in the workspace vault"),
  guardedHandler(() => secretListEffect),
);

const secretDeleteCommand = Command.make(DELETE, { name: secretNameArgument }).pipe(
  Command.withDescription("Delete a secret from the workspace vault"),
  guardedHandler(({ name }) => secretDeleteEffectFromName(name)),
);

export const secretCommand = Command.make("secret").pipe(
  Command.withDescription("Workspace secret vault"),
  Command.withSubcommands([
    secretCreateCommand,
    secretUpdateCommand,
    secretShowCommand,
    secretListCommand,
    secretDeleteCommand,
  ]),
);
