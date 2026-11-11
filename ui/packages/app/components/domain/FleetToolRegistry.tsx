"use client";

import type { ToolCallMessagePartProps } from "@assistant-ui/react";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { AuiConfig, Tools, defineToolkit, useAuiState } from "@assistant-ui/react";
import { ToolCallRow } from "./FleetToolCallRow";
import { TOOL_NAME } from "./tool-call-shape";
import { REPLY_ID_SUFFIX } from "./useFleetThreadEntries";

const toolkit = defineToolkit(Object.fromEntries(Object.values(TOOL_NAME).map(name => [name, {
  type: "backend" as const, render: FleetRegisteredTool,
}])));

/** Render existing backend calls. No schema or executor is supplied by the browser. */
export const FLEET_CHAT_CONFIG = AuiConfig({tools: Tools({toolkit})});

export function FleetRegisteredTool(part: ToolCallMessagePartProps<ToolArgs, unknown>) {
  const id = useAuiState(state => state.message.id);
  const settled = useAuiState(state => state.message.status?.type !== "running");
  const eventId = id.endsWith(REPLY_ID_SUFFIX) ? id.slice(0, -REPLY_ID_SUFFIX.length) : id;
  return <ToolCallRow name={part.toolName} args={part.args} argsText={part.argsText} result={part.result}
    running={part.status.type === "running"} settled={settled} eventId={eventId} />;
}
