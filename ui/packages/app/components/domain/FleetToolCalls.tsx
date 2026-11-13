"use client";

import type { ReactNode } from "react";
import type { ToolActivity } from "./tool-call-activity";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, List, cn } from "@agentsfleet/design-system";
import { CheckCircleIcon } from "@phosphor-icons/react/ssr";
import { CELL_STATE } from "./tool-call-copy";
import { ACTIVITY_ICONS, CELL_COPY, TRIGGER_CLASS } from "./tool-call-presentation";

export const TOOL_CALLS_LABEL = "Tool activity";
const COMMANDS = "commands";

/** Adjacent calls share one compact disclosure; each keeps its own identity. */
export function ToolCallList({children, activity, footer}: {children: ReactNode; activity: ToolActivity; footer?: ReactNode}) {
  const running = activity.state === CELL_STATE.RUNNING;
  const Icon = activity.state === CELL_STATE.SUCCEEDED ? CheckCircleIcon : ACTIVITY_ICONS[activity.kind];
  const label = [activity.label, activity.target].filter(Boolean).join(" ");
  return <Accordion type="single" collapsible className="my-5 border-y border-border">
    <AccordionItem value={COMMANDS} className="border-0">
      <AccordionTrigger className={cn(TRIGGER_CLASS, "min-h-11 gap-md px-0 py-lg sm:min-h-11")} aria-label={`${TOOL_CALLS_LABEL}: ${label}`} title={label}>
        <span aria-hidden="true" className={cn("inline-flex size-4 shrink-0 items-center justify-center", running ? "text-muted-foreground" : CELL_COPY[activity.state].tone)}><Icon className="size-4 shrink-0" /></span>
        <span className="min-w-0 flex-1 text-left sm:truncate" role={running ? "status" : undefined}>
          <span className="block sm:inline" data-chat-loading={running}>{activity.label}</span>{activity.target ? <> <span className="block truncate font-mono text-label text-text-dim sm:inline sm:text-inherit">{activity.target}</span></> : null}
        </span>
      </AccordionTrigger>
      <AccordionContent className="pb-0"><List variant="plain" aria-label={TOOL_CALLS_LABEL} className="flex flex-col gap-xs space-y-0 sm:gap-0">{children}</List>{footer}</AccordionContent>
    </AccordionItem>
  </Accordion>;
}
