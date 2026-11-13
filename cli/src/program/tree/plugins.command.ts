// Plugins: reusable fleet setups this workspace can browse, publish and remove.
// The API routes still say `fleet-libraries`; only the command reads `plugins`.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { libraryEffectFromOptions } from "../../commands/library/library.ts";
import { libraryListEffectFromOptions } from "../../commands/library/list.ts";
import { libraryDeleteEffectFromArgs } from "../../commands/library/delete.ts";
import { libraryLimitFlag, opt, optNum, pluginIdArgument, startingAfterFlag, VERB } from "./flags.ts";

const libraryPaging = { startingAfter: startingAfterFlag, limit: libraryLimitFlag };
const libraryListCommand = Command.make(VERB.list, libraryPaging).pipe(
  Command.withDescription("List this workspace's published plugins"),
  guardedHandler(({ startingAfter, limit }) => libraryListEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
);

const libraryDeleteCommand = Command.make(VERB.delete, {
  pluginId: pluginIdArgument,
}).pipe(
  Command.withDescription("Remove a published plugin owned by this workspace"),
  guardedHandler(({ pluginId }) => libraryDeleteEffectFromArgs(pluginId)),
);

export const pluginsCommand = () => Command.make("plugins", libraryPaging).pipe(
  Command.withDescription("Browse this workspace's plugins"),
  guardedHandler(({ startingAfter, limit }) => libraryEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
  Command.withSubcommands([libraryListCommand, libraryDeleteCommand]),
);
