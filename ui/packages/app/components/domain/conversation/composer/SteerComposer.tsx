"use client";

import { useEffect, useId, useMemo, useRef, type RefObject } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { ComposerPrimitive, useAui, useAuiState } from "@assistant-ui/react";
import { Alert, Button, List, ListItem } from "@agentsfleet/design-system";
import { ArrowUpIcon, SquareIcon } from "@phosphor-icons/react/ssr";
import { PENDING_SEND_STATE, type PendingSend } from "../delivery/useFleetPendingSends";
import { signInPath } from "@/lib/auth/sign-in-redirect";
import { chatComposerText } from "@/lib/chat-message-text";
import { ChatComposerFrame, ChatComposerHint } from "../../shared/chat/ChatComposerFrame";
import { useFleetSetup } from "../../setup/FleetSetupContext";
import { FleetModelSelect } from "../../models/FleetModelSelect";
import { AddUploadPreview, ComposerUploadAttachments } from "./UploadAttachments";
import type { TurnStop } from "./useTurnStop";

const PLACEHOLDER = "Message your fleet…";
const SEND_LABEL = "Send";
const STOP_LABEL = "Stop";
const COMPOSER_LABEL = "Chat composer";
const SESSION_EXPIRED = "Your session expired. Sign in again before sending this message.";
const SEND_FAILED = "Message not sent.";
const SEND_UNCONFIRMED = "Couldn't confirm this message was sent.";
const SEND_CONFLICT = "This message conflicts with one already sent.";
const SIGN_IN_LABEL = "Sign in";
const RESEND_LABEL = "Resend";
const SEND_AS_NEW_LABEL = "Send as new";
const NOTICES_LABEL = "Unsent messages";
// In flight, or dismissed: neither is the operator's to act on here.
const NOT_NOTICED: ReadonlySet<PendingSend["state"]> = new Set([PENDING_SEND_STATE.SENDING, PENDING_SEND_STATE.DISMISSED]);

// The composer is a persistent part of the transcript: a compact, bordered
// field that grows with the message while leaving the visible conversation in
// place. It never disables itself on the live feed's state — sending is an
// authenticated write that does not touch the stream.
export type SteerComposerProps = {
  modelLabel?: string;
  /** The fleet's ledger of unresolved sends; the notice lists every one not in flight. */
  pending: readonly PendingSend[];
  onResend: (operationId: string) => void;
  onDismiss: (operationId: string) => void;
  /** The composer put this send's text back on mount, so a Send of it unchanged is that send again. */
  onRestored: (operationId: string, text: string) => void;
  /** Every draft the composer holds, so a failed send's id follows only its own returned text. */
  onDraft: (text: string) => void;
  /** Stop for the turn this fleet is working on; absent where no turn can be stopped. */
  turnStop?: TurnStop;
};

export function SteerComposer({ pending, onResend, onDismiss, onRestored, onDraft, turnStop, modelLabel = "Saved model" }: SteerComposerProps) {
  const modelChanging = useFleetSetup()?.modelChanging ?? false;
  const unresolved = useMemo(
    () => pending.filter((entry) => !NOT_NOTICED.has(entry.state)),
    [pending],
  );
  useRestoreRefusedTextOnMount(unresolved, onRestored);
  // A notice unmounts under its own Resend or Dismiss, so focus goes back to
  // the draft rather than falling to the page.
  const draftRef = useRef<HTMLTextAreaElement>(null);
  return (
    <>
    <ChatComposerFrame id="fleet-steer-composer" label={COMPOSER_LABEL} placeholder={PLACEHOLDER} inputRef={draftRef} disabled={modelChanging}
      toolbar={<><AddUploadPreview /><FleetModelSelect label={modelLabel} /></>}
      actions={<>{turnStop?.visible ? <StopButton stop={turnStop} /> : null}<SendButton draftRef={draftRef} /></>}>
        <PendingSendNotices entries={unresolved} onResend={onResend} onDismiss={onDismiss} draftRef={draftRef} />
        <DraftReporter onDraft={onDraft} />
        <ComposerUploadAttachments />

    </ChatComposerFrame>
    <ChatComposerHint />
    </>
  );
}

// A refusal while mounted needs nothing here: the handler rejects with
// `MessageNotSentError` and assistant-ui returns the draft itself. A remount
// starts a fresh composer, so the newest unresolved text comes back from the
// ledger — once per ledger, and only into an empty composer. Once per ledger,
// not once per mount: the ledger is memory-only until the signed-in user is
// known, and the user's own ledger arrives with a new `onRestored`. Older
// entries stay in the notice, each with its own Resend.
function useRestoreRefusedTextOnMount(
  unresolved: readonly PendingSend[],
  onRestored: SteerComposerProps["onRestored"],
): void {
  const aui = useAui();
  const restoredFor = useRef<SteerComposerProps["onRestored"] | null>(null);
  useEffect(() => {
    if (restoredFor.current === onRestored) return;
    restoredFor.current = onRestored;
    const newest = unresolved.at(-1);
    if (newest === undefined) return;
    const composer = aui.composer();
    if (!composer.getState().isEmpty) return;
    composer.setText(newest.text);
    onRestored(newest.operationId, newest.text);
  }, [aui, onRestored, unresolved]);
}

