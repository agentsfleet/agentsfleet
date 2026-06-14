// Zombie group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap. Production routes through
// commander → these handlers → the Effect dispatcher (runEffect). Every
// zombie.* leaf is an Effect.Effect<void, CliError, R>.

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
} from "../commands/zombie.ts";
import {
  installEffectFromFlags,
  updateEffectFromArgs,
} from "../commands/zombie_install.ts";
import { listEffectFromFlags } from "../commands/zombie_list.ts";
import { logsEffectFromFlags } from "../commands/zombie_logs.ts";
import { eventsEffectFromFlags } from "../commands/zombie_events.ts";
import { steerEffectFromArgs } from "../commands/zombie_steer.ts";
import {
  credentialAddEffectFromFlags,
  credentialShowEffectFromName,
  credentialListEffect,
  credentialDeleteEffectFromName,
} from "../commands/zombie_credential.ts";

export type WrapE = <E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export type WrapEFn = <E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export const buildZombieHandlers = (
  wrapE: WrapE,
  wrapEFn: WrapEFn,
): Handlers[typeof ZOMBIE] => ({
  install: wrapEFn(
    "zombie.install",
    (frame) => installEffectFromFlags(optString(frame.parsed.options, FIELD_FROM)),
  ),
  update: wrapEFn(
    "zombie.update",
    (frame) =>
      updateEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_FROM),
      ),
  ),
  list: wrapEFn(
    "zombie.list",
    (frame) =>
      listEffectFromFlags({
        workspaceId:
          optString(frame.parsed.options, "workspace-id") ??
          optString(frame.parsed.options, "workspaceId"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  status: wrapE("zombie.status", statusEffect),
  stop: wrapEFn(
    "zombie.stop",
    (frame) => stopEffectFromId(frame.parsed.positionals[0]),
  ),
  resume: wrapEFn(
    "zombie.resume",
    (frame) => resumeEffectFromId(frame.parsed.positionals[0]),
  ),
  kill: wrapEFn(
    "zombie.kill",
    (frame) => killEffectFromId(frame.parsed.positionals[0]),
  ),
  delete: wrapEFn(
    "zombie.delete",
    (frame) => deleteEffectFromId(frame.parsed.positionals[0]),
  ),
  logs: wrapEFn(
    "zombie.logs",
    (frame) =>
      logsEffectFromFlags({
        zombieId:
          optString(frame.parsed.options, ZOMBIE) ??
          frame.parsed.positionals[0],
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  events: wrapEFn(
    "zombie.events",
    (frame) =>
      eventsEffectFromFlags({
        zombieId: frame.parsed.positionals[0],
        actor: optString(frame.parsed.options, "actor"),
        since: optString(frame.parsed.options, "since"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
        json: frame.parsed.options["json"] === true,
      }),
  ),
  steer: wrapEFn(
    "zombie.steer",
    (frame) =>
      steerEffectFromArgs(
        frame.parsed.positionals[0],
        frame.parsed.positionals[1],
        { forceTty: frame.parsed.options[OPT_TTY] === true },
      ),
  ),
  credential: {
    add: wrapEFn(
      "zombie.credential.add",
      (frame) =>
        credentialAddEffectFromFlags({
          name: frame.parsed.positionals[0],
          data: optString(frame.parsed.options, "data"),
          force: frame.parsed.options["force"] === true,
        }),
    ),
    show: wrapEFn(
      "zombie.credential.show",
      (frame) => credentialShowEffectFromName(frame.parsed.positionals[0]),
    ),
    list: wrapE("zombie.credential.list", credentialListEffect),
    delete: wrapEFn(
      "zombie.credential.delete",
      (frame) => credentialDeleteEffectFromName(frame.parsed.positionals[0]),
    ),
  },
});
const FIELD_CURSOR = "cursor" as const;
const FIELD_FROM = "from" as const;
const FIELD_LIMIT = "limit" as const;
const ZOMBIE = "zombie" as const;
