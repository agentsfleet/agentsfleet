"use client";

import { use, useRef, useState, type ReactNode } from "react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { ArrowDownIcon } from "@phosphor-icons/react/ssr";
import { Button, Section } from "@agentsfleet/design-system";

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
    <Section aria-label={label} className="fleet-thread" data-centered={centered || undefined} data-summary={summary ? true : undefined}>
      <ThreadPrimitive.Viewport ref={viewport} turnAnchor="top" autoScroll={false} scrollToBottomOnInitialize={false}
        role="presentation" className="fleet-thread-viewport">
        <div className="fleet-thread-content">
          <div ref={log} className="fleet-thread-log">{children}</div>
          <FleetWorkSidebar>{summary}</FleetWorkSidebar>
        </div>
        {/* The composer overlays the log. A library ViewportFooter would count the rows beneath it as read. */}
        <div data-testid="fleet-chat-footer" className="fleet-thread-footer">
          <div className="relative mx-auto flex w-full max-w-chat flex-col">
            {!centered && <ThreadPrimitive.ScrollToBottom asChild behavior="instant" onClick={() => setJumps(count => count + 1)}>
              <Button variant="secondary" size="icon" aria-label="Jump to latest" className="fleet-thread-jump">
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
