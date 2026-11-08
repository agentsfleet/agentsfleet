"use client";

import { useChatSummary } from "./chat/ChatSummary";
import type { ReactNode } from "react";
import { useSyncExternalStore } from "react";
import { Button, Dialog, DialogContent, DialogTitle, DialogDescription, DialogTrigger } from "@agentsfleet/design-system";

const QUERY = "(min-width: 1024px)";
/** The same authenticated work reader mounts in one surface, never twice. */
export function FleetWorkSidebar({children}: {children: ReactNode}) {
  const desktop = useSyncExternalStore(subscribe, desktopSnapshot, serverSnapshot);
  const summary = useChatSummary();
  if (!children || (summary && !summary.open)) return null;
  return desktop ? <aside id="fleet-summary" aria-label={summary ? "Fleet summary" : "Work context"} className="min-h-0 w-80 shrink-0 overflow-y-auto overscroll-contain border-l border-border bg-card/50 px-lg py-lg motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-right-2 motion-safe:duration-stream">{summary ? <h2 className="mb-lg text-label font-semibold">Fleet summary</h2> : null}{children}</aside>
    : <div className="absolute right-lg top-sm z-20"><Dialog {...(summary ? {open: summary.open, onOpenChange: summary.setOpen} : {})}>
      {summary ? null : <DialogTrigger asChild><Button variant="outline" size="sm">Work context</Button></DialogTrigger>}
      <DialogContent onCloseAutoFocus={summary ? event => {event.preventDefault(); summary.trigger.current?.focus();} : undefined} id="fleet-summary" className="left-auto right-0 top-0 h-dvh max-h-dvh w-[min(24rem,100vw)] translate-x-0 translate-y-0 rounded-none p-lg pt-3xl">
        <DialogTitle className="text-label">{summary ? "Fleet summary" : "Work context"}</DialogTitle>
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

function subscribe(changed: () => void) {
  const media = window.matchMedia(QUERY);
  media.addEventListener("change", changed);
  return () => media.removeEventListener("change", changed);
}
function desktopSnapshot() {return window.matchMedia(QUERY).matches;}
function serverSnapshot() {return false;}
