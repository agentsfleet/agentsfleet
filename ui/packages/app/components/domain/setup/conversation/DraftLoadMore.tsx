"use client";

import { useState, useTransition } from "react";
import { Alert, Button } from "@agentsfleet/design-system";

export type LoadMoreCopy = {readonly label: string; readonly failure: string};
export const LOAD_MORE_COPY = {
  models: {label: "Load more models", failure: "We couldn’t load more models. Try again in a moment."},
  chats: {label: "Load more chats", failure: "We couldn’t load more chats. Try again in a moment."},
} as const satisfies Record<string, LoadMoreCopy>;
const LOADING_LABEL = "Loading…";

/** The app's append control (as on the fleet wall): rows already shown stay when a page fails. */
export function DraftLoadMore({copy, onLoad, className}: {copy: LoadMoreCopy; onLoad: () => Promise<void>; className?: string}) {
  const [pending, startLoading] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const load = (): void => {
    setError(null);
    startLoading(async () => {
      // Lost access is the client's to show: it clears the lists, which removes this control.
      try { await onLoad(); } catch { setError(copy.failure); }
    });
  };
  return <div className={className}>
    <Button variant="ghost" size="sm" onClick={load} disabled={pending} aria-busy={pending}>{pending ? LOADING_LABEL : copy.label}</Button>
    {error ? <Alert variant="destructive" className="mt-sm">{error}</Alert> : null}
  </div>;
}
