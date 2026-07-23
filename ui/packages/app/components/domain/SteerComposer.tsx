"use client";

import Link from "next/link";
import { ComposerPrimitive } from "@assistant-ui/react";
import { Alert, Button, DashboardPanel, Textarea, cn } from "@agentsfleet/design-system";
import type { DeliveryFailureKind } from "./useFleetDeliveryFailure";

const PLACEHOLDER = "Message this fleet…";
const SEND_LABEL = "Send";
const COMPOSER_LABEL = "Chat composer";
const SESSION_EXPIRED = "Your session expired. Sign in again before sending this message.";
const SEND_FAILED = "Message not sent.";
const SIGN_IN_LABEL = "Sign in";
const RETRY_LABEL = "Retry";

// The composer is a persistent part of the transcript: a compact, bordered
// field that grows with the message while leaving the visible conversation in
// place. It never disables itself on the live feed's state — sending is an
// authenticated write that does not touch the stream.
export type SteerComposerProps = {
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
};

export function SteerComposer({ failureKind, onRetry }: SteerComposerProps) {
  return (
    <DashboardPanel
      asChild
      padding="none"
      className="rounded-xl bg-card p-md focus-within:border-pulse/60 focus-within:ring-1 focus-within:ring-pulse/40"
    >
      <ComposerPrimitive.Root
        id="fleet-steer-composer"
        className="flex flex-col gap-sm"
        aria-label={COMPOSER_LABEL}
      >
        <DeliveryFailureNotice failureKind={failureKind} onRetry={onRetry} />

        <div
          className={cn(
            "flex min-h-12 flex-col gap-xs",
            "sm:flex-row sm:items-end sm:gap-md",
          )}
        >
          <ComposerPrimitive.Input asChild placeholder={PLACEHOLDER} submitMode="enter">
            <Textarea
              aria-label={PLACEHOLDER}
              rows={1}
              className={cn(
                "min-h-12 flex-1 resize-none border-0 bg-transparent px-sm py-sm",
                "text-body leading-body text-foreground",
                "placeholder:text-muted-foreground",
                "focus-visible:border-0 focus-visible:outline-none focus-visible:ring-0",
              )}
            />
          </ComposerPrimitive.Input>
          <ComposerPrimitive.Send asChild>
            <Button
              type="submit"
              variant="secondary"
              size="sm"
              className="h-10 min-h-10 shrink-0 self-end rounded-lg px-lg"
            >
              {SEND_LABEL}
            </Button>
          </ComposerPrimitive.Send>
        </div>
      </ComposerPrimitive.Root>
    </DashboardPanel>
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
