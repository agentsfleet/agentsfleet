// Terminal actions are explicit; ordinary text only creates or continues a draft.

import { Effect } from "effect";
import { CLI_ERROR_TAG, ValidationError, type CliError } from "../../errors/index.ts";
import type { Input } from "../../services/input.ts";
import type { Output } from "../../services/output.ts";
import type { ChatActions } from "./actions.ts";
import { START } from "./setup-session.ts";

const COMMAND = {quit: "/quit", review: "/review", save: "/save", run: "/run", publish: "/publish",
  stop: "/stop", model: "/model", remove: "/delete", retry: "/retry", apply: "/apply", help: "/help"} as const;
const PROMPT = "you › ";
const HELP = "/review [instance-name] · /save · /run <message> · /publish <label> · /apply · /model <ref> · /stop · /retry · /delete · /quit. Ctrl+C stops the reply being followed.";
const ACCESS_STATUSES = new Set([401, 403, 404, 410]);

export function runChatRepl(actions: ChatActions, input: Input, output: Output): Effect.Effect<number, CliError> {
  return Effect.gen(function* () {
    yield* output.info(HELP);
    while (true) {
      const line = yield* input.readLine(PROMPT);
      if (line === null || line.trim() === COMMAND.quit) {
        yield* actions.reportRecovery();
        return actions.partial || actions.pending ? 1 : 0;
      }
      const text = line.trim();
      if (!text) continue;
      yield* dispatch(actions, text, output).pipe(Effect.catchIf(
        (error) => !(error._tag === CLI_ERROR_TAG.server && ACCESS_STATUSES.has(error.status)),
        (error) => output.warn(`${error.detail}. ${error.suggestion}`).pipe(Effect.andThen(() => actions.reportRecovery())),
      ));
    }
  });
}

export function dispatch(actions: ChatActions, text: string, output: Output): Effect.Effect<void, CliError> {
  const [command, ...words] = text.split(/\s+/u);
  const argument = words.join(" ");
  switch (command) {
    case COMMAND.review: return actions.review(argument || undefined).pipe(Effect.asVoid);
    case COMMAND.save: return actions.save();
    case COMMAND.run: return required(argument, "run message").pipe(Effect.andThen(() => actions.save({kind: START.runNow, message: argument})));
    case COMMAND.publish: return required(argument, "Library label").pipe(Effect.andThen(() => actions.publish(argument)));
    case COMMAND.apply: return actions.apply();
    case COMMAND.stop: return actions.stop();
    case COMMAND.model: return required(argument, "authoring model reference").pipe(Effect.andThen(() => actions.model(argument)));
    case COMMAND.remove: return actions.remove();
    case COMMAND.retry: return actions.retry();
    case COMMAND.help: return output.info(HELP);
    default: return text.startsWith("/") ? Effect.fail(new ValidationError({
      detail: "Unknown chat action", suggestion: HELP,
    })) : actions.message(text);
  }
}

function required(value: string, label: string): Effect.Effect<void, ValidationError> {
  return value ? Effect.void : Effect.fail(new ValidationError({detail: `Missing ${label}`, suggestion: HELP}));
}
