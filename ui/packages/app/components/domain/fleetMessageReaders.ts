import type { MessageState } from "@assistant-ui/react";

import { GROUP_META, RENDER_KIND_KEY } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";

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
 * (`afd_events` history/statement.rs), and a completion frame for a run the
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

export function readReply(message: MessageState): string {
  const raw = message.metadata.custom["reply"];
  return typeof raw === "string" ? raw : "";
}

export function readReasoning(message: MessageState): string {
  const raw = message.metadata.custom["reasoning"];
  return typeof raw === "string" ? raw : "";
}

export function readThinking(message: MessageState): boolean {
  return message.metadata.custom["thinking"] === true;
}

export function readReplyRecovering(message: MessageState): boolean {
  return message.metadata.custom["replyRecovering"] === true;
}

export function readSubmittedAtMs(message: MessageState): number | null {
  const raw = message.metadata.custom["submittedAtMs"];
  return typeof raw === "number" && Number.isFinite(raw) ? raw : null;
}

export function readQueued(message: MessageState): boolean {
  return message.metadata.custom["queued"] === true;
}

export function readOutcome(message: MessageState): string {
  const raw = message.metadata.custom["outcome"];
  return typeof raw === "string" ? raw : "";
}

/** Present only on a grouped message; null means this row stands for itself. */
export function readGroupMembers(message: MessageState): FleetEvent[] | null {
  const raw = message.metadata.custom[GROUP_META.MEMBERS];
  return Array.isArray(raw) && raw.length > 0 ? (raw as FleetEvent[]) : null;
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
