"use client";

import { useSyncExternalStore, type ReactNode } from "react";
import { Button, Dialog, DialogContent, DialogTitle, DialogDescription, DialogTrigger } from "@agentsfleet/design-system";

const QUERY = "(min-width: 1024px)";
function subscribe(changed: () => void) {
  const media = window.matchMedia(QUERY);
  media.addEventListener("change", changed);
  return () => media.removeEventListener("change", changed);
}
const desktopSnapshot = () => window.matchMedia(QUERY).matches;
const serverSnapshot = () => false;

/** The same authenticated work reader mounts in one surface, never twice. */
export function FleetWorkSidebar({children}: {children: ReactNode}) {
  const desktop = useSyncExternalStore(subscribe, desktopSnapshot, serverSnapshot);
  if (!children) return null;
  return desktop ? <aside aria-label="Work context" className="min-h-0 w-80 shrink-0 overflow-y-auto overscroll-contain border-l border-border px-lg py-lg">{children}</aside>
    : <div className="absolute right-lg top-sm z-20"><Dialog>
      <DialogTrigger asChild><Button variant="outline" size="sm">Work context</Button></DialogTrigger>
      <DialogContent className="left-auto right-0 top-0 h-dvh max-h-dvh w-[min(24rem,100vw)] translate-x-0 translate-y-0 rounded-none p-lg pt-3xl">
        <DialogTitle className="text-label">Work context</DialogTitle>
        <DialogDescription className="text-eyebrow">Inspect saved progress here. Send questions and directions in the conversation.</DialogDescription>
        {children}
      </DialogContent>
    </Dialog></div>;
}

/** Keep rapidly changing context beside, rather than inside, the memoized conversation. */
export function FleetConversationLayout({children, work}: {children: ReactNode; work?: ReactNode}) {
  return <div className="relative flex min-h-0 flex-1" data-testid="fleet-conversation-layout">
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">{children}</div>
    <FleetWorkSidebar>{work}</FleetWorkSidebar>
  </div>;
}
