"use client";

import { useEffect, useSyncExternalStore } from "react";
import { fallbackPersonLabel, systemLabel } from "@/lib/identity/person";
import { nameFor, requestName, subscribe } from "@/lib/identity/person-directory";

/**
 * A person, by the name they go by.
 *
 * Wherever the dashboard knows only a Clerk subject — who approved a gate, who
 * closed one — this is what renders it. The subject itself never disappears:
 * it is the title, so the string that matches a log line or an API response is
 * one hover away from the name that means something to a human.
 *
 * Until the directory answers, the shortened subject stands in. A row that says
 * who decided must not go blank while a lookup is in flight.
 */
export function PersonLabel({
  actor,
  className,
}: {
  actor: string;
  className?: string;
}) {
  const resolved = usePersonName(actor);
  if (actor.length === 0) return null;
  return (
    <span className={className} title={actor}>
      {systemLabel(actor) ?? resolved ?? fallbackPersonLabel(actor)}
    </span>
  );
}

/**
 * The name the directory holds for a subject, asking for it if nobody has.
 *
 * Registration is an effect, not a render-time call: a lookup is a side effect
 * and belongs after the paint that showed the fallback. `requestName` ignores
 * anything that is not a Clerk subject, so the daemon's own sentinels are never
 * sent to a directory that has not heard of them.
 */
function usePersonName(actor: string): string | null {
  useEffect(() => {
    requestName(actor);
  }, [actor]);
  const cached = useSyncExternalStore(
    subscribe,
    () => nameFor(actor),
    () => undefined,
  );
  // An empty string is the directory answering "nobody by that subject"; the
  // caller falls back rather than rendering a blank cell.
  return cached ? cached : null;
}
