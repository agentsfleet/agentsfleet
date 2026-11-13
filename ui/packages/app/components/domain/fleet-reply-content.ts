import type { MessageState } from "@assistant-ui/react";
import type { readReasoningSpan, ReplyFigures } from "./fleetMessageReaders";
import { z } from "zod";

export type ReplyContext = {
  errored: boolean; running: boolean; queued: boolean; eventId: string; reasoning: string;
  span: ReturnType<typeof readReasoningSpan>;
  content: MessageState["content"];
  startedAtMs: number;
  wallMs?: number | null;
  figures?: ReplyFigures;
  activity?: unknown;
  contextProgress?: unknown;
  plainText?: boolean;
};
export type DefaultReplyContent = {text: string | null; label: string; href: string | null};

const displayFieldsSchema = z.object({
  text: z.string().optional().catch(undefined), title: z.string().optional().catch(undefined),
  filename: z.string().optional().catch(undefined), url: z.url({protocol: /^https?$/}).optional().catch(undefined),
});

/** Arbitrary data, binary bodies and component specs are never rendered as HTML. */
export function readDefaultReplyContent(part: unknown): DefaultReplyContent {
  const result = displayFieldsSchema.safeParse(part);
  const fields = result.success ? result.data : {};
  return {text: fields.text?.trim() || null,
    label: fields.title?.trim() || fields.filename?.trim() || "Activity details", href: fields.url ?? null};
}
