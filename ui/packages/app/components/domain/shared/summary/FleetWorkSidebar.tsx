"use client";

import { createContext, use, type ReactNode } from "react";
import { Card, cn } from "@agentsfleet/design-system";
import { ListBulletsIcon } from "@phosphor-icons/react/ssr";

/** Presentation slot only; task readers and action owners remain unchanged. */
export const FleetSummaryContext = createContext<ReactNode>(null);
export const CurrentWorkSummaryContext = createContext(false);

/** One mounted reader across breakpoints, inside the native thread viewport. */
export function FleetWorkSidebar({children}: {children: ReactNode}) {
  const currentWork = use(CurrentWorkSummaryContext);
  if (!children) return null;
  return <Card asChild className={cn("fleet-summary -order-1 m-lg min-h-0 max-h-none shrink-0 overflow-visible rounded-lg p-xl hover:border-border",
    "summary:sticky summary:top-2xl summary:order-none summary:col-start-2 summary:row-start-1 summary:row-span-2 summary:mx-0 summary:my-2xl summary:max-h-[calc(100dvh_-_12rem)] summary:w-[var(--summary-width)] summary:self-start summary:overflow-y-auto",
    currentWork && "summary:p-lg summary-medium:p-xl")}>
    <aside aria-label="Summary">
      <h2 className={cn("flex items-center justify-between text-label font-semibold leading-label",
        currentWork ? "mb-md summary:mb-xl" : "mb-xl border-b border-border pb-lg")}>Summary<ListBulletsIcon size={16} className="text-muted-foreground" aria-hidden="true" /></h2>
      {children}
    </aside>
  </Card>;
}

export function FleetConversationLayout({children, work, className, summary = "current-work"}: {children: ReactNode; work?: ReactNode; className?: string; summary?: "current-work" | "setup"}) {
  return <FleetSummaryContext value={work}>
    <CurrentWorkSummaryContext value={summary === "current-work"}>
      <div className={cn("fleet-conversation-layout relative flex min-h-0 flex-1 flex-col", className)} data-testid="fleet-conversation-layout">{children}</div>
    </CurrentWorkSummaryContext>
  </FleetSummaryContext>;
}
