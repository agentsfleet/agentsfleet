// `agentsfleet chat [<fleet_id|@name>]`: talk to a fleet; with none named, the
// first line starts one.

import { Option } from "effect";
import { Command, Flag } from "effect/cli";
import { chatEffect } from "../../commands/chat/chat.ts";
import { fleetIdOrNameOptionalArgument, opt } from "./flags.ts";
import { guardedHandler } from "./guarded-handler.ts";

// The draft chat's flags. Each still parses, hidden, so `chat` refuses it by
// name and points at what replaced it, where an unknown flag would only be
// called unknown.
const RETIRED_SWITCHES = ["review", "save", "apply", "stop", "no-input"] as const;
const RETIRED_VALUES = [
  "definition", "fleet", "setup-revision", "candidate-id", "setup-etag",
  "review-digest", "run-now", "name", "model", "message", "operation-id",
] as const;

const retiredFlags = () => ({
  ...Object.fromEntries(RETIRED_SWITCHES.map((name) =>
    [name, Flag.Boolean(name).pipe(Flag.withHidden, Flag.withoutNegation, Flag.withDefault(false))])),
  ...Object.fromEntries(RETIRED_VALUES.map((name) => [name, Flag.String(name).pipe(Flag.withHidden, Flag.optional)])),
});

export const chatCommand = () => Command.make("chat", {
  fleetId: fleetIdOrNameOptionalArgument,
  retired: retiredFlags(),
}).pipe(
  Command.withDescription("Talk to a fleet; with none named, the first line starts one"),
  guardedHandler(({ fleetId, retired }) => chatEffect(opt(fleetId), {
    retired: Object.entries(retired)
      .filter(([, typed]) => typeof typed === "boolean" ? typed : Option.isSome(typed))
      .map(([name]) => name),
  })),
);
