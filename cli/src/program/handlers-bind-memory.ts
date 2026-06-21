// Memory group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap (the workspace/fleet precedent).
// Both verbs route through the Effect dispatcher. `stdoutIsTty` arrives as
// a thunk the caller binds to the lifecycle ctx — the bind site owns the
// environment read (7 Pillars auto-JSON-when-piped); handlers stay pure.

import type { ActionFrame, MemoryHandlers } from "./cli-tree-types.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import {
  memoryListEffectFromFlags,
  memorySearchEffectFromArgs,
  type MemoryReadFlags,
} from "../commands/memory.ts";
import type { WrapEFn } from "./handlers-bind-fleet.ts";

// Single-word flags (`--fleet <id>`, `--workspace <id>`) have no camelCase
// or dashed spelling for normalizeOptions to mirror — one key each is the
// complete read (unlike the fleet/grant `--workspace-id`-era fallbacks).
const sharedFlags = (frame: ActionFrame, stdoutIsTty: boolean): MemoryReadFlags => ({
  fleetId: optString(frame.parsed.options, "fleet"),
  limit: optString(frame.parsed.options, "limit"),
  workspaceId: optString(frame.parsed.options, "workspace"),
  stdoutIsTty,
});

export const buildMemoryHandlers = (
  wrapEFn: WrapEFn,
  stdoutIsTty: () => boolean,
): MemoryHandlers => ({
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
