"use client";

import { memo, useMemo } from "react";
import type { MessageState } from "@assistant-ui/react";
import { List, ListItem } from "@agentsfleet/design-system";

import { TOOL_CALL_STATUS, type ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { OUTPUT_RAIL, RailRow } from "./FleetToolCallBody";
import { TOOL_BULLET } from "./FleetToolCalls";
import { readToolResult } from "./fleetReplyMessage";
import { FAILED_MARK } from "./tool-call-copy";
import { exploreLines, type ExploreCall } from "./tool-call-explore";

// The reads a reply made in a row, folded the way Codex folds them: one bold
// "Exploring" while any runs, "Explored" once all are back, and a line per
// look under it. Only the calls the group map sends here, which change
// nothing, so folding them hides nothing the fleet did.

export const EXPLORING_LABEL = "Exploring";
export const EXPLORED_LABEL = "Explored";
const TARGET_SEPARATOR = ", ";
const FAILED: ReadonlySet<string> = new Set([TOOL_CALL_STATUS.FAILED, TOOL_CALL_STATUS.INTERRUPTED]);

type FleetExploredProps = {
  content: MessageState["content"];
  /** The group's parts, by index into `content`. */
  indices: readonly number[];
  running: boolean;
};

export const FleetExplored = memo(function FleetExplored({ content, indices, running }: FleetExploredProps) {
  const lines = useMemo(() => exploreLines(exploreCalls(content, indices)), [content, indices]);
  const label = running ? EXPLORING_LABEL : EXPLORED_LABEL;
  return (
    <div data-explored={label} className="mb-xs flex min-w-0 flex-col gap-3xs font-mono text-label leading-mono text-text-dim">
      <span className="flex items-baseline gap-xs">
        <span aria-hidden="true" data-tool-shimmer={running} className="text-text-dim transition-colors duration-snap ease-snap">
          {TOOL_BULLET}
        </span>
        <span className="font-semibold text-foreground">{label}</span>
      </span>
      <List variant="plain" aria-label={label} className="flex flex-col space-y-0 pl-md">
        {lines.map((line, index) => (
          <ListItem key={index} data-failed={line.failed || undefined}>
            <RailRow glyph={index === 0 ? OUTPUT_RAIL : null}>
              <span className="text-info">{line.verb}</span>{" "}
              <span className="text-foreground">{[...line.targets].join(TARGET_SEPARATOR)}</span>
              {line.scope === null ? null : ` ${line.scope}`}
              {line.failed ? <span className="text-destructive"> {FAILED_MARK}</span> : null}
            </RailRow>
          </ListItem>
        ))}
      </List>
    </div>
  );
});

/** The group's calls with what Explored needs of each: a call is failed once
 * its outcome says it failed or was cut off. */
function exploreCalls(content: MessageState["content"], indices: readonly number[]): ExploreCall[] {
  return indices.flatMap((index) => {
    const part = content[index];
    if (part?.type !== "tool-call") return [];
    const status = readToolResult(part.result)?.status;
    const args: ToolArgs = part.args;
    return [{ name: part.toolName, args, failed: status !== undefined && FAILED.has(status) }];
  });
}
