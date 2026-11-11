"use client";

import { useMemo, useState, type ReactNode } from "react";
import { MessagePrimitive, type AssistantState } from "@assistant-ui/react";
import { fleetChatElements } from "./fleet-chat-adapters";
import { FleetChatElementView } from "./FleetChatElements";

type MessageState = AssistantState["message"];

export function renderFleetMessage({message}: {message: MessageState}): ReactNode {
  return <FleetMessageRoot message={message} />;
}

function FleetMessageRoot({message}: {message: MessageState}) {
  const settled = message.role === "assistant" && message.status.type !== "running";
  // Only completed history may skip layout. Enabling containment on the
  // transition from live to complete would move the reader's scroll anchor.
  const [skipLayout] = useState(settled);
  const elements = useMemo(() => fleetChatElements(message), [message]);
  return <MessagePrimitive.Root className="w-full" data-testid="fleet-message"
    data-settled={settled ? "true" : undefined} data-skip-layout={skipLayout ? "true" : undefined}>
    {elements.map(element => <FleetChatElementView key={element.kind} element={element} />)}
  </MessagePrimitive.Root>;
}
