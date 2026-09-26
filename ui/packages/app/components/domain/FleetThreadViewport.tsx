"use client";

import { useLayoutEffect } from "react";
import { ArrowDownIcon } from "lucide-react";
import { ThreadPrimitive, useThreadViewportStore } from "@assistant-ui/react";
import { Button, Skeleton, cn } from "@agentsfleet/design-system";
import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";
import type { DeliveryFailureKind } from "./useFleetDeliveryFailure";
import { SteerComposer } from "./SteerComposer";
import { renderFleetMessage } from "./fleetMessageRenderers";

const PANEL_TITLE = "Chat";
const EMPTY_HINT = "Message this fleet or wait for its next trigger. Activity and outcomes appear here.";
const JUMP_TO_LATEST = "Jump to latest";
const BACKFILL_LABEL = "Loading recent activity";

type FleetThreadViewportProps = {
  eventsCount: number;
  connectionStatus: ConnectionStatus;
  submittedMessageId: string | null;
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
};

export function FleetThreadViewport({
  eventsCount, connectionStatus, submittedMessageId, failureKind, onRetry,
}: FleetThreadViewportProps) {
  const viewport = useThreadViewportStore();
  // The external runtime stays steerable while the fleet runs, so its normal
  // run-start scroll event never fires. Only a newly submitted message pulls
  // the reader to the latest turn; background replies leave history alone.
  useLayoutEffect(() => {
    if (submittedMessageId) viewport.getState().scrollToBottom({ behavior: "instant" });
  }, [submittedMessageId, viewport]);
  return (
    // `overflow-clip`, never `overflow-hidden`: a hidden box is still a scroll
    // container, and focus or scroll anchoring during a long streamed reply
    // scrolled it, lifting the composer off the bottom over a blank band.
    // A clipped box cannot scroll; only the viewport inside it does.
    <ThreadPrimitive.Root
      data-testid="fleet-thread-root"
      className="relative flex min-h-0 flex-1 flex-col overflow-clip bg-background"
    >
      {/* assistant-ui's layout: the viewport is the only scroller, and the
          composer rides inside it in ViewportFooter, stuck to the bottom.
          Capped at the viewport's height, a tall draft shrinks the textarea
          (see SteerComposer) instead of pushing Send out of view. */}
      <ThreadPrimitive.Viewport
        autoScroll
        className="flex min-h-0 flex-1 flex-col overflow-y-auto px-lg sm:px-xl"
        role="presentation"
      >
        <ChatHistory eventsCount={eventsCount} connectionStatus={connectionStatus} />
        <ThreadPrimitive.ViewportFooter
          data-testid="fleet-chat-footer"
          className="sticky bottom-0 mx-auto flex max-h-full w-full max-w-measure flex-col bg-background pb-md pt-md"
        >
          <JumpToLatest />
          <SteerComposer failureKind={failureKind} onRetry={onRetry} />
        </ThreadPrimitive.ViewportFooter>
      </ThreadPrimitive.Viewport>
    </ThreadPrimitive.Root>
  );
}

// Rides in the sticky footer, just above the composer, so it stays in view
// while the history scrolls under it.
function JumpToLatest() {
  return (
    <ThreadPrimitive.ScrollToBottom asChild>
      <Button
        variant="secondary"
        size="icon"
        aria-label={JUMP_TO_LATEST}
        className={cn(
          "absolute bottom-full left-1/2 z-20 mb-sm -translate-x-1/2 rounded-full",
          "disabled:invisible disabled:pointer-events-none",
        )}
      >
        <ArrowDownIcon className="size-4" aria-hidden="true" />
      </Button>
    </ThreadPrimitive.ScrollToBottom>
  );
}

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
    <div
      role="log"
      aria-live="polite"
      aria-label={PANEL_TITLE}
      // `flex-1`, not `min-h-full`: the log takes only the height left beside
      // the footer, so a short thread does not scroll and the sticky composer
      // never covers its newest rows.
      className="mx-auto flex w-full max-w-measure flex-1 flex-col justify-end py-lg"
    >
      {isAwaitingFirstFrames ? <BackfillSkeleton /> : null}
      {isIdleEmpty ? (
        <p className="px-sm py-lg text-sm text-muted-foreground">{EMPTY_HINT}</p>
      ) : null}
      <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
    </div>
  );
}
