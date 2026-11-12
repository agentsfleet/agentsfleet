import type { ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { memo } from "react";
import { ToolCallList } from "./FleetToolCalls";
import { toolGroupActivity } from "./tool-call-activity";
import {FleetReplyFigures} from "./FleetReplyFigures";
import type {ReplyFigures} from "./fleetMessageReaders";

type Props = {
  content: MessageState["content"];
  indices: readonly number[];
  running: boolean;
  wallMs?: number | null;
  figures?: ReplyFigures;
  activity?: unknown;
  eventId: string;
  children: ReactNode;
};

export const FleetToolGroup = memo(function FleetToolGroup({content, indices, running, wallMs, figures, activity, children}: Props) {
  // A run's duration belongs to the whole group only when its calls are contiguous.
  const wholeRun = !running && content.filter(part => part.type === "tool-call").length === indices.length;
  return <ToolCallList activity={toolGroupActivity(content, indices, running, wholeRun ? wallMs : null)}>{children}
    {wholeRun && figures ? <li><FleetReplyFigures figures={figures} activity={activity} /></li> : null}
  </ToolCallList>;
}, sameToolGroup);

// Conversion reallocates part wrappers but retains a call's cached fields.
// Compare those fields so text outside this group cannot replace open rows.
function sameToolGroup(previous: Props, next: Props): boolean {
  return previous.eventId === next.eventId && previous.running === next.running && previous.wallMs === next.wallMs && previous.activity === next.activity && previous.figures?.tokens === next.figures?.tokens && previous.figures?.costNanos === next.figures?.costNanos && previous.indices.length === next.indices.length &&
    previous.indices.every((index, position) => {
      const before = previous.content[index], after = next.content[index];
      return index === next.indices[position] && before?.type === "tool-call" && after?.type === "tool-call" &&
        before.toolCallId === after.toolCallId && before.toolName === after.toolName && before.args === after.args &&
        before.argsText === after.argsText && before.result === after.result && before.isError === after.isError && before.timing === after.timing;
    });
}
