import { z } from "zod";

import { HTTP_STATUS_NOT_FOUND } from "@/lib/api/errors";
import { fleetToolCallUrl } from "@/lib/api/events-types";
import { readToolArgs, type ToolArgs } from "./fleet-stream-tool-trace";

// One tool call read in full, for the thread's "show all": over the same-origin
// route, as the event re-read goes, so the token stays on the server and a
// send in flight never queues behind it.

/** How long the read may take before the dialog falls back to what it has. */
export const TOOL_CALL_READ_TIMEOUT_MS = 10_000;

export const TOOL_CALL_READ = {
  FULL: "full",
  NOT_KEPT: "not-kept",
  FAILED: "failed",
} as const;

/** `afd_wire::tool_detail::ToolCallDetail`, narrowed. */
export type ToolCallFull = {
  args: ToolArgs | undefined;
  /** The arguments passed the full read's cap, so the runner kept none. */
  argsTruncated: boolean;
  output: string;
  /** The output passed the full read's cap and ends early. */
  outputTruncated: boolean;
};

export type ToolCallRead =
  | { kind: typeof TOOL_CALL_READ.FULL; call: ToolCallFull }
  | { kind: typeof TOOL_CALL_READ.NOT_KEPT }
  | { kind: typeof TOOL_CALL_READ.FAILED };

const DETAIL = z.object({
  arguments: z.unknown(),
  truncated_arguments: z.boolean().catch(false),
  output: z.string(),
  truncated: z.boolean().catch(false),
});

export type ToolCallAt = { workspaceId: string; fleetId: string; eventId: string; callId: string };

/** Reads one call; never throws. A 404 means the runner kept no full output
 * for it, which the dialog says in words. */
export async function readToolCall(at: ToolCallAt, signal: AbortSignal): Promise<ToolCallRead> {
  const timeout = AbortSignal.timeout(TOOL_CALL_READ_TIMEOUT_MS);
  try {
    const res = await fetch(fleetToolCallUrl(at.workspaceId, at.fleetId, at.eventId, at.callId), {
      signal: AbortSignal.any([signal, timeout]),
      // A signed-out read answers with a redirect to sign-in; followed, it
      // would parse the sign-in page.
      redirect: "manual",
    });
    if (res.status === HTTP_STATUS_NOT_FOUND) return { kind: TOOL_CALL_READ.NOT_KEPT };
    if (!res.ok) return { kind: TOOL_CALL_READ.FAILED };
    const parsed = DETAIL.safeParse(await res.json());
    if (!parsed.success) return { kind: TOOL_CALL_READ.FAILED };
    const { arguments: args, truncated_arguments, output, truncated } = parsed.data;
    return {
      kind: TOOL_CALL_READ.FULL,
      call: { args: readToolArgs(args), argsTruncated: truncated_arguments, output, outputTruncated: truncated },
    };
  } catch {
    // Aborted, timed out, dropped, or an unreadable body.
    return { kind: TOOL_CALL_READ.FAILED };
  }
}
