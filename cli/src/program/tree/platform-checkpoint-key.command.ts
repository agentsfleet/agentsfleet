import { Command } from "effect/cli";
import { platformCheckpointKeyRotate } from "../../commands/platform/checkpoint-key.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { PLATFORM_SCOPE } from "./platform-flags.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { idArgument } from "./flags.ts";

const rotate = Command.make("rotate", {workspace: idArgument("workspace_id", "Workspace whose checkpoint key to replace")}).pipe(
  Command.withDescription("Replace a workspace's checkpoint key; its saved checkpoints are deleted and cannot be restored (platform-key:admin)"),
  guardedHandler(({workspace}) => platformCheckpointKeyRotate(workspace)));

export const platformCheckpointKeyCommand = (visibility: Visibility) => {
  const admin = grants(visibility, PLATFORM_SCOPE.keyAdmin);
  return {
    listed: admin,
    command: Command.make("checkpoint-key").pipe(
      Command.withDescription("Manage the key that encrypts a workspace's checkpoints"),
      Command.withSubcommands([listedWhen(admin)(rotate)]),
    ),
  };
};
