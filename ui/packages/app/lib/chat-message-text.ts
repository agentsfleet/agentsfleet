import type { AppendMessage, QuoteInfo } from "@assistant-ui/react";
import { parseChatAddress } from "./chat-address";
import { z } from "zod";

type Draft = {text: string; quote?: QuoteInfo};
const Quote = z.object({text: z.string(), messageId: z.string().optional()});

/** Our APIs persist text. Include native quote metadata in that same durable message. */
export function chatMessageText(message: AppendMessage): string {
  return chatComposerText(chatMessageDraft(message));
}

/** Preserve native quote identity when restoring a failed queue submission. */
export function chatMessageDraft(message: AppendMessage, format = chatComposerText): Draft {
  const text = message.content.flatMap(part => part.type === "text" ? [part.text] : []).join("\n");
  const parsed = Quote.safeParse(message.metadata.custom.quote);
  if (!parsed.success) return {text};
  const quote = parsed.data;
  return quote.messageId === undefined ? {text: format({text, quote})}
    : {text, quote: {text: quote.text, messageId: quote.messageId}};
}

/** Draft recovery, byte limits and delivery compare the same native quote payload. */
export function chatComposerText({text, quote}: {text: string; quote?: {text: string}}): string {
  if (!quote?.text.trim()) return text;
  const quoted = quote.text.replaceAll("\r\n", "\n").split("\n").map(line => `> ${line}`).join("\n");
  return `${quoted}\n\n${text}`;
}

/** Address the recipient before serializing a quote, so the quote is sent to that fleet. */
export function fleetComposerText(draft: {text: string; quote?: {text: string}}): string {
  const address = draft.quote?.text.trim() && parseChatAddress(draft.text);
  if (!address) return chatComposerText(draft);
  return `@${address.name} ${chatComposerText({...draft, text: address.rest})}`;
}

export function fleetMessageText(message: AppendMessage): string {
  return fleetComposerText(chatMessageDraft(message, fleetComposerText));
}
