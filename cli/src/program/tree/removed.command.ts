// Commands this CLI no longer has, each refused by name with what replaced it.
//
// An unknown command would answer UNKNOWN_COMMAND and a near-miss list, which
// names nothing a person used `chat` for. So `chat` still parses, unlisted, and
// takes any argument and the draft chat's old flags, only to refuse: it sends
// nothing, which is why it needs no credential and skips the auth gate.

import { Effect } from "effect";
import { Argument, Command, Flag } from "effect/cli";
import { ValidationError } from "../../errors/index.ts";

const RETIRED_SWITCHES = ["review", "save", "apply", "stop", "no-input"] as const;
const RETIRED_VALUES = [
  "definition", "fleet", "setup-revision", "candidate-id", "setup-etag",
  "review-digest", "run-now", "name", "model", "message", "operation-id",
] as const;

const chatRemoved = () => new ValidationError({
  detail: "the chat command was removed: a fleet starts with fleet create and is talked to with fleet steer",
  suggestion: 'start a fleet with: agentsfleet fleet create "<message>"; talk to one with: agentsfleet fleet steer <fleet_id|@name>',
});

const retiredFlags = () => ({
  ...Object.fromEntries(RETIRED_SWITCHES.map((name) =>
    [name, Flag.Boolean(name).pipe(Flag.withHidden, Flag.withoutNegation, Flag.withDefault(false))])),
  ...Object.fromEntries(RETIRED_VALUES.map((name) => [name, Flag.String(name).pipe(Flag.withHidden, Flag.optional)])),
});

export const removedCommands = () => [
  Command.make("chat", { rest: Argument.String("args").pipe(Argument.variadic(), Argument.optional), retired: retiredFlags() }).pipe(
    Command.withDescription("Removed: use fleet create and fleet steer"),
    Command.withHandler(() => Effect.fail(chatRemoved())),
    Command.unlisted,
  ),
] as const;
