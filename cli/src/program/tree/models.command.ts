// The models a tenant can run and the one it runs.
//
// `list` reads the catalogue this server serves. `current`, `use` and `reset`
// read and set the tenant's provider posture: a self-managed secret and model,
// or the platform default. One group, because a person choosing a model asks
// both questions in the same breath.

import { Option } from "effect";
import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { modelsEffectFromFlags } from "../../commands/models/models.ts";
import {
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
  tenantProviderShowEffect,
} from "../../commands/tenant/tenant.ts";
import { modelOverrideFlag, providerFlag, secretNameFlag } from "./flags.ts";

const opt = Option.getOrUndefined;

const modelsListCommand = Command.make("list", {
  provider: providerFlag,
  advanced: Flag.Boolean("advanced").pipe(
    Flag.withDescription("Include advanced models"),
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("List the model catalogue this server serves"),
  guardedHandler(({ provider, advanced }) => modelsEffectFromFlags({ provider: opt(provider), advanced })),
);

const modelsCurrentCommand = Command.make("current").pipe(
  Command.withDescription("Show the provider and model this tenant runs"),
  guardedHandler(() => tenantProviderShowEffect),
);

const modelsUseCommand = Command.make("use", {
  secret: secretNameFlag,
  model: modelOverrideFlag,
}).pipe(
  Command.withDescription("Run a self-managed provider secret"),
  guardedHandler(({ secret, model }) => tenantProviderAddEffectFromArgs(opt(secret), opt(model))),
);

const modelsResetCommand = Command.make("reset").pipe(
  Command.withDescription("Return to the platform default"),
  guardedHandler(() => tenantProviderDeleteEffect),
);

export const modelsCommand = () => Command.make("models").pipe(
  Command.withDescription("Browse the model catalogue and choose the tenant's provider"),
  Command.withSubcommands([
    modelsListCommand,
    modelsCurrentCommand,
    modelsUseCommand,
    modelsResetCommand,
  ]),
);
