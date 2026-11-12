"use client";

import { useState } from "react";
import { AttachmentPrimitive, ComposerPrimitive, MessagePrimitive, useAuiEvent, useAuiState } from "@assistant-ui/react";
import { Button, cn } from "@agentsfleet/design-system";
import { FileTextIcon, PaperclipIcon, XIcon } from "@phosphor-icons/react/ssr";

/** Native attachment contexts own preparation, error state and removal. */
export function AddUploadPreview() {
  const supported = useAuiState(state => state.thread.capabilities.attachments);
  if (!supported) return null;
  return <ComposerPrimitive.AddAttachment asChild><Button variant="ghost" size="icon-sm" className="size-9 [&_svg]:size-4.5" aria-label="Attach a preview file" title="Preview upload · up to 5 MiB">
    <PaperclipIcon aria-hidden="true" />
  </Button></ComposerPrimitive.AddAttachment>;
}

export function ComposerUploadAttachments() {
  const [error, setError] = useState<string | null>(null);
  useAuiEvent("composer.attachmentAddError", event => setError(event.message));
  useAuiEvent("composer.attachmentAdd", () => setError(null));
  return <>
    {error ? <div role="alert" className="flex items-center gap-sm text-label text-destructive">
      <span className="min-w-0 flex-1 break-words">{error}</span>
      <Button variant="ghost" size="icon-sm" aria-label="Dismiss attachment error" onClick={() => setError(null)}><XIcon aria-hidden="true" /></Button>
    </div> : null}
    <ComposerPrimitive.Attachments>{() => <UploadAttachment removable />}</ComposerPrimitive.Attachments>
  </>;
}

export function MessageUploadAttachments({preparing}: {preparing?: boolean}) {
  const count = useAuiState(state => (state.message.submission?.attachments ?? state.message.attachments ?? []).length);
  if (!count) return null;
  return <div className="mb-lg flex min-w-0 flex-wrap gap-sm"><MessagePrimitive.Attachments>{() => <UploadAttachment preparing={preparing} message />}</MessagePrimitive.Attachments></div>;
}

function UploadAttachment({removable, preparing, message}: {removable?: boolean; preparing?: boolean; message?: boolean}) {
  const {status, name} = useAuiState(state => state.attachment);
  return <AttachmentPrimitive.Root title="Preview upload. The file is not stored or analyzed." className={cn("min-h-9 min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-md rounded-control border border-border px-lg py-sm text-eyebrow", message ? "inline-grid max-w-full bg-card" : "grid bg-muted")}>
    <span className="flex min-w-0 items-center gap-md"><FileTextIcon className="size-4 shrink-0" aria-hidden="true" /><span className="min-w-0 truncate" title={name}><AttachmentPrimitive.Name /></span></span>
    <span className="flex shrink-0 items-center gap-md"><span className={cn("whitespace-nowrap text-eyebrow text-muted-foreground", message && status.type === "complete" && "sr-only")}>{status.type === "running" || (preparing && status.type !== "complete") ? "Uploading…" : "Preview"}</span>
      {removable ? <AttachmentPrimitive.Remove asChild><Button variant="ghost" size="icon-sm" aria-label="Remove attachment"><XIcon aria-hidden="true" /></Button></AttachmentPrimitive.Remove> : null}
    </span>
    {status.type === "incomplete" ? <output className="col-span-full break-words text-destructive">{status.message || "Upload failed. Remove this file and try again."}</output> : null}
  </AttachmentPrimitive.Root>;
}
