// Checks on this machine's setup, and the owl.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { doctorEffect } from "../../commands/fleet/core-ops.ts";
import { withManagedExitCode } from "../../runtime/exit-code.service.ts";
import { PET_COMMAND } from "../../constants/pet.ts";
import { petPreview } from "../../commands/pet/preview.ts";

const doctorCommand = Command.make("doctor").pipe(
  Command.withDescription("Diagnose CLI configuration and connectivity"),
  // doctor answers a verdict, not just success: every check ran, and one
  // failed. The number goes to the exit-code service because the command
  // tree discards a handler's return value.
  guardedHandler(() => withManagedExitCode(doctorEffect)),
);

// Runnable by name, never advertised: it is a preview of the progress owl,
// not a command anyone needs to find.
const petCommand = Command.make(PET_COMMAND).pipe(
  Command.withDescription("Watch Scout, your terminal owl, for five seconds"),
  guardedHandler(() => petPreview),
  Command.unlisted,
);

export const diagnosticsCommands = () => [doctorCommand, petCommand] as const;
