import type { MessageState } from "@assistant-ui/react";
import { z } from "zod";

import { REASONING_SPAN, REPLY_FIGURE } from "./fleetReplyMessage";
import { GROUP_META, RENDER_KIND_KEY } from "./useFleetThreadEntries";
import { readNanos } from "@/lib/api/nanos";

// The custom-metadata accessors a rendered message reads. Pure and JSX-free,
// split out of `fleetMessageRenderers` at its length cap. `convertEvent`
// packs the durable row's fields into `metadata.custom`; these read them back
// out with the tolerant defaults the renderer relies on (a missing or
// wrong-typed field reads as empty/null, never throws).

export function readText(message: MessageState): string {
  for (const part of message.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}

/**
 * Whether this turn has anything of its own to show.
 *
 * Text, or any non-text part — an image-only append carries no `text` part and
 * is still a turn somebody sent. What this excludes is the turn whose body the
 * read never CARRIED: the events list selects no `request_json`
 * (`afd_events` history/statement/mod.rs), and a completion frame for a run the
 * timeline never opened is rebuilt from exactly that shape, so rendering it as
 * an operator bubble would show an empty pill and claim somebody sent nothing.
 */
export function hasOwnContent(message: MessageState): boolean {
  return message.content.some(
    (part) => (part.type === "text" ? part.text.trim().length > 0 : true),
  );
}

export function readActor(message: MessageState): string {
  const raw = message.metadata.custom["actor"];
  return typeof raw === "string" ? raw : "";
}

export function readCustomStatus(message: MessageState): string {
  const raw = message.metadata.custom["status"];
  return typeof raw === "string" ? raw : "";
}

export function readRenderKind(message: MessageState): string | null {
  const raw = message.metadata.custom[RENDER_KIND_KEY];
  return typeof raw === "string" ? raw : null;
}

/** When the reply's thinking started and ended; null for an end not recorded. */
export function readReasoningSpan(message: MessageState): { startedAtMs: number | null; endedAtMs: number | null } {
  return {
    startedAtMs: finiteNumber(message.metadata.custom[REASONING_SPAN.STARTED]),
    endedAtMs: finiteNumber(message.metadata.custom[REASONING_SPAN.ENDED]),
  };
}

/** What the turn cost, as the run reported it. A figure it did not report
 * reads null, so nothing draws it as zero. */
export type ReplyFigures = { tokens: number | null; wallMs: number | null; costNanos: string | null };

export function readReplyFigures(message: MessageState): ReplyFigures {
  const custom = message.metadata.custom;
  return {
    tokens: finiteNumber(custom[REPLY_FIGURE.TOKENS]),
    wallMs: finiteNumber(custom[REPLY_FIGURE.WALL_MS]),
    costNanos: readNanos(custom[REPLY_FIGURE.COST_NANOS]),
  };
}

/** How many calls the saved trace left out to stay inside its bounds. */
export function readOmittedCallCount(message: MessageState): number {
  return Math.max(0, finiteNumber(message.metadata.custom[REPLY_FIGURE.OMITTED_CALLS]) ?? 0);
}

export function readReplyRecovering(message: MessageState): boolean {
  return message.metadata.custom["replyRecovering"] === true;
}

export function readSubmittedAtMs(message: MessageState): number | null {
  return finiteNumber(message.metadata.custom["submittedAtMs"]);
}

export function readQueued(message: MessageState): boolean {
  return message.metadata.custom["queued"] === true;
}

export function readOutcome(message: MessageState): string {
  const raw = message.metadata.custom["outcome"];
  return typeof raw === "string" ? raw : "";
}

const recordedActivitySchema = z.object({
  id: z.string().min(1), role: z.literal("system"), actor: z.string(), text: z.string(), status: z.string(), createdAt: z.date(),
  outcome: z.string().default(""), failureLabel: z.string().nullable().default(null),
  failureDetail: z.string().nullable().default(null),
  custom: z.object({requestJson: z.string().nullable().optional()}).optional(),
});
export type RecordedActivity = z.infer<typeof recordedActivitySchema>;
const groupedActivitySchema = z.tuple([recordedActivitySchema]).rest(recordedActivitySchema)
  .refine(members => new Set(members.map(member => member.id)).size === members.length);

/** Malformed members cannot become rows or duplicate React identities.
 * The message root memoizes adaptation for each immutable message snapshot. */
export function readGroupMembers(message: MessageState): [RecordedActivity, ...RecordedActivity[]] | null {
  const raw = message.metadata.custom[GROUP_META.MEMBERS];
  if (!Array.isArray(raw)) return null;
  const parsed = groupedActivitySchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

export function readFailureLabel(message: MessageState): string | null {
  const raw = message.metadata.custom["failureLabel"];
  return typeof raw === "string" && raw.length > 0 ? raw : null;
}

export function readFailureDetail(message: MessageState): string | null {
  const raw = message.metadata.custom["failureDetail"];
  return typeof raw === "string" && raw.length > 0 ? raw : null;
}

export function readRequestJson(message: MessageState): string | null {
  const raw = message.metadata.custom["requestJson"];
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 && trimmed !== "{}" ? trimmed : null;
}

function finiteNumber(raw: unknown): number | null {
  return typeof raw === "number" && Number.isFinite(raw) ? raw : null;
}
