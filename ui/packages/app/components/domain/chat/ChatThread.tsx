"use client";

import { use, type ReactNode } from "react";
import { ThreadPrimitive, useAuiState } from "@assistant-ui/react";
import { ArrowDownIcon } from "@phosphor-icons/react/ssr";
import { Button, Section, cn } from "@agentsfleet/design-system";

import { FleetSummaryContext, FleetWorkSidebar } from "../FleetWorkSidebar";

/** The viewport owns scrolling; its capped footer keeps the composer reachable. */
export function ChatThread({children, footer, label, testId, autoScroll = true, centered = false}: {
  children: ReactNode; footer: ReactNode; label: string; testId?: string; autoScroll?: boolean; centered?: boolean;
}) {
  const summary = use(FleetSummaryContext);
  const composing = useAuiState(state => state.composer.text.length > 0);
  return <ThreadPrimitive.Root asChild data-testid={testId}>
    <Section aria-label={label} className="relative flex min-h-0 flex-1 flex-col overflow-clip bg-background">
    <ThreadPrimitive.Viewport turnAnchor="top" autoScroll={autoScroll && !composing} scrollToBottomOnInitialize={autoScroll} scrollToBottomOnThreadSwitch={autoScroll} role="presentation"
      className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto", summary && "lg:grid lg:grid-cols-[minmax(0,1fr)_var(--summary-width)] lg:grid-rows-[1fr_auto] lg:gap-x-3xl lg:pr-2xl")}>
      <div className={cn("flex min-w-0 flex-col", centered ? "mt-auto flex-none" : "flex-1", summary && "lg:contents")}>
        <div className={cn("flex min-w-0 flex-1 flex-col px-xl pt-xl sm:px-chat-inset sm:pt-2xl", summary && "lg:col-start-1 lg:row-start-1")}>{children}</div>
        <FleetWorkSidebar>{summary}</FleetWorkSidebar>
      </div>
      <ThreadPrimitive.ViewportFooter data-testid="fleet-chat-footer"
        className={cn("flex max-h-[calc(100%_-_3rem_-_var(--sp-sm))] shrink-0 flex-col bg-background px-xl pb-5 pt-lg sm:px-chat-inset", centered ? "relative mb-auto" : "sticky bottom-0", summary && "lg:col-start-1 lg:row-start-2 lg:max-h-[calc(100dvh_-_9rem)]")}>
        <div className="relative mx-auto flex w-full max-w-chat flex-col">
        {autoScroll ? <ThreadPrimitive.ScrollToBottom asChild>
          <Button variant="secondary" size="icon" aria-label="Jump to latest"
            className="absolute bottom-full left-1/2 z-20 mb-sm -translate-x-1/2 rounded-full disabled:pointer-events-none disabled:invisible">
            <ArrowDownIcon className="size-4" aria-hidden="true" />
          </Button>
        </ThreadPrimitive.ScrollToBottom> : null}
        {footer}
        </div>
      </ThreadPrimitive.ViewportFooter>
    </ThreadPrimitive.Viewport>
    </Section>
  </ThreadPrimitive.Root>;
}
