"use client";

import { useEffect, useRef } from "react";

import { RecentPaints } from "./RecentPaints";

// Browser submit-to-visible time for the chat: from the operator's submit to
// the first painted answer, reasoning or tool row.

export const FIRST_VISIBLE_MEASURE = "agentsfleet.chat.submit_to_first_visible";
// A turn's reply row is renamed once, when the pending id becomes the server's.
// The component remounts, but that is still one visible response.
const RECENT_PAINTS = 400;
const measuredPaints = new RecentPaints(RECENT_PAINTS);

/** Record browser submit-to-visible time after the first reply, reasoning, or tool paint. */
export function useFirstVisiblePaint(eventId: string, submittedAtMs: number | null, visible: boolean): void {
  const measuredEvent = useRef<string | null>(null);
  useEffect(() => {
    if (!visible || submittedAtMs === null || measuredEvent.current === eventId) return;
    const key = `${eventId}:${submittedAtMs}`;
    if (measuredPaints.has(key)) return;
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        if (measuredPaints.has(key)) return;
        measuredEvent.current = eventId;
        measuredPaints.add(key);
        performance.measure(FIRST_VISIBLE_MEASURE, { start: submittedAtMs, end: performance.now() });
      });
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [eventId, submittedAtMs, visible]);
}
