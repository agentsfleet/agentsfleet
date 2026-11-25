// The models a tenant can run and the one it runs.
//
// `list` reads the catalogue this server serves. `current`, `use` and `reset`
// read and set the tenant's provider posture: a self-managed secret and model,
// or the platform default. One group, because a person choosing a model asks
// both questions in the same breath.

import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { modelsEffectFromFlags } from "../../commands/models/models.ts";
import {
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
  tenantProviderShowEffect,
} from "../../commands/tenant/tenant.ts";
import { modelOverrideFlag, opt, providerFlag, secretNameFlag, workspaceFlag } from "./flags.ts";

const modelsListCommand = Command.make("list", {
  provider: providerFlag,
  advanced: Flag.Boolean("advanced").pipe(
    Flag.withDescription("Also show advanced models"),
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("List the AI models you can use"),
  guardedHandler(({ provider, advanced }) => modelsEffectFromFlags({ provider: opt(provider), advanced })),
);

const modelsCurrentCommand = Command.make("current").pipe(
  Command.withDescription("Show which AI provider and model your account uses"),
  guardedHandler(() => tenantProviderShowEffect),
);

const modelsUseCommand = Command.make("use", {
  secret: secretNameFlag,
  workspace: workspaceFlag,
  model: modelOverrideFlag,
}).pipe(
  Command.withDescription("Use your own provider API key, saved as a secret"),
  guardedHandler(({ secret, model, workspace }) => tenantProviderAddEffectFromArgs(opt(secret), opt(model), opt(workspace))),
);

const modelsResetCommand = Command.make("reset").pipe(
  Command.withDescription("Go back to the default model that comes with your account"),
  guardedHandler(() => tenantProviderDeleteEffect),
);

export const modelsCommand = () => Command.make("models").pipe(
  Command.withDescription("See the AI models and choose which one your account uses"),
  Command.withSubcommands([
    modelsListCommand,
    modelsCurrentCommand,
    modelsUseCommand,
    modelsResetCommand,
  ]),
);
