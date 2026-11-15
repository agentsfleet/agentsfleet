"use client";

import { createContext, use, useEffect, useMemo, useState, type ReactNode } from "react";
import { Time, formatTimeAbsolute, formatTimeRelative, sameCalendarDay, useClientTimeReady } from "@agentsfleet/design-system";

// When a row happened, relative to one clock every row shares. Split out of
// `FleetMessageRow` at its length cap.

const RELATIVE_TIME_REFRESH_MS = 30_000;
const RelativeNowContext = createContext<Date | null>(null);

/** Date separators and message clocks share the reader's calendar. */
export function ConversationDate({createdAt, previous}: {createdAt: Date; previous?: Date}) {
  const ready = useClientTimeReady();
  if (!ready || Number.isNaN(createdAt.getTime()) || (previous && sameCalendarDay(createdAt, previous))) return null;
  return <div data-conversation-date="" className="mb-xl text-center text-eyebrow leading-eyebrow text-text-dim"><Time value={createdAt} format="date" tooltip={false} /></div>;
}

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
export function Timestamp({ createdAt, compact = false }: { createdAt: Date; compact?: boolean }) {
  const now = compact ? null : use(RelativeNowContext);
  const ready = useClientTimeReady();
  const absolute = useMemo(() => ready ? formatTimeAbsolute(createdAt) : "—", [createdAt, ready]);
  const label = !compact && now ? formatTimeRelative(createdAt, now) : undefined;

  return (
    <>
      <Time
        aria-hidden="true"
        value={createdAt}
        format={compact ? "minute" : "relative"}
        label={label}
        tooltip={false}
        title={absolute}
        className="shrink-0 font-sans text-eyebrow leading-eyebrow text-text-dim"
      />
      <span className="sr-only">Occurred {absolute}</span>
    </>
  );
}
