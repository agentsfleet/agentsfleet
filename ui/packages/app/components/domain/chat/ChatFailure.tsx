"use client";

import type { ReactNode } from "react";
import { useRef, useState } from "react";
import { WarningCircleIcon } from "@phosphor-icons/react/ssr";
import { Alert, Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button } from "@agentsfleet/design-system";

/** Setup and saved-fleet failures use the same quiet, actionable message. */
export function ChatFailure({title, description, note, details, onRetry, disabled}: {
  title: string; description: string; note?: string; details?: ReactNode;
  onRetry?: () => void | Promise<void>; disabled?: boolean;
}) {
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
  return <Alert variant="destructive" aria-label={title} className="block space-y-md rounded-md border-error-border bg-error-surface p-5 text-body-sm leading-body-sm text-muted-foreground">
    <div className="block space-y-md">
      <span className="flex items-center gap-md text-base font-semibold text-destructive"><WarningCircleIcon className="size-4 shrink-0 text-destructive" aria-hidden="true" />{title}</span>
      <span className="block">{description}</span>
    </div>
    {note ? <p className="text-eyebrow text-muted-foreground">{note}</p> : null}
    <div className="space-y-lg">
      {onRetry ? <Button disabled={disabled || retrying} onClick={() => {void retry(onRetry);}}>{retrying ? "Trying again…" : "Try again"}</Button> : null}
      {details ? <Accordion type="single" collapsible className="min-w-0">
        <AccordionItem value="details" className="border-0">
          <AccordionTrigger className="min-h-9 w-fit flex-none gap-sm py-0 text-eyebrow text-muted-foreground">View details</AccordionTrigger>
          <AccordionContent className="break-words pt-sm text-label text-muted-foreground">{details}</AccordionContent>
        </AccordionItem>
      </Accordion> : null}
    </div>
    {retryError ? <p className="text-label text-destructive">Couldn’t retry this request. Try again.</p> : null}
  </Alert>;
}
