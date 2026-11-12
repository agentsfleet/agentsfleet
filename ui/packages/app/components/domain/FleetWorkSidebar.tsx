"use client";

import { createContext, type ReactNode } from "react";
import { Card } from "@agentsfleet/design-system";
import { ListBulletsIcon } from "@phosphor-icons/react/ssr";

/** Presentation slot only; task readers and action owners remain unchanged. */
export const FleetSummaryContext = createContext<ReactNode>(null);

/** One mounted reader across breakpoints, inside the native thread viewport. */
export function FleetWorkSidebar({children}: {children: ReactNode}) {
  if (!children) return null;
  return <Card asChild className="fleet-summary min-h-0 shrink-0 rounded-lg p-xl hover:border-border summary:p-lg summary-medium:p-xl">
    <aside aria-label="Summary">
      <h2 className="mb-lg flex items-center justify-between border-b border-border pb-lg text-label font-semibold leading-label">Summary<ListBulletsIcon size={16} className="text-muted-foreground" aria-hidden="true" /></h2>
      {children}
    </aside>
  </Card>;
}

export function FleetConversationLayout({children, work}: {children: ReactNode; work?: ReactNode}) {
  return <FleetSummaryContext value={work}>
    <div className="fleet-conversation-layout relative flex min-h-0 flex-1 flex-col" data-testid="fleet-conversation-layout">{children}</div>
  </FleetSummaryContext>;
}
