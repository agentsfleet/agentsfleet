"use client";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import type { FleetThreadEntry } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ENTRY_KIND } from "@/lib/events/event-grouping";
import { PreviewAttachmentAdapter } from "@/lib/api/preview-attachment-adapter";
import { UPLOAD_PREVIEW_LABEL } from "@/lib/api/upload-preview";
import { RENDER_KIND, RENDER_KIND_KEY, REPLY_ID_SUFFIX } from "./useFleetThreadEntries";

type Anchor = Pick<FleetEvent, "id" | "submittedAtMs">;
type PreviewEntry = {kind: "upload-preview"; key: string; anchors: readonly Anchor[]; message: ThreadMessageLike};
export type FleetConversationEntry = FleetThreadEntry | PreviewEntry;
type Props = {workspaceId: string; fleetId: string; subject: string | null; events: readonly Anchor[]; onNew: (message: AppendMessage) => Promise<void>};
type AnchorIndex = {ids: Map<string, number>; local: Map<number, number>};
const MAX_PREVIEW_TURNS = 4;
const MAX_ANCHORS = 16;
const EMPTY_PREVIEWS: readonly PreviewEntry[] = [];
const EMPTY_BREAKS: ReadonlySet<string> = new Set();

/** Temporary receipts share the runtime, but never enter the durable event store. */
export function useFleetUploadPreviews({workspaceId, fleetId, subject, events, onNew}: Props) {
  const owner = JSON.stringify([workspaceId, fleetId, subject]);
  const attachments = useMemo(() => new PreviewAttachmentAdapter(workspaceId, fleetId, subject), [workspaceId, fleetId, subject]);
  const [preview, setPreview] = useState<{owner: string; entries: PreviewEntry[]}>({owner, entries: []});
  const currentEvents = useRef(events);
  useLayoutEffect(() => {currentEvents.current = events;}, [events]);
  useEffect(() => {attachments.connect(); return () => attachments.dispose();}, [attachments]);
  const receive = useCallback(async (message: AppendMessage) => {
    attachments.signal.throwIfAborted();
    const files = message.attachments ?? [];
    if (!files.length) return onNew(message);
    if (!subject) throw new Error("Sign in before uploading a preview.");
    const id = files.map(file => file.id).join(":");
    const text = message.content.flatMap(part => part.type === "text" ? [part.text] : []).join("\n");
    const reply = files.flatMap(file => file.content.flatMap(part => part.type === "text" ? [part.text] : [])).join("\n");
    const anchors = currentEvents.current.slice(-MAX_ANCHORS).reverse().map(event => ({id: event.id, submittedAtMs: event.submittedAtMs}));
    const entries: PreviewEntry[] = [
      {kind: "upload-preview", key: `upload:${id}`, anchors, message: {id: `upload:${id}`, role: "user", createdAt: new Date(),
        content: text || "Attached files for preview", attachments: files, metadata: {custom: {actor: `steer:${subject}`, [RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, previewUpload: true}}}},
      {kind: "upload-preview", key: `upload:${id}:receipt`, anchors, message: {id: `upload:${id}:receipt`, role: "assistant", createdAt: new Date(),
        content: `${UPLOAD_PREVIEW_LABEL}\n\n${reply}`, status: {type: "complete", reason: "stop"},
        metadata: {custom: {actor: "fleet", status: "processed", previewUpload: true}}}},
    ];
    setPreview(previous => ({owner, entries: [...(previous.owner === owner ? previous.entries.filter(entry => !entries.some(next => next.key === entry.key)) : []), ...entries].slice(-MAX_PREVIEW_TURNS * 2)}));
    attachments.consume(files);
  }, [attachments, owner, subject, onNew]);
  const entries = preview.owner === owner ? preview.entries : EMPTY_PREVIEWS;
  const groupBreaks = useMemo(() => {
    if (!entries.length) return EMPTY_BREAKS;
    const index: AnchorIndex = {ids: new Map(), local: new Map()};
    events.forEach((event, position) => indexAnchor(index, event, position));
    const breaks = new Set<string>();
    for (const entry of entries) {
      const next = events[anchorPosition(index, entry.anchors) + 1];
      if (next) breaks.add(next.id);
    }
    return breaks;
  }, [events, entries]);
  return {attachments, onNew: receive, entries, groupBreaks};
}

/** Insert receipts at their observed boundary without sorting or replacing durable rows. */
export function mergePreviewEntries(entries: FleetThreadEntry[], previews: readonly PreviewEntry[]): FleetConversationEntry[] {
  if (!previews.length) return entries;
  const index: AnchorIndex = {ids: new Map(), local: new Map()};
  entries.forEach((entry, position) => {
    for (const event of entry.kind === ENTRY_KIND.GROUP ? entry.events : [entry.event]) indexAnchor(index, event, position);
    if (entry.kind === "reply") index.ids.set(entry.event.id.slice(0, -REPLY_ID_SUFFIX.length), position);
  });
  const slots = new Map<number, PreviewEntry[]>();
  for (const preview of previews) {
    const position = anchorPosition(index, preview.anchors);
    const slot = slots.get(position);
    if (slot) slot.push(preview); else slots.set(position, [preview]);
  }
  const result: FleetConversationEntry[] = [...(slots.get(-1) ?? [])];
  entries.forEach((entry, position) => result.push(entry, ...(slots.get(position) ?? [])));
  return result;
}

function indexAnchor(index: AnchorIndex, event: Anchor, position: number): void {
  index.ids.set(event.id, position);
  if (event.submittedAtMs !== undefined) index.local.set(event.submittedAtMs, position);
}

function anchorPosition(index: AnchorIndex, anchors: readonly Anchor[]): number {
  for (const anchor of anchors) {
    const position = index.ids.get(anchor.id) ?? (anchor.submittedAtMs === undefined ? undefined : index.local.get(anchor.submittedAtMs));
    if (position !== undefined) return position;
  }
  return -1;
}
