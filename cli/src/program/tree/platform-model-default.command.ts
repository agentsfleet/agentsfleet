import { Argument, Command } from "effect/cli";
import { platformDefaultDelete, platformDefaultList, platformDefaultUse } from "../../commands/platform/model-default.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { PLATFORM_SCOPE } from "./platform-flags.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { baseUrlFlag, opt, requiredIdFlag, requiredTextFlag } from "./flags.ts";

const list = Command.make("list").pipe(Command.withDescription("List the providers set up, and which is the default (platform-key:read)"), guardedHandler(() => platformDefaultList()));
const use = Command.make("use", {
  provider: requiredTextFlag("provider", "Provider whose key is saved in that workspace"),
  workspace: requiredIdFlag("workspace", "Workspace where the provider key is saved"),
  model: requiredTextFlag("model", "Model ID from the catalogue"),
  baseUrl: baseUrlFlag,
}).pipe(Command.withDescription("Make a model the default, using a key saved in a workspace (platform-key:admin)"),
  guardedHandler(({provider, workspace, model, baseUrl}) => platformDefaultUse({provider, workspace, model, baseUrl: opt(baseUrl)})));
const remove = Command.make("delete", {provider: Argument.String("provider")}).pipe(Command.withDescription("Remove a provider's default setting (platform-key:admin)"),
  guardedHandler(({provider}) => platformDefaultDelete(provider)));

export const platformModelDefaultCommand = (visibility: Visibility) => {
  const read = grants(visibility, PLATFORM_SCOPE.keyRead);
  const admin = grants(visibility, PLATFORM_SCOPE.keyAdmin);
  return {
    listed: read || admin,
    command: Command.make("default").pipe(
      Command.withDescription("Set the default model every account gets"),
      Command.withSubcommands([listedWhen(read)(list), listedWhen(admin)(use), listedWhen(admin)(remove)]),
    ),
  };
};
