"use client";

import type { ReactNode } from "react";
import { ThreadPrimitive } from "@assistant-ui/react";
import { ArrowDownIcon } from "lucide-react";
import { Button, cn } from "@agentsfleet/design-system";

/** The viewport owns scrolling; its capped footer keeps the composer reachable. */
export function ChatThread({children, footer, after, label, centered, testId}: {
  children: ReactNode; footer: ReactNode; after?: ReactNode; label: string; centered?: boolean; testId?: string;
}) {
  return <ThreadPrimitive.Root data-testid={testId} aria-label={label}
    className="relative flex min-h-0 flex-1 flex-col overflow-clip bg-background">
    <ThreadPrimitive.Viewport turnAnchor="top" autoScroll role="presentation"
      className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto px-lg sm:px-xl", centered && "justify-center")}>
      {children}
      <ThreadPrimitive.ViewportFooter data-testid="fleet-chat-footer"
        className="sticky bottom-0 mx-auto flex max-h-full w-full max-w-measure flex-col bg-background pb-md pt-md">
        <ThreadPrimitive.ScrollToBottom asChild>
          <Button variant="secondary" size="icon" aria-label="Jump to latest"
            className="absolute bottom-full left-1/2 z-20 mb-sm -translate-x-1/2 rounded-full disabled:pointer-events-none disabled:invisible">
            <ArrowDownIcon className="size-4" aria-hidden="true" />
          </Button>
        </ThreadPrimitive.ScrollToBottom>
        {footer}
      </ThreadPrimitive.ViewportFooter>
      {after}
    </ThreadPrimitive.Viewport>
  </ThreadPrimitive.Root>;
}
