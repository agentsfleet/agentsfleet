import type { ReactNode } from "react";
import { ListBulletsIcon } from "@phosphor-icons/react/ssr";

/** One mounted reader across breakpoints; layout never resets its work or draft. */
export function FleetWorkSidebar({children}: {children: ReactNode}) {
  if (!children) return null;
  return <aside aria-label="Summary" className="fleet-summary min-h-0 shrink-0 overflow-y-auto overscroll-contain rounded-lg border border-border bg-card p-xl">
    <h2 className="mb-xl flex items-center justify-between text-label font-semibold leading-label">Summary<ListBulletsIcon size={16} className="text-muted-foreground" aria-hidden="true" /></h2>
    {children}
  </aside>;
}

export function FleetConversationLayout({children, work}: {children: ReactNode; work?: ReactNode}) {
  return <div className="fleet-conversation-layout relative flex min-h-0 flex-1 flex-col lg:flex-row lg:gap-3xl" data-testid="fleet-conversation-layout">
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">{children}</div>
    <FleetWorkSidebar>{work}</FleetWorkSidebar>
  </div>;
}
