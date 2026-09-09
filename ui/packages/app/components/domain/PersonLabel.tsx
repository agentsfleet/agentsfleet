"use client";

import { useEffect, useSyncExternalStore } from "react";
import { Skeleton } from "@agentsfleet/design-system";
import { fallbackPersonLabel, isPersonId, systemLabel } from "@/lib/identity/person";
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
  const answer = usePersonName(actor);
  if (actor.length === 0) return null;
  const system = systemLabel(actor);
  if (system !== null) {
    return <span className={className} title={actor}>{system}</span>;
  }
  // Nothing to say yet. The shortened subject is what a directory that answered
  // "nobody by that id" leaves behind — not a placeholder to flash while it is
  // still being asked. Showing it first made every name in the table visibly
  // change a beat after it appeared.
  if (answer === undefined && isPersonId(actor)) {
    return <Skeleton className="inline-block h-4 w-24 align-middle" data-testid="person-loading" />;
  }
  return (
    <span className={className} title={actor}>
      {answer || fallbackPersonLabel(actor)}
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
function usePersonName(actor: string): string | undefined {
  useEffect(() => {
    requestName(actor);
  }, [actor]);
  const cached = useSyncExternalStore(
    subscribe,
    () => nameFor(actor),
    () => undefined,
  );
  // `undefined` is "nobody has asked yet"; an empty string is the directory
  // answering "no such subject". The caller tells the two apart — one waits,
  // the other falls back.
  return cached;
}
