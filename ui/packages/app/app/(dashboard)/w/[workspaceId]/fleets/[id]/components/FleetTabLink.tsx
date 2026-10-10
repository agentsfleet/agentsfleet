"use client";

import type { ComponentProps, ReactNode } from "react";
import Link, { useLinkStatus } from "next/link";
import { cn } from "@agentsfleet/design-system";

// A fleet view is a `?view=` query on one server-rendered page, so a click
// waits on the server before anything moves. The tab the user clicked says so
// at once: its label brightens and pulses until the view arrives, while the
// underline stays on the view still showing. `TabNav` takes this as its link.
const PENDING_TAB_CLASS = "has-data-[pending=true]:text-foreground";

export function FleetTabLink({ className, children, ...props }: ComponentProps<typeof Link>) {
  return (
    <Link {...props} className={cn(className, PENDING_TAB_CLASS)}>
      <PendingLabel>{children}</PendingLabel>
    </Link>
  );
}

/** `useLinkStatus` answers only inside the link it belongs to. */
function PendingLabel({ children }: { children: ReactNode }) {
  const { pending } = useLinkStatus();
  return (
    <span data-pending={pending ? "true" : undefined} className={cn(pending && "animate-pulse")}>
      {children}
    </span>
  );
}
