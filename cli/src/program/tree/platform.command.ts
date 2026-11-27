// The operator families, each advertised only to a caller that can use one
// of its commands. A family with no listed command is unlisted itself, and
// so is `platform` when no family is listed: the library hides unlisted
// children but not a parent left empty by them.

import { Command } from "effect/cli";
import { platformRunnersCommand } from "./platform-runners.command.ts";
import { platformModelsCommand } from "./platform-models.command.ts";
import { platformCheckpointKeyCommand } from "./platform-checkpoint-key.command.ts";
import { listedWhen, type Visibility } from "./visibility.ts";

export const platformCommand = (visibility: Visibility) => {
  const runners = platformRunnersCommand(visibility);
  const models = platformModelsCommand(visibility);
  const checkpointKey = platformCheckpointKeyCommand(visibility);
  const families = [runners, models, checkpointKey];
  return Command.make("platform").pipe(
    Command.withDescription("Run the platform itself (for operators)"),
    Command.withSubcommands([
      listedWhen(runners.listed)(runners.command),
      listedWhen(models.listed)(models.command),
      listedWhen(checkpointKey.listed)(checkpointKey.command),
    ]),
    listedWhen(families.some((family) => family.listed)),
  );
};
