"use client";

import { ComposerPrimitive, useAui, useAuiState } from "@assistant-ui/react";
import { QuotesIcon, XIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";

export function QuoteReply({text}: {text: string}) {
  const aui = useAui();
  const messageId = useAuiState(state => state.message.id);
  const disabled = useAuiState(state => state.thread.isDisabled);
  return <Button variant="ghost" size="icon-sm" aria-label="Quote reply" title="Quote reply" disabled={disabled}
    onClick={() => aui.thread.composer().setQuote({text, messageId})}><QuotesIcon aria-hidden="true" /></Button>;
}

export function ComposerQuote() {
  return <ComposerPrimitive.Quote className="mb-md flex items-start gap-md rounded-sm border-l-2 border-border-strong bg-muted px-lg py-md">
    <ComposerPrimitive.QuoteText className="line-clamp-3 min-w-0 flex-1 whitespace-pre-wrap break-words text-label text-muted-foreground" />
    <ComposerPrimitive.QuoteDismiss asChild><Button variant="ghost" size="icon-sm" aria-label="Remove quote"><XIcon aria-hidden="true" /></Button></ComposerPrimitive.QuoteDismiss>
  </ComposerPrimitive.Quote>;
}
