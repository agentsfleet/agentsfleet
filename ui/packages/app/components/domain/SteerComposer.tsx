"use client";

import { useEffect } from "react";
import Link from "next/link";
import { ComposerPrimitive, useAui, useAuiState } from "@assistant-ui/react";
import { Alert, Button, DashboardPanel, Textarea, cn } from "@agentsfleet/design-system";
import { ArrowUpIcon } from "lucide-react";
import { DELIVERY_FAILURE, type FailedDelivery } from "./useFleetDeliveryFailure";

const PLACEHOLDER = "Message this fleet…";
const SEND_LABEL = "Send";
const COMPOSER_LABEL = "Chat composer";
const SESSION_EXPIRED = "Your session expired. Sign in again before sending this message.";
const SEND_FAILED = "Message not sent.";
const SIGN_IN_LABEL = "Sign in";
const RESEND_LABEL = "Resend";
const RESTORE_LABEL = "Restore";
// Restore puts the refused text above a draft typed since, a blank line
// apart, so neither is lost.
const DRAFT_SEPARATOR = "\n\n";

// The composer is a persistent part of the transcript: a compact, bordered
// field that grows with the message while leaving the visible conversation in
// place. It never disables itself on the live feed's state — sending is an
// authenticated write that does not touch the stream.
export type SteerComposerProps = {
  failure: FailedDelivery | null;
};

export function SteerComposer({ failure }: SteerComposerProps) {
  useRestoreRefusedText(failure);
  return (
    <DashboardPanel
      asChild
      padding="none"
      // `min-h-0` lets a capped chat footer shrink this box, never scroll it:
      // a scrolling composer anchored its draft and pushed the failure notice
      // out of view. Hover and focus follow the shared input boundary; the ring
      // tracks the draft field, so Send's own focus ring never doubles it.
      className="min-h-0 rounded-xl bg-card p-md hover:border-ring has-[textarea:focus-visible]:border-border-strong has-[textarea:focus-visible]:ring-2 has-[textarea:focus-visible]:ring-ring"
    >
      <ComposerPrimitive.Root
        id="fleet-steer-composer"
        className="flex flex-col gap-sm"
        aria-label={COMPOSER_LABEL}
      >
        <DeliveryFailureNotice failure={failure} />

        {/* Stretch, not end-alignment: when the footer is capped this row
            shrinks, and a stretched textarea shrinks with it and scrolls its
            own text. End-aligned, it kept its height and overflowed upward,
            out of view. Send keeps `self-end`. On touch, Send grows to 44 px,
            so the row floor and the textarea padding grow with it to keep one
            line level with the arrow. */}
        <div
          className={cn(
            "flex min-h-9 flex-row items-stretch gap-sm pointer-coarse:min-h-11",
            "sm:gap-md",
          )}
        >
          {/* The draft stops growing at 12rem or 30% of the window, whichever
              is lower, so on a short screen it yields height before the
              conversation above it disappears. */}
          <ComposerPrimitive.Input asChild placeholder={PLACEHOLDER} submitMode="enter">
            <Textarea
              aria-label={PLACEHOLDER}
              rows={1}
              className={cn(
                "field-sizing-content min-h-9 max-h-[min(12rem,30dvh)] flex-1 resize-none overflow-y-auto border-0 bg-transparent px-sm py-xs pointer-coarse:py-md",
                "text-reading leading-reading text-foreground",
                "placeholder:text-muted-foreground",
                "focus-visible:border-0 focus-visible:outline-none focus-visible:ring-0",
              )}
            />
          </ComposerPrimitive.Input>
          {/*
            * Send is an icon, and the word moves to the accessible name.
            *
            * A labelled button took ~86px of a 720px composer to say what the
            * arrow says in 36 — and the submit path an operator actually uses
            * is Enter, which `submitMode="enter"` already binds. Both ChatGPT
            * and Claude land on the same shape: measured on chatgpt.com, a
            * 36x36 round icon inset from the right edge of the composer.
            *
            * The name is unchanged for anyone not looking at it: the button
            * still answers to "Send".
            */}
          <ComposerPrimitive.Send asChild>
            <Button
              type="submit"
              variant="default"
              size="icon"
              aria-label={SEND_LABEL}
              className="shrink-0 self-end rounded-full"
            >
              <ArrowUpIcon size={16} aria-hidden="true" />
            </Button>
          </ComposerPrimitive.Send>
        </div>
      </ComposerPrimitive.Root>
    </DashboardPanel>
  );
}

// The composer clears on send, so a refusal lands in an empty field and the
// text goes back where the operator typed it; a mount after navigating away
// does the same. A draft started since is never overwritten.
function useRestoreRefusedText(failure: FailedDelivery | null): void {
  const aui = useAui();
  useEffect(() => {
    if (failure === null) return;
    const composer = aui.composer();
    if (composer.getState().isEmpty) composer.setText(failure.text);
  }, [aui, failure]);
}

function DeliveryFailureNotice({ failure }: { failure: FailedDelivery | null }) {
  if (failure === null) return null;
  if (failure.kind === DELIVERY_FAILURE.SESSION) {
    return (
      <Alert variant="destructive" className="items-center justify-between">
        <span>{SESSION_EXPIRED}</span>
        <Button asChild type="button" variant="outline" size="sm">
          <Link href="/sign-in">{SIGN_IN_LABEL}</Link>
        </Button>
      </Alert>
    );
  }
  return (
    <Alert variant="destructive" className="items-center justify-between">
      <span>{SEND_FAILED}</span>
      <SendFailureAction text={failure.text} />
    </Alert>
  );
}

// Resend is the composer's own Send, so it takes the one path every message
// takes. It shows once the composer holds the refused text; until then the
// operator is offered Restore, which puts it back without losing a new draft.
function SendFailureAction({ text }: { text: string }) {
  const aui = useAui();
  // A boolean, not the draft: typing does not re-render the notice.
  const holdsRefusedText = useAuiState((s) => s.composer.text.includes(text));
  if (holdsRefusedText) {
    return (
      <ComposerPrimitive.Send asChild>
        <Button type="submit" variant="outline" size="sm">
          {RESEND_LABEL}
        </Button>
      </ComposerPrimitive.Send>
    );
  }
  const restore = () => {
    const composer = aui.composer();
    const draft = composer.getState().text;
    composer.setText(draft.length === 0 ? text : `${text}${DRAFT_SEPARATOR}${draft}`);
  };
  return (
    <Button type="button" variant="outline" size="sm" onClick={restore}>
      {RESTORE_LABEL}
    </Button>
  );
}
