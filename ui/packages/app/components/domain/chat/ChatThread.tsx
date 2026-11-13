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
    <Section aria-label={label} className={cn("relative flex min-h-0 flex-1 flex-col overflow-clip bg-background", centered && "[--max-w-chat:var(--max-w-new-fleet)] [--chat-input-height:var(--new-fleet-input-height)] min-[681px]:[--chat-inset:var(--sp-3xl)]")}>
    <ThreadPrimitive.Viewport turnAnchor="top" autoScroll={autoScroll && !composing && !centered} scrollToBottomOnInitialize={autoScroll && !centered} scrollToBottomOnThreadSwitch={autoScroll && !centered} role="presentation"
      className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto", summary && "summary:grid summary:grid-cols-[minmax(0,1fr)_var(--summary-width)] summary:grid-rows-[1fr_auto] summary:gap-x-lg summary:pr-lg summary-medium:gap-x-xl summary-medium:pr-xl summary-wide:gap-x-3xl summary-wide:pr-2xl")}>
      <div className={cn("flex min-w-0 flex-col", centered ? "mt-2xl flex-none sm:mt-auto sm:-translate-y-xl" : "flex-1", summary && "summary:contents")}>
        <div className={cn("flex min-w-0 flex-1 flex-col px-xl sm:px-chat-inset", !centered && "pt-xl sm:pt-2xl", summary && "summary:col-start-1 summary:row-start-1")}>{children}</div>
        <FleetWorkSidebar>{summary}</FleetWorkSidebar>
      </div>
      <ThreadPrimitive.ViewportFooter data-testid="fleet-chat-footer"
        className={cn("flex max-h-[calc(100%_-_3rem_-_var(--sp-sm))] shrink-0 flex-col bg-background px-xl sm:px-chat-inset", centered ? "relative mb-2xl pt-xl sm:mb-auto sm:-translate-y-xl sm:pt-3xl" : "sticky bottom-0 pb-5 pt-lg", summary && "summary:col-start-1 summary:row-start-2 summary:max-h-[calc(100dvh_-_9rem)]")}>
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
