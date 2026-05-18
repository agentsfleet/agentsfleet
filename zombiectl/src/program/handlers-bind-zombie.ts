// Zombie group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap. Production routes through
// commander → these handlers → the Effect dispatcher (runEffect). Every
// zombie.* leaf is an Effect.Effect<void, CliError, R>.

import type { Effect } from "effect";
import type { CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import type { MainLayerServices } from "../lib/run-effect.ts";
import type { CliError } from "../errors/index.ts";
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

// Commander parsers like `parseIntOption` return numbers, others return
// strings; this reader normalises both to a string for downstream
// query-string + flag plumbing. Empty-string + non-finite-number
// produce undefined so callers can `??` cleanly.
const optString = (
  options: Record<string, unknown>,
  key: string,
): string | undefined => {
  const v = options[key];
  if (typeof v === "string" && v.length > 0) return v;
  if (typeof v === "number" && Number.isFinite(v)) return String(v);
  return undefined;
};

export type WrapE = <E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export type WrapEFn = <E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: import("./cli-tree-types.ts").ActionFrame) => Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export const buildZombieHandlers = (
  wrapE: WrapE,
  wrapEFn: WrapEFn,
): Handlers["zombie"] => ({
  install: wrapEFn(
    "zombie.install",
    (frame) => installEffectFromFlags(optString(frame.parsed.options, "from")),
  ),
  update: wrapEFn(
    "zombie.update",
    (frame) =>
      updateEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, "from"),
      ),
  ),
  list: wrapEFn(
    "zombie.list",
    (frame) =>
      listEffectFromFlags({
        workspaceId:
          optString(frame.parsed.options, "workspace-id") ??
          optString(frame.parsed.options, "workspaceId"),
        cursor: optString(frame.parsed.options, "cursor"),
        limit: optString(frame.parsed.options, "limit"),
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
          optString(frame.parsed.options, "zombie") ??
          frame.parsed.positionals[0],
        cursor: optString(frame.parsed.options, "cursor"),
        limit: optString(frame.parsed.options, "limit"),
      }),
  ),
  events: wrapEFn(
    "zombie.events",
    (frame) =>
      eventsEffectFromFlags({
        zombieId: frame.parsed.positionals[0],
        actor: optString(frame.parsed.options, "actor"),
        since: optString(frame.parsed.options, "since"),
        cursor: optString(frame.parsed.options, "cursor"),
        limit: optString(frame.parsed.options, "limit"),
        json: frame.parsed.options["json"] === true,
      }),
  ),
  steer: wrapEFn(
    "zombie.steer",
    (frame) =>
      steerEffectFromArgs(
        frame.parsed.positionals[0],
        frame.parsed.positionals[1],
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
