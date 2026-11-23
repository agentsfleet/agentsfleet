import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { renderFleetMessage } from "@/components/domain/activity/messages/fleetMessageRenderers";
import { SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { SettledReplyStatus } from "@/components/domain/activity/replies/FleetReplyAnnouncements";
import { FLEET_CHAT_CONFIG } from "@/components/domain/activity/tools/FleetToolRegistry";
import { FleetScopeProvider } from "@/components/domain/conversation/stream/FleetScope";
import { convertTurn } from "@/components/domain/conversation/stream/useFleetTurnStream";
import { useFleetThreadEntries } from "@/components/domain/conversation/thread/useFleetThreadEntries";

/** Native transport supplies every row; rendering uses the production thread adapters. */
export function RestartBrowserProbe({ turns }: { turns: FleetTurn[] }) {
  const grouped = useFleetThreadEntries(turns, convertTurn);
  const runtime = useExternalStoreRuntime({ messages: grouped.entries,
    convertMessage: grouped.convertEntry, isRunning: turns.some(turn => turn.status === "received"),
    onNew: async () => {} });
  return <TooltipProvider><AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}>
    <FleetScopeProvider workspaceId="browser-workspace" fleetId="browser-transport">
      <SenderLabelProvider senderLabel="Recovery review"><ThreadPrimitive.Root>
        <SettledReplyStatus><div role="log" aria-live="off" aria-label="Recovery conversation">
          <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
        </div></SettledReplyStatus>
      </ThreadPrimitive.Root></SenderLabelProvider>
    </FleetScopeProvider>
  </AssistantRuntimeProvider></TooltipProvider>;
}
