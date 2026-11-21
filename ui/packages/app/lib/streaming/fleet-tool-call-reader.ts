import * as v from "valibot";

import { HTTP_STATUS_NOT_FOUND } from "@/lib/api/errors";
import { fleetTurnItemUrl } from "@/lib/api/turns-types";
import {
  FILE_CHANGES,
  readToolArgs,
  TOOL_CALL_STATUS_SCHEMA,
  type FileChange,
  type ToolArgs,
  type ToolCallStatus,
} from "./fleet-stream-tool-trace";

// One tool call read in full, for the thread's "show all": over the same-origin
// route, as the turn re-read goes, so the token stays on the server and a
// send in flight never queues behind it.

/** How long the read may take before the dialog falls back to what it has. */
export const TOOL_CALL_READ_TIMEOUT_MS = 10_000;

export const TOOL_CALL_READ = {
  FULL: "full",
  NOT_KEPT: "not-kept",
  FAILED: "failed",
} as const;

/** What kind of thing an item is, as `af_wire::tool_detail::ItemKind` spells it. */
const ITEM_KIND = {
  TOOL_CALL: "tool_call",
  EDIT: "edit",
} as const;

type ItemKind = (typeof ITEM_KIND)[keyof typeof ITEM_KIND];

/** `af_wire::tool_detail::ToolCallDetail`, narrowed. */
export type ToolCallFull = {
  /** What kind of thing the call was, as recorded; absent when unreadable. */
  kind?: ItemKind;
  /** The tool it called; absent for an item that called none. */
  tool?: string;
  /** How it ended, as recorded; absent when unreadable. */
  status?: ToolCallStatus;
  args: ToolArgs | undefined;
  /** The arguments passed the full read's cap, so the runner kept none. */
  argsTruncated: boolean;
  output: string;
  /** The output passed the full read's cap and ends early. */
  outputTruncated: boolean;
  /** An edit's files and the lines each gained and lost, as recorded. */
  changes: readonly FileChange[];
};

export type ToolCallRead =
  | { kind: typeof TOOL_CALL_READ.FULL; call: ToolCallFull }
  | { kind: typeof TOOL_CALL_READ.NOT_KEPT }
  | { kind: typeof TOOL_CALL_READ.FAILED };

// Each word narrows on its own: one this build does not spell reads as absent,
// and the call still opens.
const DETAIL = v.object({
  kind: v.fallback(v.optional(v.picklist(Object.values(ITEM_KIND))), undefined),
  tool: v.fallback(v.optional(v.pipe(v.string(), v.nonEmpty())), undefined),
  status: v.fallback(v.optional(TOOL_CALL_STATUS_SCHEMA), undefined),
  arguments: v.optional(v.unknown()),
  truncated_arguments: v.fallback(v.boolean(), false),
  output: v.string(),
  truncated: v.fallback(v.boolean(), false),
  changes: FILE_CHANGES,
});

export type ToolCallAt = { workspaceId: string; fleetId: string; turnId: string; callId: string };

/** Reads one call; never throws. A 404 means the runner kept no full output
 * for it, which the dialog says in words. */
export async function readToolCall(at: ToolCallAt, signal: AbortSignal): Promise<ToolCallRead> {
  const timeout = AbortSignal.timeout(TOOL_CALL_READ_TIMEOUT_MS);
  try {
    const res = await fetch(fleetTurnItemUrl(at.workspaceId, at.fleetId, at.turnId, at.callId), {
      signal: AbortSignal.any([signal, timeout]),
      // A signed-out read answers with a redirect to sign-in; followed, it
      // would parse the sign-in page.
      redirect: "manual",
    });
    if (res.status === HTTP_STATUS_NOT_FOUND) return { kind: TOOL_CALL_READ.NOT_KEPT };
    if (!res.ok) return { kind: TOOL_CALL_READ.FAILED };
    const parsed = v.safeParse(DETAIL, await res.json());
    if (!parsed.success) return { kind: TOOL_CALL_READ.FAILED };
    const { kind, tool, status, arguments: args, truncated_arguments, output, truncated, changes } = parsed.output;
    return {
      kind: TOOL_CALL_READ.FULL,
      call: {
        ...(kind === undefined ? {} : { kind }),
        ...(tool === undefined ? {} : { tool }),
        ...(status === undefined ? {} : { status }),
        args: readToolArgs(args),
        argsTruncated: truncated_arguments,
        output,
        outputTruncated: truncated,
        changes: changes ?? [],
      },
    };
  } catch {
    // Aborted, timed out, dropped, or an unreadable body.
    return { kind: TOOL_CALL_READ.FAILED };
  }
}
