"use client";

import { use, useRef, useState, type ReactNode } from "react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { ArrowDownIcon } from "@phosphor-icons/react/ssr";
import { Button, Section, cn } from "@agentsfleet/design-system";

import { FleetSummaryContext, FleetWorkSidebar } from "../summary/FleetWorkSidebar";
import { useOpenAtLatest } from "./useOpenAtLatest";

/**
 * The viewport owns scrolling; its capped footer keeps the composer reachable.
 * A chat opens at its latest turn, and after that only the reader moves the
 * view: the library's top anchor lifts a sent turn to the top over a reserve
 * sized to the viewport, and a streamed reply, a resize or the composer never
 * scrolls it. Jump to latest follows a reply that runs below the composer.
 */
export function ChatThread({children, footer, label, testId, centered = false}: {
  children: ReactNode; footer: ReactNode; label: string; testId?: string; centered?: boolean;
}) {
  const summary = use(FleetSummaryContext);
  const viewport = useRef<HTMLDivElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const [jumps, setJumps] = useState(0);
  useOpenAtLatest(viewport, log, jumps);
  return <ThreadPrimitive.Root asChild data-testid={testId}>
    <Section aria-label={label} className={cn("relative flex min-h-0 flex-1 flex-col overflow-clip bg-background",
      centered && "[--max-w-chat:var(--max-w-new-fleet)] [--chat-input-height:var(--new-fleet-input-height)] summary:[--chat-inset:var(--sp-3xl)]")}>
      <ThreadPrimitive.Viewport ref={viewport} turnAnchor="top" autoScroll={false} scrollToBottomOnInitialize={false}
        role="presentation" className={cn("fleet-thread-viewport flex min-h-0 flex-1 flex-col overflow-y-auto", summary &&
          "summary:grid summary:grid-cols-[minmax(0,1fr)_var(--summary-width)] summary:grid-rows-[1fr_auto] summary:gap-x-lg summary:pr-lg summary-medium:gap-x-xl summary-medium:pr-xl summary-wide:gap-x-3xl summary-wide:pr-2xl")}>
        <div className={cn("flex min-w-0 flex-1 flex-col",
          centered && "mt-4xl flex-none sm:mt-auto sm:-translate-y-xl", summary && "summary:contents")}>
          <div ref={log} className={cn("fleet-thread-log flex min-w-0 flex-1 flex-col",
            centered ? "px-5 summary:px-chat-inset" : "px-xl pt-xl sm:px-chat-inset sm:pt-2xl",
            summary && "summary:col-start-1 summary:row-start-1")}>{children}</div>
          <FleetWorkSidebar>{summary}</FleetWorkSidebar>
        </div>
        {/* The composer overlays the log. A library ViewportFooter would count the rows beneath it as read. */}
        <div data-testid="fleet-chat-footer" className={cn("z-10 flex max-h-[calc(100%_-_3rem_-_var(--sp-sm))] shrink-0 flex-col bg-background summary:px-chat-inset",
          centered ? "relative mb-2xl px-5 pt-3xl sm:mb-auto sm:-translate-y-xl" : "sticky bottom-0 px-lg pb-5 pt-lg",
          summary && "summary:col-start-1 summary:row-start-2 summary:max-h-[calc(100dvh_-_9rem)]")}>
          <div className="relative mx-auto flex w-full max-w-chat flex-col">
            {!centered && <ThreadPrimitive.ScrollToBottom asChild behavior="instant" onClick={() => setJumps(count => count + 1)}>
              <Button variant="secondary" size="icon" aria-label="Jump to latest" className="absolute bottom-full left-1/2 z-20 mb-sm -translate-x-1/2 rounded-full disabled:pointer-events-none disabled:invisible">
                <ArrowDownIcon className="size-4" aria-hidden="true" />
              </Button>
            </ThreadPrimitive.ScrollToBottom>}
            {footer}
          </div>
        </div>
      </ThreadPrimitive.Viewport>
    </Section>
  </ThreadPrimitive.Root>;
}
