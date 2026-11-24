// `agentsfleet chat [<fleet_id|@name>]` — talk to a fleet from the terminal.
//
// Each line is one turn on the fleet's thread, sent with `fleet steer`'s
// transport and followed on the fleet's stream. With no fleet named, the first
// line starts one: it is the fleet's first message, and the rest of the
// session talks to the fleet it made. A piped message is one line.

import type { ReplInputStream } from "../../lib/repl.ts";
import type { CliConfig } from "../../services/config.ts";
import type { Credentials } from "../../services/credentials.ts";
import type { HttpClient } from "../../services/http-client.ts";
import type { Workspaces } from "../../services/workspaces.ts";
import { Effect } from "effect";
import { ValidationError, type CliError } from "../../errors/index.ts";
import { readPipedMessage } from "../../lib/repl.ts";
import { streamGet as defaultStreamGet } from "../../lib/sse.ts";
import { isTty } from "../../output/capability.ts";
import { Output } from "../../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { createFleet } from "../fleet/create.ts";
import { promptTurns, type PromptTurn, type SteerDeps } from "../fleet/steer-prompt.ts";
import { followTurnEffect, steerEffectFromArgs, steerTurnEffect } from "../fleet/steer.ts";

export interface ChatOptions {
  /** The draft chat's flags this invocation typed, each refused by name. */
  readonly retired: ReadonlyArray<string>;
}

const CHAT_USAGE = "usage: agentsfleet chat [<fleet_id|@name>]" as const;
const REPLACEMENT =
  'start a fleet with: agentsfleet fleet create "<message>"; talk to one with: agentsfleet chat <fleet_id|@name>' as const;

export const chatEffect = (
  fleetArgument: string | undefined,
  options: ChatOptions,
  deps: SteerDeps = {},
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const [retired] = options.retired;
    if (retired !== undefined) {
      return yield* Effect.fail(new ValidationError({
        detail: `chat --${retired} was removed: a fleet starts from its first message, with no draft to review or save`,
        suggestion: REPLACEMENT,
      }));
    }
    if (fleetArgument !== undefined) return yield* steerEffectFromArgs(fleetArgument, undefined, {}, deps);

    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const streamGet = deps.streamGet ?? defaultStreamGet;
    const stdin = deps.stdin ?? (process.stdin as ReplInputStream);
    // Unset until the first line has made the fleet; every later line steers it.
    let fleetId: string | undefined;
    const turn: PromptTurn = (line, signal) => fleetId !== undefined
      ? steerTurnEffect(wsId, fleetId, line, token, streamGet, signal)
      : Effect.gen(function* () {
          const created = yield* createFleet(wsId, token, { message: line }, signal);
          fleetId = created.fleet.id;
          yield* (yield* Output).success(`Started fleet ${created.fleet.name} (${created.fleet.id})`,
            { fleet_id: created.fleet.id, turn_id: created.accepted.turn_id });
          yield* followTurnEffect(wsId, created.fleet.id, created.accepted, token, streamGet, signal);
        });

    if (isTty(stdin)) return yield* promptTurns(turn, undefined, { ...deps, stdin });
    const message = yield* Effect.promise(() => readPipedMessage(stdin));
    if (message.length === 0) {
      return yield* Effect.fail(new ValidationError({ detail: "message is required", suggestion: CHAT_USAGE }));
    }
    yield* turn(message);
  });
