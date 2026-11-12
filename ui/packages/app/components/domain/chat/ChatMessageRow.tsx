"use client";

import type { ReactNode } from "react";
import { BrandMark, cn } from "@agentsfleet/design-system";

type MessageIdentity = {sender: string; timestamp?: ReactNode; annotation?: ReactNode};
type TurnProps = MessageIdentity & {children: ReactNode; footer?: ReactNode; showSender?: boolean};
type ChatMessageRowProps = TurnProps & {
  operator: boolean;
  messageRole: string;
  dimmed?: boolean;
  failed?: boolean;
};

/** Layout only. Message, tool, approval and failure renderers supply their content. */
export function ChatMessageRow({operator, messageRole, dimmed, failed, ...turn}: ChatMessageRowProps) {
  return (
    <div className="w-full motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-stream"
      data-role={messageRole} data-optimistic={dimmed || undefined} data-failed={failed || undefined}>
      <div className={cn("flex w-full pb-lg pt-lg sm:pb-2xl", operator ? "justify-end" : "justify-start")}>
        {operator ? <OperatorTurn {...turn} /> : <AssistantTurn {...turn} />}
      </div>
    </div>
  );
}

function AssistantTurn({sender, timestamp, annotation, children, footer}: TurnProps) {
  return (
    <div className="flex min-w-0 w-full flex-col items-start gap-lg">
      <SenderLine sender={sender} timestamp={timestamp} annotation={annotation} />
      <MessageContent sender={sender}>{children}</MessageContent>
      {footer}
    </div>
  );
}

function OperatorTurn({sender, timestamp, annotation, children, footer, showSender}: TurnProps) {
  return (
    <div className="flex w-full min-w-0 max-w-full flex-col items-end gap-md sm:w-9/10">
      <div className="w-full min-w-0 max-w-full rounded-lg rounded-br-[6px] border border-border bg-muted px-5 py-xl">
        <MessageContent sender={sender}>{children}</MessageContent>
      </div>
      <MessageMetadata sender={sender} timestamp={timestamp} annotation={annotation} showSender={showSender} />
      {footer}
    </div>
  );
}

function MessageContent({sender, children}: {sender: string; children: ReactNode}) {
  return (
    <div className="w-full min-w-0 max-w-full break-words font-sans text-conversation leading-conversation text-foreground">
      <span className="sr-only">{sender}: </span>{children}
    </div>
  );
}

function SenderLine({sender, timestamp, annotation}: MessageIdentity) {
  return (
    <div className="flex min-h-5 w-full min-w-0 items-center gap-sm" data-testid="chat-message-header">
      <span aria-hidden="true" data-testid="fleet-message-sender" className="inline-flex min-w-0 items-center gap-md text-label leading-label text-muted-foreground">
        <BrandMark className="size-4.5 shrink-0 rounded-brand bg-brand-tile p-xs text-brand-ray" /><span className="truncate font-semibold">{sender}</span>
      </span>
      <MessageTime>{timestamp}</MessageTime>
      {annotation}
    </div>
  );
}

function MessageMetadata({sender, timestamp, annotation, showSender}: MessageIdentity & {showSender?: boolean}) {
  if (!timestamp && !annotation && !showSender) return null;
  return (
    <div className="inline-flex items-center gap-sm px-sm text-eyebrow leading-eyebrow text-muted-foreground">
      <span aria-hidden="true" data-testid="fleet-message-sender">{sender}</span><MessageTime>{timestamp}</MessageTime>{annotation}
    </div>
  );
}

function MessageTime({children}: {children?: ReactNode}) {
  if (!children) return null;
  return <span className="inline-flex items-center gap-sm text-eyebrow text-muted-foreground"><span aria-hidden="true">·</span>{children}</span>;
}
