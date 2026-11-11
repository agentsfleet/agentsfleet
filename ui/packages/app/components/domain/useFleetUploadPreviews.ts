"use client";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import type { FleetThreadEntry } from "./useFleetThreadEntries";
import { useCallback, useEffect, useMemo, useState } from "react";
import { PreviewAttachmentAdapter } from "@/lib/api/preview-attachment-adapter";
import { UPLOAD_PREVIEW_LABEL } from "@/lib/api/upload-preview";
import { RENDER_KIND, RENDER_KIND_KEY } from "./useFleetThreadEntries";

type PreviewEntry = {kind: "upload-preview"; key: string; message: ThreadMessageLike};
export type FleetConversationEntry = FleetThreadEntry | PreviewEntry;
type Props = {workspaceId: string; fleetId: string; subject: string | null; onNew: (message: AppendMessage) => Promise<void>};
const MAX_PREVIEW_TURNS = 4;

/** Temporary receipts share the runtime, but never enter the durable event store. */
export function useFleetUploadPreviews({workspaceId, fleetId, subject, onNew}: Props) {
  const owner = JSON.stringify([workspaceId, fleetId, subject]);
  const attachments = useMemo(() => new PreviewAttachmentAdapter(workspaceId, fleetId, subject), [workspaceId, fleetId, subject]);
  const [preview, setPreview] = useState<{owner: string; entries: PreviewEntry[]}>({owner, entries: []});
  useEffect(() => {attachments.connect(); return () => attachments.dispose();}, [attachments]);
  const receive = useCallback(async (message: AppendMessage) => {
    const files = message.attachments ?? [];
    if (!files.length) return onNew(message);
    attachments.signal.throwIfAborted();
    if (!subject) throw new Error("Sign in before uploading a preview.");
    const id = files.map(file => file.id).join(":");
    const text = message.content.flatMap(part => part.type === "text" ? [part.text] : []).join("\n");
    const reply = files.flatMap(file => file.content.flatMap(part => part.type === "text" ? [part.text] : [])).join("\n");
    const entries: PreviewEntry[] = [
      {kind: "upload-preview", key: `upload:${id}`, message: {id: `upload:${id}`, role: "user", createdAt: new Date(),
        content: text || "Attached files for preview", attachments: files, metadata: {custom: {actor: `steer:${subject}`, [RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, previewUpload: true}}}},
      {kind: "upload-preview", key: `upload:${id}:receipt`, message: {id: `upload:${id}:receipt`, role: "assistant", createdAt: new Date(),
        content: `${UPLOAD_PREVIEW_LABEL}\n\n${reply}`, status: {type: "complete", reason: "stop"},
        metadata: {custom: {actor: "fleet", status: "processed", previewUpload: true}}}},
    ];
    setPreview(previous => ({owner, entries: [...(previous.owner === owner ? previous.entries.filter(entry => !entries.some(next => next.key === entry.key)) : []), ...entries].slice(-MAX_PREVIEW_TURNS * 2)}));
    attachments.consume(files);
  }, [attachments, owner, subject, onNew]);
  return {attachments, onNew: receive, entries: preview.owner === owner ? preview.entries : []};
}
