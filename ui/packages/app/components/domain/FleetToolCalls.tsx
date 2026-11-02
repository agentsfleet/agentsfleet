"use client";

import type { ReactNode } from "react";
import type { ToolActivity } from "./tool-call-activity";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, List, SignalSpinner, cn } from "@agentsfleet/design-system";
import { CELL_STATE } from "./tool-call-copy";
import { ACTIVITY_ICONS, CELL_COPY, TRIGGER_CLASS } from "./tool-call-presentation";

export const TOOL_CALLS_LABEL = "Tool activity";
const COMMANDS = "commands";

/** Adjacent calls share one compact disclosure; each keeps its own identity. */
export function ToolCallList({children, activity}: {children: ReactNode; activity: ToolActivity}) {
  const running = activity.state === CELL_STATE.RUNNING;
  const Icon = ACTIVITY_ICONS[activity.kind];
  const label = [activity.label, activity.target].filter(Boolean).join(" ");
  return <Accordion type="single" collapsible className="mb-md">
    <AccordionItem value={COMMANDS} className="border-0">
      <AccordionTrigger className={TRIGGER_CLASS} aria-label={`${TOOL_CALLS_LABEL}: ${label}`} title={label}>
        <span aria-hidden="true" className={cn("inline-flex size-3.5 shrink-0 items-center justify-center", CELL_COPY[activity.state].tone)}>{running ? <SignalSpinner className="size-3.5 text-current" /> : <Icon className="size-3.5 shrink-0" />}</span>
        <span className="min-w-0 flex-1 text-left sm:truncate" role={running ? "status" : undefined}>
          <span className="block sm:inline">{activity.label}</span>{activity.target ? <> <span className="block truncate text-label text-text-dim sm:inline sm:text-inherit">{activity.target}</span></> : null}
        </span>
      </AccordionTrigger>
      <AccordionContent className="pb-0"><List variant="plain" aria-label={TOOL_CALLS_LABEL} className="flex flex-col gap-xs space-y-0">{children}</List></AccordionContent>
    </AccordionItem>
  </Accordion>;
}
