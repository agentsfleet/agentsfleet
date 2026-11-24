import { Argument, Command } from "effect/cli";
import { platformDefaultDelete, platformDefaultList, platformDefaultUse } from "../../commands/platform/model-default.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { PLATFORM_SCOPE } from "./platform-flags.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { baseUrlFlag, opt, requiredIdFlag, requiredTextFlag } from "./flags.ts";

const list = Command.make("list").pipe(Command.withDescription("List configured providers and the selected default (platform-key:read)"), guardedHandler(() => platformDefaultList()));
const use = Command.make("use", {
  provider: requiredTextFlag("provider", "Provider with a named secret in the source workspace"),
  workspace: requiredIdFlag("workspace", "Workspace containing the provider secret"),
  model: requiredTextFlag("model", "Catalogued provider model identifier"),
  baseUrl: baseUrlFlag,
}).pipe(Command.withDescription("Select the platform default using a workspace secret (platform-key:admin)"),
  guardedHandler(({provider, workspace, model, baseUrl}) => platformDefaultUse({provider, workspace, model, baseUrl: opt(baseUrl)})));
const remove = Command.make("delete", {provider: Argument.String("provider")}).pipe(Command.withDescription("Delete a platform provider default (platform-key:admin)"),
  guardedHandler(({provider}) => platformDefaultDelete(provider)));

export const platformModelDefaultCommand = (visibility: Visibility) => {
  const read = grants(visibility, PLATFORM_SCOPE.keyRead);
  const admin = grants(visibility, PLATFORM_SCOPE.keyAdmin);
  return {
    listed: read || admin,
    command: Command.make("default").pipe(
      Command.withDescription("Manage the platform default model and its provider reference"),
      Command.withSubcommands([listedWhen(read)(list), listedWhen(admin)(use), listedWhen(admin)(remove)]),
    ),
  };
};
