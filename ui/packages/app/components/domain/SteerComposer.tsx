"use client";

import { useEffect, useMemo, useRef } from "react";
import Link from "next/link";
import { ComposerPrimitive, useAui, useAuiState } from "@assistant-ui/react";
import { Alert, Button, DashboardPanel, Textarea, cn } from "@agentsfleet/design-system";
import { ArrowUpIcon } from "lucide-react";
import { PENDING_SEND_STATE, type PendingSend } from "./useFleetPendingSends";
import { exceedsSteerLimit } from "./useFleetMessageDelivery";
import { STEER_MESSAGE_MAX_BYTES } from "@/lib/api/fleets-types";

const PLACEHOLDER = "Message this fleet…";
const SEND_LABEL = "Send";
const COMPOSER_LABEL = "Chat composer";
const SESSION_EXPIRED = "Your session expired. Sign in again before sending this message.";
const SEND_FAILED = "Message not sent.";
const SEND_UNCONFIRMED = "Couldn't confirm this message was sent.";
const SIGN_IN_LABEL = "Sign in";
const RESEND_LABEL = "Resend";
const NOTICES_LABEL = "Unsent messages";
const BYTE_COUNT = new Intl.NumberFormat("en-US");
const TOO_LONG = `Messages can be at most ${BYTE_COUNT.format(STEER_MESSAGE_MAX_BYTES)} bytes.`;

// The composer is a persistent part of the transcript: a compact, bordered
// field that grows with the message while leaving the visible conversation in
// place. It never disables itself on the live feed's state — sending is an
// authenticated write that does not touch the stream.
export type SteerComposerProps = {
  /** The fleet's ledger of unresolved sends; the notice lists every one not in flight. */
  pending: readonly PendingSend[];
  onResend: (operationId: string) => void;
  onDismiss: (operationId: string) => void;
  /** The composer put this send's text back on mount, so a Send of it unchanged is that send again. */
  onRestored: (operationId: string, text: string) => void;
};

export function SteerComposer({ pending, onResend, onDismiss, onRestored }: SteerComposerProps) {
  const unresolved = useMemo(
    () => pending.filter((entry) => entry.state !== PENDING_SEND_STATE.SENDING),
    [pending],
  );
  useRestoreRefusedTextOnMount(unresolved, onRestored);
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
        <PendingSendNotices entries={unresolved} onResend={onResend} onDismiss={onDismiss} />
        <DraftTooLongHint />

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

// A refusal while mounted needs nothing here: the handler rejects with
// `MessageNotSentError` and assistant-ui returns the draft itself. A remount
// starts a fresh composer, so the newest unresolved text comes back from the
// ledger — once, on mount, and only into an empty composer. Older entries stay
// in the notice, each with its own Resend.
function useRestoreRefusedTextOnMount(
  unresolved: readonly PendingSend[],
  onRestored: SteerComposerProps["onRestored"],
): void {
  const aui = useAui();
  const atMount = useRef({ unresolved, onRestored });
  useEffect(() => {
    const { unresolved: atMountEntries, onRestored: restoredAtMount } = atMount.current;
    const newest = atMountEntries.at(-1);
    if (newest === undefined) return;
    const composer = aui.composer();
    if (!composer.getState().isEmpty) return;
    composer.setText(newest.text);
    restoredAtMount(newest.operationId, newest.text);
  }, [aui]);
}

// Shown while the draft is longer than the daemon takes, so a Send that does
// nothing says why. A boolean, not the draft: typing does not re-render it.
function DraftTooLongHint() {
  const tooLong = useAuiState((s) => exceedsSteerLimit(s.composer.text));
  if (!tooLong) return null;
  return (
    // `info`, so it is announced politely: it appears while the operator types.
    <Alert variant="info">{TOO_LONG}</Alert>
  );
}

function PendingSendNotices({
  entries,
  onResend,
  onDismiss,
}: { entries: readonly PendingSend[] } & Pick<SteerComposerProps, "onResend" | "onDismiss">) {
  if (entries.length === 0) return null;
  return (
    <ul className="flex flex-col gap-xs" aria-label={NOTICES_LABEL}>
      {entries.map((entry) => (
        <li key={entry.operationId}>
          <PendingSendNotice entry={entry} onResend={onResend} onDismiss={onDismiss} />
        </li>
      ))}
    </ul>
  );
}

// One unresolved send. Resend posts the ledger record under its own operation
// id — never the composer's text — and clears a draft that is exactly that
// text, so Enter cannot send it a second time. A refused send and an
// unconfirmed one read differently, because they are: the server said no to
// the first, and nothing answered for the second.
function PendingSendNotice({
  entry,
  onResend,
  onDismiss,
}: { entry: PendingSend } & Pick<SteerComposerProps, "onResend" | "onDismiss">) {
  const aui = useAui();
  const resend = () => {
    const composer = aui.composer();
    if (composer.getState().text === entry.text) composer.setText("");
    onResend(entry.operationId);
  };
  const unconfirmed = entry.state === PENDING_SEND_STATE.UNKNOWN;
  return (
    <Alert
      variant={unconfirmed ? "warning" : "destructive"}
      className="items-center gap-sm"
      onDismiss={() => onDismiss(entry.operationId)}
    >
      <span className="min-w-0 flex-1 truncate">
        {sentenceFor(entry.state)} <span className="text-foreground">{entry.text}</span>
      </span>
      {entry.state === PENDING_SEND_STATE.SESSION ? (
        <Button asChild type="button" variant="outline" size="sm">
          <Link href="/sign-in">{SIGN_IN_LABEL}</Link>
        </Button>
      ) : null}
      {/* Offered after a sign-in too: once the session is back, a Resend is
          the way out, and a fresh 401 simply marks it again. */}
      <Button type="button" variant="outline" size="sm" onClick={resend}>
        {RESEND_LABEL}
      </Button>
    </Alert>
  );
}

function sentenceFor(state: PendingSend["state"]): string {
  switch (state) {
    case PENDING_SEND_STATE.SESSION:
      return SESSION_EXPIRED;
    case PENDING_SEND_STATE.UNKNOWN:
      return SEND_UNCONFIRMED;
    default:
      return SEND_FAILED;
  }
}
