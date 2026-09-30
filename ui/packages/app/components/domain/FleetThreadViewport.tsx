"use client";

import { memo } from "react";
import { ArrowDownIcon } from "lucide-react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { Button, Skeleton, cn } from "@agentsfleet/design-system";
import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";
import { SteerComposer, type SteerComposerProps } from "./SteerComposer";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { SettledReplyStatus } from "./FleetReplyBody";

const PANEL_TITLE = "Chat";
const EMPTY_HINT = "Message this fleet or wait for its next trigger. Activity and outcomes appear here.";
const JUMP_TO_LATEST = "Jump to latest";
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
          (see SteerComposer) instead of pushing Send out of view.
          The top anchor is the one scroller on a send: while a reply runs it
          pins the operator's newest row to the top and holds room below the
          reply, so a fold or a settle cannot drag the view. `autoScroll`
          still follows the bottom where no anchor holds the view, as for a
          webhook's reply, which has no operator row to pin; the library
          leaves it off under a top anchor unless asked. */}
      <ThreadPrimitive.Viewport
        turnAnchor="top"
        autoScroll
        className="flex min-h-0 flex-1 flex-col overflow-y-auto px-lg sm:px-xl"
        role="presentation"
      >
        <SettledReplyStatus>
          <ChatHistory eventsCount={eventsCount} connectionStatus={connectionStatus} />
        </SettledReplyStatus>
        <ThreadPrimitive.ViewportFooter
          data-testid="fleet-chat-footer"
          className="sticky bottom-0 mx-auto flex max-h-full w-full max-w-measure flex-col bg-background pb-md pt-md"
        >
          {/* Laid over the history, above the composer, so nothing that
              appears here moves a row the reader is on. */}
          <div className="pointer-events-none absolute inset-x-0 bottom-full z-20 mb-sm flex flex-col items-center gap-sm">
            <JumpToLatest />
            <div className="pointer-events-auto w-full">
              <FleetConnectionNotice status={connectionStatus} onRetry={onRetry} />
            </div>
          </div>
          <SteerComposer
            pending={pending}
            onResend={onResend}
            onDismiss={onDismiss}
            onRestored={onRestored}
            onDraft={onDraft}
          />
        </ThreadPrimitive.ViewportFooter>
      </ThreadPrimitive.Viewport>
    </ThreadPrimitive.Root>
  );
});

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
          "pointer-events-auto rounded-full",
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
