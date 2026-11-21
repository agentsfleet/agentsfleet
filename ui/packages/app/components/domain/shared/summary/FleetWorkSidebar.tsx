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
  return <Card asChild className="fleet-summary min-h-0 shrink-0 rounded-lg p-xl hover:border-border summary:p-lg summary-medium:p-xl">
    <aside aria-label="Summary">
      <h2 className={cn("flex items-center justify-between text-label font-semibold leading-label",
        currentWork ? "mb-md summary:mb-xl" : "mb-lg border-b border-border pb-lg")}>Summary<ListBulletsIcon size={16} className="text-muted-foreground" aria-hidden="true" /></h2>
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
