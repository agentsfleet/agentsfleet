import type { ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { memo } from "react";
import { ToolCallList } from "./FleetToolCalls";
import { toolGroupActivity } from "./presentation/tool-call-activity";

type Props = {
  content: MessageState["content"];
  indices: readonly number[];
  running: boolean;
  wallMs?: number | null;
  turnId: string;
  children: ReactNode;
};

export const FleetToolGroup = memo(function FleetToolGroup(props: Props) {
  const {content, indices, running, wallMs, children} = props;
  return <ToolCallList activity={toolGroupActivity(content, indices, running, ownsRunDuration(props) ? wallMs : null)}>{children}</ToolCallList>;
}, sameToolGroup);

// Conversion reallocates part wrappers but retains a call's cached fields.
// Compare those fields so text outside this group cannot replace open rows.
// Ownership is read from the whole reply: a call appended elsewhere moves it.
function sameToolGroup(previous: Props, next: Props): boolean {
  return ownsRunDuration(previous) === ownsRunDuration(next) &&
    previous.turnId === next.turnId && previous.running === next.running && previous.wallMs === next.wallMs && previous.indices.length === next.indices.length &&
    previous.indices.every((index, position) => {
      const before = previous.content[index], after = next.content[index];
      return index === next.indices[position] && before?.type === "tool-call" && after?.type === "tool-call" &&
        before.toolCallId === after.toolCallId && before.toolName === after.toolName && before.args === after.args &&
        before.argsText === after.argsText && before.result === after.result && before.isError === after.isError && before.timing === after.timing;
    });
}

/** A run's duration belongs to a group only when it holds every call of the run. */
function ownsRunDuration({content, indices, running}: Pick<Props, "content" | "indices" | "running">): boolean {
  return !running && content.filter(part => part.type === "tool-call").length === indices.length;
}
