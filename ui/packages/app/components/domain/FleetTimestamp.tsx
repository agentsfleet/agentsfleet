"use client";

import { createContext, use, useEffect, useMemo, useState, type ReactNode } from "react";
import { Time, formatTimeAbsolute, formatTimeClockUtc, formatTimeRangeUtc, formatTimeRelative } from "@agentsfleet/design-system";

// When a row happened, relative to one clock every row shares. Split out of
// `FleetMessageRow` at its length cap.

const RELATIVE_TIME_REFRESH_MS = 30_000;
const RelativeNowContext = createContext<Date | null>(null);

/** Calendar boundaries use the same UTC frame as the compact message clock. */
export function ConversationDate({createdAt, previous}: {createdAt: Date; previous?: Date}) {
  if (Number.isNaN(createdAt.getTime()) || (previous && !Number.isNaN(previous.getTime()) && createdAt.toISOString().slice(0, 10) === previous.toISOString().slice(0, 10))) return null;
  return <div data-conversation-date="" className="mb-xl text-center text-eyebrow leading-eyebrow text-text-dim"><Time value={createdAt} label={formatTimeRangeUtc(createdAt, createdAt)} tooltip={false} /></div>;
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
  const absolute = useMemo(() => formatTimeAbsolute(createdAt, "en-US", compact ? "UTC" : undefined) + (compact ? " UTC" : ""), [createdAt, compact]);
  const label = compact ? formatTimeClockUtc(createdAt) : now ? formatTimeRelative(createdAt, now) : undefined;

  return (
    <>
      <Time
        aria-hidden="true"
        value={createdAt}
        format={compact ? "absolute" : "relative"}
        label={label}
        tooltip={false}
        title={absolute}
        className="shrink-0 font-sans text-eyebrow leading-eyebrow text-text-dim"
      />
      <span className="sr-only">Occurred {absolute}</span>
    </>
  );
}
