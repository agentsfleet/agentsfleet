// The workspace secret vault.
//
// Every verb names a secret by its vault name, and none echoes secret bytes.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import {
  secretAddEffectFromFlags,
  secretDeleteEffectFromName,
  secretShowEffectFromName,
  secretUpdateEffectFromFlags,
} from "../../commands/secret/secret.ts";
import { secretListEffect } from "../../commands/secret/list.ts";
import { apiKeyFlag, baseUrlFlag, dataFlag, dataReplacementFlag, modelFlag, opt, providerFlag, secretNameArgument, VERB } from "./flags.ts";


// Two ways to supply the body: the generic `--data` blob, or the typed
// provider flags composing the same JSON object. `--base-url` is checked at
// parse time so a non-https endpoint costs no request; `--provider` cannot be,
// because its accepted set is whatever this server's catalogue serves.
const secretCreateCommand = Command.make("create", {
  name: secretNameArgument,
  data: dataFlag,
  provider: providerFlag,
  baseUrl: baseUrlFlag,
  apiKey: apiKeyFlag,
  model: modelFlag,
}).pipe(
  Command.withDescription("Save a secret (a JSON object) in this workspace"),
  guardedHandler(({ name, data, provider, baseUrl, apiKey, model }) =>
    secretAddEffectFromFlags({
      name,
      data: opt(data),
      provider: opt(provider),
      baseUrl: opt(baseUrl),
      apiKey: opt(apiKey),
      model: opt(model),
    }),
  ),
);

// Replaces the stored body in place: the name stays claimed for the whole
// call, so fleets that require it keep resolving. Delete-then-create also
// replaces a value, but leaves a window where the name does not exist.
const secretUpdateCommand = Command.make(VERB.update, {
  name: secretNameArgument,
  data: dataReplacementFlag,
  provider: providerFlag,
  baseUrl: baseUrlFlag,
  apiKey: apiKeyFlag,
  model: modelFlag,
}).pipe(
  Command.withDescription("Replace a secret's value, keeping its name"),
  guardedHandler(({ name, data, provider, baseUrl, apiKey, model }) =>
    secretUpdateEffectFromFlags({
      name,
      data: opt(data),
      provider: opt(provider),
      baseUrl: opt(baseUrl),
      apiKey: opt(apiKey),
      model: opt(model),
    }),
  ),
);

const secretShowCommand = Command.make("show", { name: secretNameArgument }).pipe(
  Command.withDescription("Check that a secret exists (its value is never shown)"),
  guardedHandler(({ name }) => secretShowEffectFromName(name)),
);

const secretListCommand = Command.make(VERB.list).pipe(
  Command.withDescription("List the secrets saved in this workspace"),
  guardedHandler(() => secretListEffect),
);

const secretDeleteCommand = Command.make(VERB.delete, { name: secretNameArgument }).pipe(
  Command.withDescription("Delete a secret from this workspace"),
  guardedHandler(({ name }) => secretDeleteEffectFromName(name)),
);

export const secretsCommand = () => Command.make("secrets").pipe(
  Command.withDescription("Save passwords and API keys your fleets can ask for"),
  Command.withSubcommands([
    secretCreateCommand,
    secretUpdateCommand,
    secretShowCommand,
    secretListCommand,
    secretDeleteCommand,
  ]),
);
