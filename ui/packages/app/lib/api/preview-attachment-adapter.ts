import type { Attachment, AttachmentAdapter, CompleteAttachment, PendingAttachment } from "@assistant-ui/react";
import { v7 } from "uuid";
import { PREVIEW_FILE_COUNT, uploadPreviewUrl, uploadReceiptSchema } from "./upload-preview";
import { MESSAGE_IMAGE_ACCEPT, canAttachImage, checkImage, prepareImage } from "./message-images";

/** Files are sent only when the user presses Send; cancellation owns each request. */
export class PreviewAttachmentAdapter implements AttachmentAdapter {
  readonly accept: string;
  #lifetime = new AbortController();
  readonly #pending = new Map<string, AbortController>();
  readonly #files = new Set<string>();

  constructor(readonly workspace: string, readonly fleet: string | null, readonly subject: string | null) {
    this.accept = fleet === null ? MESSAGE_IMAGE_ACCEPT : "*";
  }

  get signal(): AbortSignal {return this.#lifetime.signal;}
  connect(): void {if (this.#lifetime.signal.aborted) this.#lifetime = new AbortController();}

  async add({file}: {file: File}): Promise<PendingAttachment> {
    this.#lifetime.signal.throwIfAborted();
    const image = canAttachImage(file.type);
    if (image || this.fleet === null) checkImage(file);
    if (!image && !this.subject) throw new Error("Sign in before uploading a preview.");
    if (!image && this.#files.size >= PREVIEW_FILE_COUNT) throw new Error("Attach up to five preview files at a time.");
    const id = v7();
    if (!image) this.#files.add(id);
    return {id, file, name: file.name, contentType: file.type, type: image ? "image" : "file", status: {type: "requires-action", reason: "composer-send"}};
  }

  async send(attachment: PendingAttachment, options?: {signal?: AbortSignal}): Promise<CompleteAttachment> {
    if (attachment.type === "image") {
      const signal = AbortSignal.any([this.signal, ...(options?.signal ? [options.signal] : [])]);
      return prepareImage(attachment, signal);
    }
    if (this.fleet === null) throw new Error("Attach an image to start a fleet.");
    if (!this.#files.has(attachment.id)) throw new Error("Choose this file again in the current conversation.");
    if (this.#pending.has(attachment.id)) throw new Error("This file preview is already being uploaded.");
    const controller = new AbortController();
    this.#pending.set(attachment.id, controller);
    const signal = AbortSignal.any([this.#lifetime.signal, controller.signal, ...(options?.signal ? [options.signal] : [])]);
    try {
      const form = new FormData();
      form.set("file", attachment.file);
      const response = await fetch(uploadPreviewUrl(this.workspace, this.fleet), {method: "POST", body: form, signal, cache: "no-store", redirect: "error"});
      if (!response.ok) throw new Error(response.status === 401 ? "Sign in before uploading a preview." : "The file preview was not accepted. Remove it and try again.");
      const receipt = uploadReceiptSchema.parse(await response.json());
      signal.throwIfAborted();
      return {id: attachment.id, type: "file", name: attachment.name, contentType: attachment.contentType,
        status: {type: "complete"}, content: [{type: "text", text: receipt.reply}]};
    } finally {this.#pending.delete(attachment.id);}
  }

  async remove(attachment: Attachment): Promise<void> {
    this.#pending.get(attachment.id)?.abort();
    this.#files.delete(attachment.id);
  }

  consume(attachments: readonly Attachment[]): void {
    for (const attachment of attachments) this.#files.delete(attachment.id);
  }

  dispose(): void {this.#lifetime.abort(); this.#pending.clear(); this.#files.clear();}
}
