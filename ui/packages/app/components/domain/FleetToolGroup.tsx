import type { ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { memo } from "react";
import { ToolCallList } from "./FleetToolCalls";
import { toolGroupActivity } from "./tool-call-activity";

type Props = {
  content: MessageState["content"];
  indices: readonly number[];
  running: boolean;
  eventId: string;
  children: ReactNode;
};

export const FleetToolGroup = memo(function FleetToolGroup({content, indices, running, children}: Props) {
  return <ToolCallList activity={toolGroupActivity(content, indices, running)}>{children}</ToolCallList>;
}, sameToolGroup);

// Conversion reallocates part wrappers but retains a call's cached fields.
// Compare those fields so text outside this group cannot replace open rows.
function sameToolGroup(previous: Props, next: Props): boolean {
  return previous.eventId === next.eventId && previous.running === next.running && previous.indices.length === next.indices.length &&
    previous.indices.every((index, position) => {
      const before = previous.content[index], after = next.content[index];
      return index === next.indices[position] && before?.type === "tool-call" && after?.type === "tool-call" &&
        before.toolCallId === after.toolCallId && before.toolName === after.toolName && before.args === after.args &&
        before.argsText === after.argsText && before.result === after.result && before.isError === after.isError && before.timing === after.timing;
    });
}
