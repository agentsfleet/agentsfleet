import { Argument, Command, Flag } from "effect/cli";
import { platformKeysDelete, platformKeysList, platformKeysUse } from "../../commands/platform/keys.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { PLATFORM_SCOPE } from "./platform-flags.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { baseUrlFlag, opt, requiredIdFlag, requiredTextFlag } from "./flags.ts";

const list = Command.make("list").pipe(Command.withDescription("Read provider default metadata (platform-key:read)"), guardedHandler(() => platformKeysList()));
const use = Command.make("use", {
  provider: requiredTextFlag("provider", "Provider with a named secret in the source workspace"),
  workspace: requiredIdFlag("workspace", "Workspace containing the provider secret"),
  model: requiredTextFlag("model", "Catalogued provider model identifier"),
  baseUrl: baseUrlFlag,
  apiKeyStdin: Flag.Boolean("api-key-stdin").pipe(Flag.withDescription("Store a piped API key first (also requires secret:write and workspace ownership)"), Flag.withDefault(false)),
  replace: Flag.Boolean("replace").pipe(Flag.withDescription("Replace the existing named secret atomically; requires --api-key-stdin"), Flag.withDefault(false)),
}).pipe(Command.withDescription("Select the platform default (platform-key:admin)"),
  guardedHandler(({provider, workspace, model, baseUrl, apiKeyStdin, replace}) => platformKeysUse({provider, workspace, model, baseUrl: opt(baseUrl), apiKeyStdin, replace})));
const remove = Command.make("delete", {provider: Argument.String("provider")}).pipe(Command.withDescription("Delete a platform provider default (platform-key:admin)"),
  guardedHandler(({provider}) => platformKeysDelete(provider)));

export const platformKeysCommand = (visibility: Visibility) => {
  const read = grants(visibility, PLATFORM_SCOPE.keyRead);
  const admin = grants(visibility, PLATFORM_SCOPE.keyAdmin);
  return {
    listed: read || admin,
    command: Command.make("keys").pipe(
      Command.withDescription("Manage vault-backed platform defaults; keys are never revealed"),
      Command.withSubcommands([listedWhen(read)(list), listedWhen(admin)(use), listedWhen(admin)(remove)]),
    ),
  };
};
