"use client";

import Link from "next/link";
import { ComposerPrimitive } from "@assistant-ui/react";
import { Alert, Button, Textarea, cn } from "@agentsfleet/design-system";
import type { DeliveryFailureKind } from "./useFleetDeliveryFailure";

const PLACEHOLDER = "Message this fleet…";
const SEND_LABEL = "Send ↵";
const SEND_HINT = "Enter to send";
const COMPOSER_LABEL = "Chat composer";
const SESSION_EXPIRED = "Your session expired. Sign in again before sending this message.";
const SEND_FAILED = "Message not sent.";
const SIGN_IN_LABEL = "Sign in";
const RETRY_LABEL = "Retry";

// The composer is the point of the page (the approved fleet-workspace design),
// so it is a surface rather than a one-line input: a bordered field that grows
// with the message, the send hint, and a send action that reads as the primary
// move. It never disables itself on the live feed's state — sending is an
// authenticated write that does not touch the stream.
export type SteerComposerProps = {
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
};

export function SteerComposer({ failureKind, onRetry }: SteerComposerProps) {
  return (
    <ComposerPrimitive.Root
      id="fleet-steer-composer"
      className={cn(
        "rounded-lg border border-border bg-card px-xl py-lg",
        "flex flex-col gap-md",
      )}
      aria-label={COMPOSER_LABEL}
    >
      <DeliveryFailureNotice failureKind={failureKind} onRetry={onRetry} />

      <div
        className={cn(
          "flex min-h-16 flex-col gap-xs",
          "sm:flex-row sm:items-end sm:gap-md",
        )}
      >
        <ComposerPrimitive.Input asChild placeholder={PLACEHOLDER}>
          <Textarea
            aria-label={PLACEHOLDER}
            rows={1}
            className={cn(
              "min-h-0 flex-1 resize-none border-0 bg-transparent px-0 py-md",
              "font-mono text-mono leading-mono text-foreground",
              "placeholder:text-muted-foreground",
              "focus-visible:border-0 focus-visible:ring-1 focus-visible:ring-pulse focus-visible:ring-offset-0",
            )}
          />
        </ComposerPrimitive.Input>
        <span className="pb-sm font-mono text-label text-muted-foreground">
          {SEND_HINT}
        </span>
        <ComposerPrimitive.Send asChild>
          <Button type="submit" variant="secondary" size="sm" className="min-h-11 sm:min-h-0">
            {SEND_LABEL}
          </Button>
        </ComposerPrimitive.Send>
      </div>
    </ComposerPrimitive.Root>
  );
}

function DeliveryFailureNotice({
  failureKind,
  onRetry,
}: {
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
}) {
  if (failureKind === "session") {
    return (
      <Alert variant="destructive" className="items-center justify-between">
        <span>{SESSION_EXPIRED}</span>
        <Button asChild type="button" variant="outline" size="sm">
          <Link href="/sign-in">{SIGN_IN_LABEL}</Link>
        </Button>
      </Alert>
    );
  }
  if (failureKind === "send") {
    return (
      <Alert variant="destructive" className="items-center justify-between">
        <span>{SEND_FAILED}</span>
        <Button type="button" variant="outline" size="sm" onClick={onRetry}>
          {RETRY_LABEL}
        </Button>
      </Alert>
    );
  }
  return null;
}
