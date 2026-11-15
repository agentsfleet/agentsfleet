"use client";

import { useMemo } from "react";
import { TextAlignLeftIcon } from "@phosphor-icons/react/ssr";
import { CONTEXT_STATE, readContextProgress } from "@/lib/context-progress";

const LABEL = {
  [CONTEXT_STATE.summarizing]: "Compacting context · This can take a few minutes",
  [CONTEXT_STATE.resuming]: "Context compacted · Continuing…",
  [CONTEXT_STATE.interrupted]: "Context compaction interrupted.",
} as const;

/** Metadata announces the operation; generated notes stay on the server. */
export function ContextProgressStatus({ value }: { value: unknown }) {
  const progress = useMemo(() => readContextProgress(value), [value]);
  if (!progress) return null;
  return <output aria-live="polite" aria-atomic="true" className="inline-flex min-w-0 items-start gap-sm text-label text-muted-foreground">
    <TextAlignLeftIcon className="mt-xs size-4 shrink-0" aria-hidden="true" />
    <span data-chat-loading={progress.state === CONTEXT_STATE.summarizing}>{LABEL[progress.state]}</span>
  </output>;
}
