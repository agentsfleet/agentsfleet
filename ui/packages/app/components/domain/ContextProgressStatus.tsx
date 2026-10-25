"use client";

import { useMemo } from "react";
import { SignalSpinner } from "@agentsfleet/design-system";
import { CONTEXT_STATE, readContextProgress } from "@/lib/context-progress";

const LABEL = {
  [CONTEXT_STATE.summarizing]: "Context summarizing…",
  [CONTEXT_STATE.resuming]: "Context summarized, resuming…",
  [CONTEXT_STATE.interrupted]: "Context summarization interrupted.",
} as const;

/** Metadata announces the operation; generated notes stay on the server. */
export function ContextProgressStatus({ value }: { value: unknown }) {
  const progress = useMemo(() => readContextProgress(value), [value]);
  if (!progress) return null;
  return <output aria-live="polite" aria-atomic="true" className="inline-flex items-center gap-sm text-label text-muted-foreground">
    {progress.state === CONTEXT_STATE.summarizing ? <SignalSpinner className="text-pulse" /> : null}
    {LABEL[progress.state]}
  </output>;
}
