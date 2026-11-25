// Local diagnostics and Orbit.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { doctorEffect } from "../../commands/fleet/core-ops.ts";
import { withManagedExitCode } from "../../runtime/exit-code.service.ts";
import { PET_COMMAND } from "../../constants/pet.ts";
import { petPreview } from "../../commands/pet/preview.ts";

const doctorCommand = Command.make("doctor").pipe(
  Command.withDescription("Check your setup and that the server is reachable"),
  // doctor answers a verdict, not just success: every check ran, and one
  // failed. The number goes to the exit-code service because the command
  // tree discards a handler's return value.
  guardedHandler(() => withManagedExitCode(doctorEffect)),
);

// Keep Orbit out of the fleet command list.
const petCommand = Command.make(PET_COMMAND).pipe(
  Command.withDescription("Play Orbit, a small terminal paddle game"),
  guardedHandler(() => petPreview),
  Command.unlisted,
);

export const diagnosticsCommands = () => [doctorCommand, petCommand] as const;
