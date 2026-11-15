// Plugins: reusable fleet setups this workspace can browse, publish and remove.

import { Argument, Command } from "effect/cli";
import { guardedHandler } from "./tree/guarded-handler.ts";
import { pluginEffectFromOptions } from "../commands/plugins/plugin.ts";
import { pluginListEffectFromOptions } from "../commands/plugins/list.ts";
import { pluginDeleteEffectFromArgs } from "../commands/plugins/delete.ts";
import { pluginShowEffect, pluginHistoryEffect, pluginRevisionEffect } from "../commands/plugins-inspect.ts";
import { pluginId, pluginRevisionId } from "../commands/plugins-read.ts";
import { idArgument, pluginLimitFlag, opt, optNum, pluginIdArgument, startingAfterFlag, VERB } from "./tree/flags.ts";

const inspectedPlugin = pluginIdArgument.pipe(Argument.withSchema(pluginId));
const inspectedRevision = idArgument("revision_id", "Immutable revision ID").pipe(Argument.withSchema(pluginRevisionId), Argument.optional);
const pluginPaging = { startingAfter: startingAfterFlag, limit: pluginLimitFlag };
const pluginListCommand = Command.make(VERB.list, pluginPaging).pipe(
  Command.withDescription("List this workspace's published plugins"),
  guardedHandler(({ startingAfter, limit }) => pluginListEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
);

const pluginDeleteCommand = Command.make(VERB.delete, {
  pluginId: pluginIdArgument,
}).pipe(
  Command.withDescription("Remove a published plugin owned by this workspace"),
  guardedHandler(({ pluginId }) => pluginDeleteEffectFromArgs(pluginId)),
);

const pluginShowCommand = Command.make(VERB.show, {pluginId: inspectedPlugin}).pipe(
  Command.withDescription("Inspect a plugin and its current revision"),
  guardedHandler(({pluginId}) => pluginShowEffect(pluginId)),
);

const pluginRevisionsCommand = Command.make("revisions", {
  pluginId: inspectedPlugin,
  revisionId: inspectedRevision,
  ...pluginPaging,
}).pipe(
  Command.withDescription("List plugin revisions or inspect one immutable revision"),
  guardedHandler(({pluginId, revisionId, startingAfter, limit}) => {
    const revision = opt(revisionId);
    return revision === undefined ? pluginHistoryEffect(pluginId, {startingAfter: opt(startingAfter), limit: optNum(limit)})
      : pluginRevisionEffect(pluginId, revision);
  }),
);

export const pluginsCommand = () => Command.make("plugins", pluginPaging).pipe(
  Command.withDescription("Browse this workspace's plugins"),
  guardedHandler(({ startingAfter, limit }) => pluginEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
  Command.withSubcommands([pluginListCommand, pluginShowCommand, pluginRevisionsCommand, pluginDeleteCommand]),
);
