"use client";

import type { ReactNode } from "react";
import { useId, useRef, useState } from "react";
import { ClockClockwiseIcon, WarningCircleIcon } from "@phosphor-icons/react/ssr";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Alert, Button } from "@agentsfleet/design-system";

/** Setup and saved-fleet failures use the same quiet, actionable message. */
export function ChatFailure({title, description, note, details, onRetry, disabled}: {
  title: string; description: string; note?: string; details?: ReactNode;
  onRetry?: () => void | Promise<void>; disabled?: boolean;
}) {
  const titleId = useId();
  const [retrying, setRetrying] = useState(false);
  const [retryError, setRetryError] = useState(false);
  const pending = useRef(false);
  const retry = async (send: () => void | Promise<void>): Promise<void> => {
    if (pending.current) return;
    pending.current = true;
    setRetryError(false);
    setRetrying(true);
    try { await send(); } catch { setRetryError(true); }
    finally { pending.current = false; setRetrying(false); }
  };
  return <Alert variant="destructive" aria-labelledby={titleId} className="my-lg block border-0 bg-transparent p-0 text-label text-muted-foreground">
    <span id={titleId} className="sr-only">{title}</span>
    <div className="flex flex-wrap items-center gap-x-lg gap-y-sm">
      <WarningCircleIcon className="size-3.5 shrink-0 text-destructive" aria-hidden="true" />
      <p>{description}</p>
      {onRetry ? <Button variant="link" size="sm" className="min-h-8 px-sm text-primary pointer-coarse:min-h-11"
        disabled={disabled || retrying} onClick={() => {void retry(onRetry);}}>
        <ClockClockwiseIcon size={16} aria-hidden="true" />{retrying ? "Trying again…" : "Try again"}
      </Button> : null}
    </div>
    {note ? <p className="mt-sm text-eyebrow leading-eyebrow text-muted-foreground">{note}</p> : null}
      {details ? <Accordion type="single" collapsible className="min-w-0">
        <AccordionItem value="details" className="border-0">
          <AccordionTrigger className="w-fit flex-none justify-start gap-sm py-0 text-eyebrow leading-conversation font-normal text-muted-foreground pointer-coarse:min-h-11 [&>svg]:order-first [&>svg]:ml-0 [&>svg]:size-3 [&>svg]:-rotate-90 [&[data-state=open]>svg]:rotate-0">View details</AccordionTrigger>
          <AccordionContent className="break-words pt-sm text-label text-muted-foreground">{details}</AccordionContent>
        </AccordionItem>
      </Accordion> : null}
    {retryError ? <p className="text-label text-destructive">Couldn’t retry this request. Try again.</p> : null}
  </Alert>;
}
