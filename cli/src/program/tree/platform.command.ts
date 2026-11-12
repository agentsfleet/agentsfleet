// The operator families, each advertised only to a caller that can use one
// of its commands. A family with no listed command is unlisted itself, and
// so is `platform` when no family is listed: the library hides unlisted
// children but not a parent left empty by them.

import { Command } from "effect/cli";
import { platformRunnersCommand } from "./platform-runners.command.ts";
import { platformModelsCommand } from "./platform-models.command.ts";
import { platformKeysCommand } from "./platform-keys.command.ts";
import { platformLibraryCommand } from "./platform-library.command.ts";
import { listedWhen, type Visibility } from "./visibility.ts";

export const platformCommand = (visibility: Visibility) => {
  const runners = platformRunnersCommand(visibility);
  const models = platformModelsCommand(visibility);
  const keys = platformKeysCommand(visibility);
  const library = platformLibraryCommand(visibility);
  const families = [runners, models, keys, library];
  return Command.make("platform").pipe(
    Command.withDescription("Administer platform resources with your existing operator scopes"),
    Command.withSubcommands([
      listedWhen(runners.listed)(runners.command),
      listedWhen(models.listed)(models.command),
      listedWhen(keys.listed)(keys.command),
      listedWhen(library.listed)(library.command),
    ]),
    listedWhen(families.some((family) => family.listed)),
  );
};