/*
 * Send is an icon, and the word moves to the accessible name.
 *
 * A labelled button took ~86px of a 720px composer to say what the arrow says
 * in 36 — and the submit path an operator actually uses is Enter, which
 * `submitMode="enter"` already binds. Both ChatGPT and Claude land on the same
 * shape: measured on chatgpt.com, a 36x36 round icon inset from the right edge
 * of the composer. The name is unchanged for anyone not looking at it: the
 * button still answers to "Send".
 *
 * While the model changes it is disabled. The prop is passed only then: the
 * primitive's slot lets a child's `disabled` override its own, and `false`
 * would enable Send on an empty draft. Length is the daemon's to refuse, and
 * a refused send comes back through the pending-send notice.
 */
function SendButton({ draftRef }: { draftRef: RefObject<HTMLTextAreaElement | null> }) {
  const modelChanging = useFleetSetup()?.modelChanging ?? false;
  return (
    <ComposerPrimitive.Send asChild>
      <Button
        type="submit"
        variant="default"
        size="icon-row"
        aria-label={SEND_LABEL}
        className="shrink-0 self-end rounded-full disabled:border-transparent disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 [&_svg]:size-4.5"
        onClick={() => draftRef.current?.focus({preventScroll: true})}
        {...(modelChanging ? { disabled: true } : {})}
      >
        <ArrowUpIcon size={18} aria-hidden="true" />
      </Button>
    </ComposerPrimitive.Send>
  );
}

// Shown only while a turn of this fleet runs, beside Send rather than in its
// place: sending mid-run steers the run, so both stay available.
function StopButton({ stop }: { stop: TurnStop }) {
  return (
    <Button
      type="button"
      variant="secondary"
      size="icon-row"
      aria-label={STOP_LABEL}
      className="shrink-0 self-end rounded-full [&_svg]:size-4.5"
      disabled={stop.stopping}
      onClick={stop.stop}
    >
      <SquareIcon size={18} aria-hidden="true" />
    </Button>
  );
}

// Its own leaf: it reads the draft, so typing re-renders only this.
function DraftReporter({ onDraft }: Pick<SteerComposerProps, "onDraft">) {
  const draft = useComposerPayload();
  useEffect(() => {
    onDraft(draft);
  }, [draft, onDraft]);
  return null;
}

function useComposerPayload(): string {
  const text = useAuiState(state => state.composer.text);
  const quote = useAuiState(state => state.composer.quote);
  return useMemo(() => chatComposerText({text, quote}), [text, quote]);
}

type NoticeProps = Pick<SteerComposerProps, "onResend" | "onDismiss"> & {
  draftRef: RefObject<HTMLTextAreaElement | null>;
};

// Capped and scrolled, so a run of unsent messages never pushes the draft out
// of a short footer.
function PendingSendNotices({ entries, ...actions }: { entries: readonly PendingSend[] } & NoticeProps) {
  if (entries.length === 0) return null;
  return (
    <List
      variant="plain"
      aria-label={NOTICES_LABEL}
      className="flex max-h-32 flex-col gap-xs space-y-0 overflow-y-auto"
    >
      {entries.map((entry) => (
        <ListItem key={entry.operationId}>
          <PendingSendNotice entry={entry} {...actions} />
        </ListItem>
      ))}
    </List>
  );
}

// One unresolved send. Resend posts the ledger record under its own operation
// id — never the composer's text — and clears a draft that is exactly that
// text, so Enter cannot send it a second time. A refused send and an
// unconfirmed one read differently, because they are: the server said no to
// the first, and nothing answered for the second. A conflict's id is spent, so
// its button sends the text as a new message rather than offering a Resend
// that could only be refused again.
function PendingSendNotice({ entry, onResend, onDismiss, draftRef }: { entry: PendingSend } & NoticeProps) {
  const aui = useAui();
  const modelChanging = useFleetSetup()?.modelChanging ?? false;
  const textId = useId();
  // Sign-in returns to this fleet, where the notice's Resend is waiting.
  const pathname = usePathname();
  const resend = () => {
    const composer = aui.composer();
    if (chatComposerText(composer.getState()) === entry.text) {
      composer.setQuote(undefined);
      composer.setText("");
    }
    onResend(entry.operationId);
    draftRef.current?.focus();
  };
  const dismiss = () => {
    onDismiss(entry.operationId);
    draftRef.current?.focus();
  };
  const unconfirmed = entry.state === PENDING_SEND_STATE.UNKNOWN;
  return (
    <Alert
      variant={unconfirmed ? "warning" : "destructive"}
      className="items-center gap-sm"
      onDismiss={dismiss}
    >
      {/* The reason keeps its own line on a narrow screen; only the quoted
          message truncates. */}
      <span className="min-w-0 flex-1">
        <span className="block">{sentenceFor(entry.state)}</span>
        <span id={textId} className="block truncate text-foreground">{entry.text}</span>
      </span>
      {entry.state === PENDING_SEND_STATE.SESSION ? (
        <Button asChild type="button" variant="outline" size="sm">
          <Link href={signInPath(pathname)}>{SIGN_IN_LABEL}</Link>
        </Button>
      ) : null}
      {/* Offered after a sign-in too: once the session is back, a Resend is
          the way out, and a fresh 401 simply marks it again. */}
      <Button type="button" variant="outline" size="sm" disabled={modelChanging} onClick={resend} aria-describedby={textId}>
        {entry.state === PENDING_SEND_STATE.CONFLICT ? SEND_AS_NEW_LABEL : RESEND_LABEL}
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
    case PENDING_SEND_STATE.CONFLICT:
      return SEND_CONFLICT;
    default:
      return SEND_FAILED;
  }
}
