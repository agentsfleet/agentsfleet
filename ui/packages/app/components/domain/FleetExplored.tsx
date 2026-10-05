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
// nothing, so folding them hides nothing the fleet did: a read that failed
// keeps its error under its line.

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
    <div data-explored={label} className="mb-xs flex min-w-0 flex-col gap-xs font-mono text-label leading-mono text-text-subtle">
      <span className="flex items-baseline gap-xs">
        <span aria-hidden="true" data-tool-bullet="" data-tool-shimmer={running} className="text-text-dim transition-colors duration-snap ease-snap">
          {TOOL_BULLET}
        </span>
        <span className="font-semibold text-foreground">{label}</span>
      </span>
      <List variant="plain" aria-label={label} className="flex flex-col space-y-0 pl-md text-label leading-mono">
        {lines.map((line, index) => (
          <ListItem key={index} data-failed={line.failed || undefined}>
            <RailRow glyph={index === 0 ? OUTPUT_RAIL : null}>
              <span className="text-info">{line.verb}</span>{" "}
              <span className="text-foreground">{line.targets.join(TARGET_SEPARATOR)}</span>
              {line.scope === null ? null : ` ${line.scope}`}
              {line.failed ? <span className="text-destructive"> {FAILED_MARK}</span> : null}
            </RailRow>
            {line.error === null ? null : <RailRow glyph={null}>{line.error}</RailRow>}
          </ListItem>
        ))}
      </List>
    </div>
  );
}, sameFold);

/** The group's calls with what Explored needs of each: a call is failed once
 * its outcome says it failed or was cut off. */
function exploreCalls(content: MessageState["content"], indices: readonly number[]): ExploreCall[] {
  return indices.flatMap((index) => {
    const part = content[index];
    if (part?.type !== "tool-call") return [];
    const outcome = readToolResult(part.result);
    const status = outcome?.status;
    const args: ToolArgs = part.args;
    const failed = status !== undefined && FAILED.has(status);
    return [{ name: part.toolName, args: Object.keys(args).length > 0 ? args : undefined, failed, output: outcome?.outputHead }];
  });
}

// The message's parts are rebuilt on every streamed word and the library
// regroups them, so neither `content` nor `indices` keeps its identity. The
// fold redraws only when a part it reads changed.
export function sameFold(prev: FleetExploredProps, next: FleetExploredProps): boolean {
  if (prev.running !== next.running || prev.indices.length !== next.indices.length) return false;
  const after = next.indices.map((index) => next.content[index]);
  return prev.indices.every((index, at) => samePart(prev.content[index], after[at]));
}

type ContentPart = MessageState["content"][number];

function samePart(before: ContentPart | undefined, after: ContentPart | undefined): boolean {
  if (before?.type !== "tool-call" || after?.type !== "tool-call") return before === after;
  return before.toolName === after.toolName && before.args === after.args && before.result === after.result;
}
