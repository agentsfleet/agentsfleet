// Memory group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap (the workspace/zombie precedent).
// Both verbs route through the Effect dispatcher. `stdoutIsTty` arrives as
// a thunk the caller binds to the lifecycle ctx — the bind site owns the
// environment read (7 Pillars auto-JSON-when-piped); handlers stay pure.

import type { ActionFrame, Handlers } from "./cli-tree-types.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import {
  memoryListEffectFromFlags,
  memorySearchEffectFromArgs,
  type MemoryReadFlags,
} from "../commands/memory.ts";
import type { WrapEFn } from "./handlers-bind-zombie.ts";

const MEMORY = "memory" as const;

const sharedFlags = (frame: ActionFrame, stdoutIsTty: boolean): MemoryReadFlags => ({
  zombieId:
    optString(frame.parsed.options, "zombie") ??
    optString(frame.parsed.options, "zombieId") ??
    optString(frame.parsed.options, "zombie-id"),
  limit: optString(frame.parsed.options, "limit"),
  workspaceId:
    optString(frame.parsed.options, "workspace") ??
    optString(frame.parsed.options, "workspaceId") ??
    optString(frame.parsed.options, "workspace-id"),
  stdoutIsTty,
});

export const buildMemoryHandlers = (
  wrapEFn: WrapEFn,
  stdoutIsTty: () => boolean,
): Handlers[typeof MEMORY] => ({
  list: wrapEFn("memory.list", (frame) =>
    memoryListEffectFromFlags({
      ...sharedFlags(frame, stdoutIsTty()),
      category: optString(frame.parsed.options, "category"),
    }),
  ),
  search: wrapEFn("memory.search", (frame) =>
    memorySearchEffectFromArgs(
      frame.parsed.positionals[0],
      sharedFlags(frame, stdoutIsTty()),
    ),
  ),
});
