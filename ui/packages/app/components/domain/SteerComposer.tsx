"use client";

import Link from "next/link";
import {
  ComposerPrimitive,
  QueueItemPrimitive,
} from "@assistant-ui/react";
import { Alert, Badge, Button, Textarea, cn } from "@agentsfleet/design-system";
import type { DeliveryFailureKind } from "./useFleetMessageQueue";

const PLACEHOLDER = "Message this fleet…";
const SEND_LABEL = "Send ↵";
const WORKING_HINT = "Working — new messages will queue.";

export type SteerComposerProps = {
  isRunning: boolean;
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
};

export function SteerComposer({
  isRunning,
  failureKind,
  onRetry,
}: SteerComposerProps) {
  return (
    <ComposerPrimitive.Root
      className={cn(
        "border-t border-border bg-card px-xl py-lg",
        "flex flex-col gap-md",
      )}
      aria-label="Chat composer"
    >
      <ComposerPrimitive.Queue>
        {({ queueItem }) => (
          <div
            key={queueItem.id}
            className="flex items-center gap-md rounded-md border border-border bg-muted/30 px-md py-sm"
          >
            <QueueItemPrimitive.Text className="min-w-0 flex-1 truncate font-mono text-sm" />
            <Badge variant="evidence">queued</Badge>
            <QueueItemPrimitive.Remove asChild>
              <Button type="button" variant="ghost" size="sm">Remove</Button>
            </QueueItemPrimitive.Remove>
          </div>
        )}
      </ComposerPrimitive.Queue>

      <DeliveryFailureNotice failureKind={failureKind} onRetry={onRetry} />
      {isRunning ? <p className="text-xs text-muted-foreground">{WORKING_HINT}</p> : null}

      <div
        className={cn(
          "flex flex-col gap-xs rounded-md border border-border bg-background",
          "sm:flex-row sm:items-end sm:gap-md",
          "px-md py-xs",
          "transition-colors duration-snap ease-snap",
          "focus-within:border-pulse",
        )}
      >
        <span
          aria-hidden="true"
          className="pb-xs font-mono text-mono text-muted-foreground focus-within:text-pulse"
        >
          ›
        </span>
        <ComposerPrimitive.Input asChild placeholder={PLACEHOLDER}>
          <Textarea
            rows={1}
            className={cn(
              "min-h-0 flex-1 resize-none border-0 bg-transparent px-0 py-md",
              "font-mono text-mono leading-mono text-foreground",
              "placeholder:text-muted-foreground",
              "focus-visible:border-0 focus-visible:ring-0 focus-visible:ring-offset-0",
            )}
          />
        </ComposerPrimitive.Input>
        <ComposerPrimitive.Send asChild>
          <Button type="submit" variant="secondary" size="sm">
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
        <span>Your session expired. Sign in again before sending this message.</span>
        <Button asChild type="button" variant="outline" size="sm">
          <Link href="/sign-in">Sign in</Link>
        </Button>
      </Alert>
    );
  }
  if (failureKind === "send") {
    return (
      <Alert variant="destructive" className="items-center justify-between">
        <span>Message not sent.</span>
        <Button type="button" variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </Alert>
    );
  }
  return null;
}
