"use client";

import { memo } from "react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { Skeleton } from "@agentsfleet/design-system";
import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";
import { SteerComposer, type SteerComposerProps } from "./SteerComposer";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { SettledReplyStatus } from "./FleetReplyBody";
import { ChatThread } from "./chat/ChatThread";

const PANEL_TITLE = "Chat";
const EMPTY_HINT = "Message this fleet or wait for its next trigger. Activity and outcomes appear here.";
const BACKFILL_LABEL = "Loading recent activity";

type FleetThreadViewportProps = SteerComposerProps & {
  eventsCount: number;
  connectionStatus: ConnectionStatus;
  /** Reconnects the live stream now, from the offline notice. */
  onRetry: () => void;
};

// Memoised: the thread re-renders on every streamed flush, and nothing this
// shell draws moves with one. The messages below subscribe on their own.
export const FleetThreadViewport = memo(function FleetThreadViewport({
  eventsCount, connectionStatus, onRetry, pending, onResend, onDismiss, onRestored, onDraft,
}: FleetThreadViewportProps) {
  return <ChatThread testId="fleet-thread-root" label="Fleet chat" footer={<>
    <FleetConnectionNotice status={connectionStatus} onRetry={onRetry} />
    <SteerComposer pending={pending} onResend={onResend} onDismiss={onDismiss} onRestored={onRestored} onDraft={onDraft} />
  </>}>
    <SettledReplyStatus><ChatHistory eventsCount={eventsCount} connectionStatus={connectionStatus} /></SettledReplyStatus>
  </ChatThread>;
});

function BackfillSkeleton() {
  return (
    <div
      className="flex w-full flex-col gap-md py-lg"
      data-testid="backfill-skeleton"
    >
      <output className="sr-only">{BACKFILL_LABEL}</output>
      <Skeleton className="h-12 w-full rounded-md" />
      <Skeleton className="h-12 w-3/4 rounded-md" />
      <Skeleton className="h-12 w-2/3 rounded-md" />
    </div>
  );
}

function ChatHistory({ eventsCount, connectionStatus }: Pick<FleetThreadViewportProps, "eventsCount" | "connectionStatus">) {
  const isAwaitingFirstFrames =
    eventsCount === 0 &&
    (connectionStatus === CONNECTION_STATUS.CONNECTING ||
      connectionStatus === CONNECTION_STATUS.RECONNECTING);
  const isIdleEmpty = eventsCount === 0 && connectionStatus === CONNECTION_STATUS.LIVE;
  return (
    // A log is a polite live region by default, and a streamed reply rewrites
    // it on every flush — a screen reader re-read the growing answer each
    // time. The log stays quiet; a settled reply is announced once, by the
    // status beside it.
    <div
      role="log"
      aria-live="off"
      aria-label={PANEL_TITLE}
      // `flex-1`, not `min-h-full`: the log takes only the height left beside
      // the footer, so a short thread does not scroll and the sticky composer
      // never covers its newest rows. Top-aligned, as the viewport's top
      // anchor expects: a bottom-aligned short thread slid down by the height
      // of every Thought that folded above the composer.
      className="mx-auto flex w-full max-w-measure flex-1 flex-col justify-start py-lg"
    >
      {isAwaitingFirstFrames ? <BackfillSkeleton /> : null}
      {isIdleEmpty ? (
        <p className="px-sm py-lg text-sm text-muted-foreground">{EMPTY_HINT}</p>
      ) : null}
      <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
    </div>
  );
}
