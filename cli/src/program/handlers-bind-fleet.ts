// Fleet group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap. Production routes through
// commander → these handlers → the Effect dispatcher (runEffect). Every
// fleet.* leaf is an Effect.Effect<void, CliError, R>.

import type { Effect } from "effect";
import type { ActionFrame, CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import type { MainLayerServices } from "../lib/run-effect.ts";
import type { CliError } from "../errors/index.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import { OPT_TTY } from "../constants/cli-flags.ts";
import {
  statusEffect,
  stopEffectFromId,
  resumeEffectFromId,
  killEffectFromId,
  deleteEffectFromId,
} from "../commands/fleet.ts";
import {
  installEffectFromFlags,
  updateEffectFromArgs,
} from "../commands/fleet_install.ts";
import { templatesEffect } from "../commands/fleet_templates.ts";
import { listEffectFromFlags } from "../commands/fleet_list.ts";
import { logsEffectFromFlags } from "../commands/fleet_logs.ts";
import { eventsEffectFromFlags } from "../commands/fleet_events.ts";
import { steerEffectFromArgs } from "../commands/fleet_steer.ts";
import {
  credentialAddEffectFromFlags,
  credentialShowEffectFromName,
  credentialListEffect,
  credentialDeleteEffectFromName,
} from "../commands/fleet_credential.ts";

export type WrapE = <E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export type WrapEFn = <E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export const buildFleetHandlers = (
  wrapE: WrapE,
  wrapEFn: WrapEFn,
): Handlers[typeof AGENT] => ({
  templates: wrapE("fleet.templates", templatesEffect),
  install: wrapEFn(
    "fleet.install",
    (frame) =>
      installEffectFromFlags({
        fromPath: optString(frame.parsed.options, FIELD_FROM),
        templateId: optString(frame.parsed.options, FIELD_TEMPLATE),
        name: optString(frame.parsed.options, FIELD_NAME),
      }),
  ),
  update: wrapEFn(
    "fleet.update",
    (frame) =>
      updateEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_FROM),
      ),
  ),
  list: wrapEFn(
    "fleet.list",
    (frame) =>
      listEffectFromFlags({
        workspaceId:
          optString(frame.parsed.options, "workspace-id") ??
          optString(frame.parsed.options, "workspaceId"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  status: wrapE("fleet.status", statusEffect),
  stop: wrapEFn(
    "fleet.stop",
    (frame) => stopEffectFromId(frame.parsed.positionals[0]),
  ),
  resume: wrapEFn(
    "fleet.resume",
    (frame) => resumeEffectFromId(frame.parsed.positionals[0]),
  ),
  kill: wrapEFn(
    "fleet.kill",
    (frame) => killEffectFromId(frame.parsed.positionals[0]),
  ),
  delete: wrapEFn(
    "fleet.delete",
    (frame) => deleteEffectFromId(frame.parsed.positionals[0]),
  ),
  logs: wrapEFn(
    "fleet.logs",
    (frame) =>
      logsEffectFromFlags({
        fleetId:
          optString(frame.parsed.options, AGENT) ??
          frame.parsed.positionals[0],
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  events: wrapEFn(
    "fleet.events",
    (frame) =>
      eventsEffectFromFlags({
        fleetId: frame.parsed.positionals[0],
        actor: optString(frame.parsed.options, "actor"),
        since: optString(frame.parsed.options, "since"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
        json: frame.parsed.options["json"] === true,
      }),
  ),
  steer: wrapEFn(
    "fleet.steer",
    (frame) =>
      steerEffectFromArgs(
        frame.parsed.positionals[0],
        frame.parsed.positionals[1],
        { forceTty: frame.parsed.options[OPT_TTY] === true },
      ),
  ),
  credential: {
    add: wrapEFn(
      "fleet.credential.add",
      (frame) =>
        credentialAddEffectFromFlags({
          name: frame.parsed.positionals[0],
          data: optString(frame.parsed.options, "data"),
          force: frame.parsed.options["force"] === true,
        }),
    ),
    show: wrapEFn(
      "fleet.credential.show",
      (frame) => credentialShowEffectFromName(frame.parsed.positionals[0]),
    ),
    list: wrapE("fleet.credential.list", credentialListEffect),
    delete: wrapEFn(
      "fleet.credential.delete",
      (frame) => credentialDeleteEffectFromName(frame.parsed.positionals[0]),
    ),
  },
});
const FIELD_CURSOR = "cursor" as const;
const FIELD_FROM = "from" as const;
const FIELD_TEMPLATE = "template" as const;
const FIELD_NAME = "name" as const;
const FIELD_LIMIT = "limit" as const;
const AGENT = "fleet" as const;
