"use client";

import { useState } from "react";
import { AttachmentPrimitive, ComposerPrimitive, MessagePrimitive, useAuiEvent, useAuiState } from "@assistant-ui/react";
import { Button } from "@agentsfleet/design-system";
import { FileIcon, PaperclipIcon, XIcon } from "@phosphor-icons/react/ssr";

/** Native attachment contexts own preparation, error state and removal. */
export function AddUploadPreview() {
  const supported = useAuiState(state => state.thread.capabilities.attachments);
  if (!supported) return null;
  return <ComposerPrimitive.AddAttachment asChild><Button variant="ghost" size="icon-sm" aria-label="Attach a preview file" title="Preview upload · up to 5 MiB">
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

export function MessageUploadAttachments() {
  return <MessagePrimitive.Attachments>{() => <UploadAttachment />}</MessagePrimitive.Attachments>;
}

function UploadAttachment({removable = false}: {removable?: boolean}) {
  const status = useAuiState(state => state.attachment.status);
  return <AttachmentPrimitive.Root className="flex min-w-0 items-center gap-sm rounded-md border border-border bg-muted px-md py-sm text-label">
    <FileIcon className="size-4 shrink-0" aria-hidden="true" />
    <span className="min-w-0 flex-1 break-words"><AttachmentPrimitive.Name /></span>
    <span className="text-eyebrow text-muted-foreground">{status.type === "running" ? "Uploading…" : "Preview"}</span>
    {status.type === "incomplete" ? <output className="text-destructive">{status.message || "Upload failed. Remove this file and try again."}</output> : null}
    {removable ? <AttachmentPrimitive.Remove asChild><Button variant="ghost" size="icon-sm" aria-label="Remove attachment"><XIcon aria-hidden="true" /></Button></AttachmentPrimitive.Remove> : null}
  </AttachmentPrimitive.Root>;
}
