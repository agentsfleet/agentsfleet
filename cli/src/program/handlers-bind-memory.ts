// Memory group handler-binding — extracted from handlers-bind.ts to keep
// that file under the 350-line FLL cap (the workspace/agent precedent).
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
import type { WrapEFn } from "./handlers-bind-agent.ts";

// Single-word flags (`--agent <id>`, `--workspace <id>`) have no camelCase
// or dashed spelling for normalizeOptions to mirror — one key each is the
// complete read (unlike the agent/grant `--workspace-id`-era fallbacks).
const sharedFlags = (frame: ActionFrame, stdoutIsTty: boolean): MemoryReadFlags => ({
  agentId: optString(frame.parsed.options, "agent"),
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
