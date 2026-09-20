// A Fleet's durable memory, read-only.
//
// There is no write verb and that is architecture, not an omission: the tenant
// memory plane is written by the Fleet and read by everyone else.

import { Option } from "effect";
import { Command } from "effect/unstable/cli";
import {
  memoryListEffectFromFlags,
  memorySearchEffectFromArgs,
} from "../../commands/memory.ts";
import {
  DEFAULT_LIST_LIMIT,
  DEFAULT_RECALL_LIMIT,
  MAX_RECALL_LIMIT,
} from "../../constants/memory-limits.ts";
import {
  categoryFlag,
  fleetFlag,
  memoryLimitFlag,
  queryArgument,
  startingAfterFlag,
  workspaceFlag,
} from "./flags.ts";
import { stdoutIsTty } from "./tty.ts";

const opt = Option.getOrUndefined;

const optNum = (value: Option.Option<number>): string | undefined =>
  Option.match(value, { onNone: () => undefined, onSome: (n) => String(n) });

const memoryListCommand = Command.make("list", {
  fleet: fleetFlag,
  category: categoryFlag,
  limit: memoryLimitFlag,
  startingAfter: startingAfterFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription(
    `List entries newest-first (server default ${DEFAULT_LIST_LIMIT}, cap ${MAX_RECALL_LIMIT})`,
  ),
  Command.withHandler(({ fleet, category, limit, startingAfter, workspace }) =>
    memoryListEffectFromFlags({
      fleetId: opt(fleet),
      category: opt(category),
      limit: optNum(limit),
      startingAfter: opt(startingAfter),
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

const memorySearchCommand = Command.make("search", {
  query: queryArgument,
  fleet: fleetFlag,
  limit: memoryLimitFlag,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription(
    `Substring-search keys and content (server default ${DEFAULT_RECALL_LIMIT}, cap ${MAX_RECALL_LIMIT})`,
  ),
  Command.withHandler(({ query, fleet, limit, workspace }) =>
    memorySearchEffectFromArgs(query, {
      fleetId: opt(fleet),
      limit: optNum(limit),
      workspaceId: opt(workspace),
      stdoutIsTty: stdoutIsTty(),
    }),
  ),
);

export const memoryCommand = Command.make("memory").pipe(
  Command.withDescription("Inspect a Fleet's durable memory (read-only)"),
  Command.withSubcommands([memoryListCommand, memorySearchCommand]),
);
