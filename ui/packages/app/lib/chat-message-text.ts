import type { AppendMessage, QuoteInfo } from "@assistant-ui/react";
import { z } from "zod";

type Draft = {text: string; quote?: QuoteInfo};
const Quote = z.object({text: z.string(), messageId: z.string().optional()});

/** Our APIs persist text. Include native quote metadata in that same durable message. */
export function chatMessageText(message: AppendMessage): string {
  return chatComposerText(chatMessageDraft(message));
}

/** Preserve native quote identity when restoring a failed queue submission. */
export function chatMessageDraft(message: AppendMessage): Draft {
  const text = message.content.flatMap(part => part.type === "text" ? [part.text] : []).join("\n");
  const parsed = Quote.safeParse(message.metadata.custom.quote);
  if (!parsed.success) return {text};
  const quote = parsed.data;
  return quote.messageId === undefined ? {text: chatComposerText({text, quote})}
    : {text, quote: {text: quote.text, messageId: quote.messageId}};
}

/** Draft recovery, byte limits and delivery compare the same native quote payload. */
export function chatComposerText({text, quote}: {text: string; quote?: {text: string}}): string {
  if (!quote?.text.trim()) return text;
  const quoted = quote.text.replaceAll("\r\n", "\n").split("\n").map(line => `> ${line}`).join("\n");
  return `${quoted}\n\n${text}`;
}
