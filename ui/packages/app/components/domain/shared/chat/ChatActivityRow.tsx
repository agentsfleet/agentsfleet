"use client";

import type {ReactNode} from "react";
import {Accordion, AccordionContent, AccordionItem, AccordionTrigger, cn} from "@agentsfleet/design-system";
import {BellIcon, ClockClockwiseIcon, ClockIcon, FileTextIcon} from "@phosphor-icons/react/ssr";
import {FleetFailedOutcome, FLEET_OUTCOME_CLASS} from "../../activity/messages/FleetFailedOutcome";
import {Timestamp} from "../identity/FleetTimestamp";

export type ChatActivityRowProps = {
  sender: string; title?: string; wokeOnSchedule?: boolean; headline: string; createdAt: Date; messageRole: string;
  outcome?: string; failed?: boolean; annotation?: ReactNode; detailsLabel?: string; children?: ReactNode;
};

/** An activity uses one icon, a short heading and the same local clock as turns. */
export function ChatActivityRow({sender, title, wokeOnSchedule, headline, createdAt, messageRole, outcome, failed, annotation, detailsLabel = "View event", children}: ChatActivityRowProps) {
  const Icon = wokeOnSchedule ? ClockClockwiseIcon : title ? ClockIcon : BellIcon;
  return <div data-role={messageRole} data-compact="true" data-failed={failed || undefined}
    className={cn("flex w-full min-w-0 items-start gap-md text-muted-foreground", title ? "my-2.5 text-label font-medium leading-label max-[680px]:[&_time]:hidden [&_time]:relative [&_time]:top-xs [&_time]:font-medium [&_time]:leading-label" : "py-lg text-conversation leading-conversation")}>
    <Icon size={16} className="mt-xs shrink-0" aria-hidden="true" />
    <div className="min-w-0 flex-1">
      <div className="flex items-start justify-between gap-lg"><p className="min-w-0 break-words">
        <span className={title ? "font-title" : "font-semibold"}>{title ?? sender}</span>{title ? null : <> — <span>{headline}</span></>}
      </p><Timestamp createdAt={createdAt} compact /></div>
      {title ? <p className="break-words font-medium text-muted-foreground">{headline}</p> : null}
      {outcome ? <div className="mt-xs">{failed ? <FleetFailedOutcome>{outcome}</FleetFailedOutcome> : <p className={FLEET_OUTCOME_CLASS}>{outcome}</p>}</div> : null}
      {annotation ? <div className="mt-sm">{annotation}</div> : null}
      {children ? <Accordion type="single" collapsible><AccordionItem value="details" className="border-0">
        <AccordionTrigger className="min-h-9 w-fit gap-sm py-sm text-label hover:no-underline">
          <FileTextIcon size={14} aria-hidden="true" />{detailsLabel}
        </AccordionTrigger><AccordionContent>{children}</AccordionContent>
      </AccordionItem></Accordion> : null}
    </div>
  </div>;
}
