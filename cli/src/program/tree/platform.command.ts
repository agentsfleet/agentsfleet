import { Command } from "effect/cli";
import { platformRunnersCommand } from "./platform-runners.command.ts";
import { platformModelsCommand } from "./platform-models.command.ts";
import { platformKeysCommand } from "./platform-keys.command.ts";
import { platformLibraryCommand } from "./platform-library.command.ts";

export const platformCommand = Command.make("platform").pipe(
  Command.withDescription("Administer platform resources with your existing operator scopes"),
  Command.withSubcommands([platformRunnersCommand, platformModelsCommand, platformKeysCommand, platformLibraryCommand]),
);
