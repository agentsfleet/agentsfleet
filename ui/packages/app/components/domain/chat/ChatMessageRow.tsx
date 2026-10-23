"use client";

import type { ReactNode } from "react";
import { BrandMark, cn } from "@agentsfleet/design-system";

/** Both chat purposes share the text rail, operator bubble and assistant identity. */
export function ChatMessageRow({sender, operator, children, messageRole, dimmed, failed, showSender, footer}: {
  sender: string; operator: boolean; children: ReactNode; messageRole: string;
  dimmed?: boolean; failed?: boolean; showSender?: boolean; footer?: ReactNode;
}) {
  return <div className="w-full motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-stream"
    data-role={messageRole} data-optimistic={dimmed || undefined} data-failed={failed || undefined}>
    <div className={cn("flex w-full py-md", operator ? "justify-end" : "justify-start")}>
      <div className={cn("flex min-w-0 flex-col gap-sm", operator ? "max-w-prose items-end" : "w-full items-start")}>
        {!operator || showSender ? <span aria-hidden="true" data-testid="fleet-message-sender"
          className="inline-flex items-center gap-sm text-label leading-label text-muted-foreground">
          {!operator ? <BrandMark className="size-3.5 shrink-0 text-pulse" /> : null}{sender}
        </span> : null}
        <div className={cn("min-w-0 max-w-full break-words font-sans text-reading leading-reading text-foreground",
          operator ? "w-fit rounded-xl bg-muted px-lg py-md" : "w-full")}>
          <span className="sr-only">{sender}: </span>{children}
        </div>
        {footer}
      </div>
    </div>
  </div>;
}
