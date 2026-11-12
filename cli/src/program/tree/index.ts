// The command tree, composed per invocation.
//
// Each domain file exports a factory rather than a command value, and this is
// the only place they meet. Building the tree is cheap and pure, so the entry
// point builds it for each run instead of sharing one module-level value
// across runs.

import { Command } from "effect/cli";
import { globalFlags } from "./global-flags.ts";
import { authCommands } from "./auth.command.ts";
import { diagnosticsCommands } from "./diagnostics.command.ts";
import { workspaceCommand, usageCommand } from "./workspace.command.ts";
import { chatCommand } from "./chat.command.ts";
import { fleetCommand } from "./fleet.command.ts";
import { libraryCommand } from "./library.command.ts";
import { modelsCommand } from "./models.command.ts";
import { secretsCommand } from "./secrets.command.ts";
import { apiKeysCommand, connectionsCommand } from "./access.command.ts";
import { platformCommand } from "./platform.command.ts";
import { UNKNOWN_VISIBILITY, type Visibility } from "./visibility.ts";

export const CLI_NAME = "agentsfleet" as const;

// `Unknown` is the fail-closed default: no operator family is advertised.
export const buildCommandTree = (visibility: Visibility = UNKNOWN_VISIBILITY) =>
  Command.make(CLI_NAME).pipe(
    Command.withDescription("agentsfleet cli"),
    Command.withSharedFlags(globalFlags),
    Command.withSubcommands([
      ...authCommands(),
      ...diagnosticsCommands(),
      workspaceCommand(),
      chatCommand(),
      fleetCommand(),
      libraryCommand(),
      modelsCommand(),
      secretsCommand(),
      connectionsCommand(),
      apiKeysCommand(),
      usageCommand(),
      platformCommand(visibility),
    ]),
  );
