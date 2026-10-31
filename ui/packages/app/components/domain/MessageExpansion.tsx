"use client";

import type { ReactNode } from "react";
import { useCallback, useId, useState } from "react";
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react";
import { Button } from "@agentsfleet/design-system";

const LONG_MESSAGE_CHARACTERS = 1000;
const MESSAGE_PREVIEW_CHARACTERS = 600;

/** Historical essays fold; a reply already being read stays open when streaming ends. */
export function MessageExpansion({length, running = false, children}: {length: number; running?: boolean; children: (display: (text: string) => string) => ReactNode}) {
  const id = useId();
  const [expanded, setExpanded] = useState(running);
  if (running && !expanded) setExpanded(true);
  const canFold = !running && length > LONG_MESSAGE_CHARACTERS;
  const collapsed = canFold && !expanded;
  const display = useCallback((text: string) => collapsed ? `${text.slice(0, MESSAGE_PREVIEW_CHARACTERS).trimEnd()}…` : text, [collapsed]);
  const Icon = expanded ? ChevronUpIcon : ChevronDownIcon;
  return <>
    <div id={id}>{children(display)}</div>
    {canFold ? <Button variant="ghost" size="sm" className="mt-sm w-fit gap-xs px-0 text-muted-foreground" aria-expanded={expanded} aria-controls={id} onClick={() => setExpanded(value => !value)}>
      {expanded ? "Show less" : "Show more"}<Icon className="size-3.5" aria-hidden="true" />
    </Button> : null}
  </>;
}
