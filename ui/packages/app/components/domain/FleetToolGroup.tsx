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

/** What this group owns of the run: its whole duration, and the one figures entry. */
function ownership({content, indices, running}: Pick<Props, "content" | "indices" | "running">) {
  const calls = content.flatMap((part, index) => part.type === "tool-call" ? [index] : []);
  return {
    // A run's duration belongs to a group only when it holds every call of the run.
    wholeRun: !running && calls.length === indices.length,
    // A split run keeps one figures entry, in the last group, once the run settles.
    footer: !running && indices.at(-1) === calls.at(-1),
  };
}

export const FleetToolGroup = memo(function FleetToolGroup(props: Props) {
  const {content, indices, running, wallMs, figures, activity, children} = props;
  const {wholeRun, footer} = ownership(props);
  return <ToolCallList activity={toolGroupActivity(content, indices, running, wholeRun ? wallMs : null)}
    footer={footer && figures ? <FleetReplyFigures figures={figures} activity={activity} /> : null}>{children}</ToolCallList>;
}, sameToolGroup);

// Conversion reallocates part wrappers but retains a call's cached fields.
// Compare those fields so text outside this group cannot replace open rows.
// Ownership is read from the whole reply: a call appended elsewhere moves it.
function sameToolGroup(previous: Props, next: Props): boolean {
  const was = ownership(previous), now = ownership(next);
  return was.wholeRun === now.wholeRun && was.footer === now.footer &&
    previous.eventId === next.eventId && previous.running === next.running && previous.wallMs === next.wallMs && previous.activity === next.activity && previous.figures?.tokens === next.figures?.tokens && previous.figures?.costNanos === next.figures?.costNanos && previous.indices.length === next.indices.length &&
    previous.indices.every((index, position) => {
      const before = previous.content[index], after = next.content[index];
      return index === next.indices[position] && before?.type === "tool-call" && after?.type === "tool-call" &&
        before.toolCallId === after.toolCallId && before.toolName === after.toolName && before.args === after.args &&
        before.argsText === after.argsText && before.result === after.result && before.isError === after.isError && before.timing === after.timing;
    });
}
