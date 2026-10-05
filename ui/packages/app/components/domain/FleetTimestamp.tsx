"use client";

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { Time, formatTimeAbsolute, formatTimeRelative } from "@agentsfleet/design-system";

// When a row happened, relative to one clock every row shares. Split out of
// `FleetMessageRow` at its length cap.

const RELATIVE_TIME_REFRESH_MS = 30_000;
const RelativeNowContext = createContext<Date | null>(null);

/** The shared "now" every row's relative time reads, refreshed on one timer. */
export function RelativeNowProvider({ children }: { children: ReactNode }) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(
      () => setNow(new Date()),
      RELATIVE_TIME_REFRESH_MS,
    );
    return () => window.clearInterval(timer);
  }, []);

  return <RelativeNowContext.Provider value={now}>{children}</RelativeNowContext.Provider>;
}

// Relative time stays visual-only so the 30-second refresh cannot repeatedly
// announce the live region. Assistive technology gets one stable exact instant.
export function Timestamp({ createdAt }: { createdAt: Date }) {
  const now = useContext(RelativeNowContext);
  const absolute = useMemo(() => formatTimeAbsolute(createdAt), [createdAt]);

  return (
    <>
      <Time
        aria-hidden="true"
        value={createdAt}
        format="relative"
        label={now ? formatTimeRelative(createdAt, now) : undefined}
        tooltip={false}
        title={absolute}
        className="shrink-0 font-mono text-label leading-mono text-muted-foreground tabular-nums"
      />
      <span className="sr-only">Occurred {absolute}</span>
    </>
  );
}
