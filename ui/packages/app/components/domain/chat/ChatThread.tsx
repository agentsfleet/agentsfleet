"use client";

import type { ReactNode } from "react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { ArrowDownIcon } from "lucide-react";
import { Button, Section } from "@agentsfleet/design-system";

/** The viewport owns scrolling; its capped footer keeps the composer reachable. */
export function ChatThread({children, footer, after, label, testId, autoScroll = true}: {
  children: ReactNode; footer: ReactNode; after?: ReactNode; label: string; testId?: string; autoScroll?: boolean;
}) {
  return <ThreadPrimitive.Root asChild data-testid={testId}>
    <Section aria-label={label} className="relative flex min-h-0 flex-1 flex-col overflow-clip bg-background">
    <ThreadPrimitive.Viewport turnAnchor="top" autoScroll={autoScroll} scrollToBottomOnInitialize={autoScroll} scrollToBottomOnThreadSwitch={autoScroll} role="presentation"
      className="flex min-h-0 flex-1 flex-col overflow-y-auto px-xl sm:px-chat-inset">
      {children}
      <ThreadPrimitive.ViewportFooter data-testid="fleet-chat-footer"
        className="sticky bottom-0 mx-auto flex max-h-full w-full max-w-chat flex-col bg-background pb-xl pt-lg">
        {autoScroll ? <ThreadPrimitive.ScrollToBottom asChild>
          <Button variant="secondary" size="icon" aria-label="Jump to latest"
            className="absolute bottom-full left-1/2 z-20 mb-sm -translate-x-1/2 rounded-full disabled:pointer-events-none disabled:invisible">
            <ArrowDownIcon className="size-4" aria-hidden="true" />
          </Button>
        </ThreadPrimitive.ScrollToBottom> : null}
        {footer}
      </ThreadPrimitive.ViewportFooter>
      {after}
    </ThreadPrimitive.Viewport>
    </Section>
  </ThreadPrimitive.Root>;
}
