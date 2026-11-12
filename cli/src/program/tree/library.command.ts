// Reusable fleet setups this workspace can browse, publish and remove.

import { Option } from "effect";
import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { libraryEffectFromOptions } from "../../commands/library/library.ts";
import { libraryListEffectFromOptions } from "../../commands/library/list.ts";
import { libraryDeleteEffectFromArgs } from "../../commands/library/delete.ts";
import { entryIdArgument, libraryLimitFlag, startingAfterFlag } from "./flags.ts";

const opt = Option.getOrUndefined;

// The command Effects declare a page size as a string, which is the spelling
// the wire uses. The flag has already refused anything outside the bound, so
// this only converts back.
const optNum = (value: Option.Option<number>): string | undefined =>
  Option.match(value, { onNone: () => undefined, onSome: (n) => String(n) });

const LIST = "list" as const;
const DELETE = "delete" as const;
const libraryPaging = { startingAfter: startingAfterFlag, limit: libraryLimitFlag };
const libraryListCommand = Command.make(LIST, libraryPaging).pipe(
  Command.withDescription("List this workspace's published Fleet libraries"),
  guardedHandler(({ startingAfter, limit }) => libraryListEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
);

const libraryDeleteCommand = Command.make(DELETE, {
  entryId: entryIdArgument,
}).pipe(
  Command.withDescription("Remove a published Fleet library owned by this workspace"),
  guardedHandler(({ entryId }) => libraryDeleteEffectFromArgs(entryId)),
);

export const libraryCommand = () => Command.make("library", libraryPaging).pipe(
  Command.withDescription("Browse this workspace's Fleet library gallery"),
  guardedHandler(({ startingAfter, limit }) => libraryEffectFromOptions({
    startingAfter: opt(startingAfter), limit: optNum(limit),
  })),
  Command.withSubcommands([libraryListCommand, libraryDeleteCommand]),
);
