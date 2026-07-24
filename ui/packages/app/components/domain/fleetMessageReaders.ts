import type { MessageState } from "@assistant-ui/react";

import { GROUP_META, RENDER_KIND_KEY } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

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
