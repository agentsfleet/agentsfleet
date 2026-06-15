// Agent group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap. Production routes through
// commander → these handlers → the Effect dispatcher (runEffect). Every
// agent.* leaf is an Effect.Effect<void, CliError, R>.

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
} from "../commands/agent.ts";
import {
  installEffectFromFlags,
  updateEffectFromArgs,
} from "../commands/agent_install.ts";
import { listEffectFromFlags } from "../commands/agent_list.ts";
import { logsEffectFromFlags } from "../commands/agent_logs.ts";
import { eventsEffectFromFlags } from "../commands/agent_events.ts";
import { steerEffectFromArgs } from "../commands/agent_steer.ts";
import {
  credentialAddEffectFromFlags,
  credentialShowEffectFromName,
  credentialListEffect,
  credentialDeleteEffectFromName,
} from "../commands/agent_credential.ts";

export type WrapE = <E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export type WrapEFn = <E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
) => CommandHandlerFn;

export const buildAgentHandlers = (
  wrapE: WrapE,
  wrapEFn: WrapEFn,
): Handlers[typeof AGENT] => ({
  install: wrapEFn(
    "agent.install",
    (frame) => installEffectFromFlags(optString(frame.parsed.options, FIELD_FROM)),
  ),
  update: wrapEFn(
    "agent.update",
    (frame) =>
      updateEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_FROM),
      ),
  ),
  list: wrapEFn(
    "agent.list",
    (frame) =>
      listEffectFromFlags({
        workspaceId:
          optString(frame.parsed.options, "workspace-id") ??
          optString(frame.parsed.options, "workspaceId"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  status: wrapE("agent.status", statusEffect),
  stop: wrapEFn(
    "agent.stop",
    (frame) => stopEffectFromId(frame.parsed.positionals[0]),
  ),
  resume: wrapEFn(
    "agent.resume",
    (frame) => resumeEffectFromId(frame.parsed.positionals[0]),
  ),
  kill: wrapEFn(
    "agent.kill",
    (frame) => killEffectFromId(frame.parsed.positionals[0]),
  ),
  delete: wrapEFn(
    "agent.delete",
    (frame) => deleteEffectFromId(frame.parsed.positionals[0]),
  ),
  logs: wrapEFn(
    "agent.logs",
    (frame) =>
      logsEffectFromFlags({
        agentId:
          optString(frame.parsed.options, AGENT) ??
          frame.parsed.positionals[0],
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
      }),
  ),
  events: wrapEFn(
    "agent.events",
    (frame) =>
      eventsEffectFromFlags({
        agentId: frame.parsed.positionals[0],
        actor: optString(frame.parsed.options, "actor"),
        since: optString(frame.parsed.options, "since"),
        cursor: optString(frame.parsed.options, FIELD_CURSOR),
        limit: optString(frame.parsed.options, FIELD_LIMIT),
        json: frame.parsed.options["json"] === true,
      }),
  ),
  steer: wrapEFn(
    "agent.steer",
    (frame) =>
      steerEffectFromArgs(
        frame.parsed.positionals[0],
        frame.parsed.positionals[1],
        { forceTty: frame.parsed.options[OPT_TTY] === true },
      ),
  ),
  credential: {
    add: wrapEFn(
      "agent.credential.add",
      (frame) =>
        credentialAddEffectFromFlags({
          name: frame.parsed.positionals[0],
          data: optString(frame.parsed.options, "data"),
          force: frame.parsed.options["force"] === true,
        }),
    ),
    show: wrapEFn(
      "agent.credential.show",
      (frame) => credentialShowEffectFromName(frame.parsed.positionals[0]),
    ),
    list: wrapE("agent.credential.list", credentialListEffect),
    delete: wrapEFn(
      "agent.credential.delete",
      (frame) => credentialDeleteEffectFromName(frame.parsed.positionals[0]),
    ),
  },
});
const FIELD_CURSOR = "cursor" as const;
const FIELD_FROM = "from" as const;
const FIELD_LIMIT = "limit" as const;
const AGENT = "agent" as const;
