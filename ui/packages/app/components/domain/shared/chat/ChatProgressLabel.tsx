"use client";

import { useEffect, useState } from "react";
import { CircleNotchIcon } from "@phosphor-icons/react/ssr";
import { MS_PER_SECOND } from "@/lib/utils";

/** The live label stays accessible while only its visual clock ticks. */
export function ChatProgressLabel({label, startedAtMs}: {label: string; startedAtMs?: number}) {
  const [observedAt] = useState(() => Date.now());
  return <span className="inline-flex min-w-0 items-center gap-sm">
    <CircleNotchIcon className="size-4 shrink-0 text-muted-foreground motion-safe:animate-spin" aria-hidden="true" data-chat-spinner="" />
    <span className="font-semibold" data-chat-loading="true">{label}</span>
    <span aria-hidden="true" className="text-muted-foreground">·</span>
    <ElapsedSeconds startedAtMs={startedAtMs ?? observedAt} />
  </span>;
}

function ElapsedSeconds({startedAtMs}: {startedAtMs: number}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), MS_PER_SECOND);
    return () => clearInterval(timer);
  }, []);
  const seconds = Math.floor(Math.max(0, now - startedAtMs) / MS_PER_SECOND);
  return <span aria-hidden="true" data-waiting-elapsed="" className="shrink-0 tabular-nums text-muted-foreground">{seconds}s</span>;
}
