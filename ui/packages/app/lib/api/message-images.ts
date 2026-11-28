import type { AppendMessage, CompleteAttachment, PendingAttachment } from "@assistant-ui/react";

const TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"] as const;
const IMAGE_BYTES_MAX = 5 * 1024 * 1024; // Messages provider limit.
type ImageMediaType = typeof TYPES[number];
export type MessageImage = {media_type: string; data: string};

export const MESSAGE_IMAGE_ACCEPT = TYPES.join(",");

export function canAttachImage(type: string): type is ImageMediaType {
  return TYPES.some(supported => supported === type);
}

export function checkImage(file: File): void {
  if (!canAttachImage(file.type)) throw new Error("Attach a PNG, JPEG, GIF or WebP image.");
  if (file.size === 0 || file.size > IMAGE_BYTES_MAX) throw new Error("Each image must contain 1 byte to 5 MiB of data.");
}

export function prepareImage(attachment: PendingAttachment, signal: AbortSignal): Promise<CompleteAttachment> {
  checkImage(attachment.file);
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    const stop = () => reader.abort();
    signal.throwIfAborted();
    signal.addEventListener("abort", stop, {once: true});
    reader.onloadend = () => {
      signal.removeEventListener("abort", stop);
      if (typeof reader.result !== "string") {
        reject(new Error("Unable to read the attached image."));
        return;
      }
      resolve({
        id: attachment.id, type: "image", name: attachment.name, contentType: attachment.contentType,
        status: {type: "complete"}, content: [{type: "image", image: reader.result}],
      });
    };
    reader.readAsDataURL(attachment.file);
  });
}

/** Assistant UI's prepared files carry local image data as data URLs. */
export function messageImages(message: AppendMessage): MessageImage[] {
  return (message.attachments ?? []).flatMap(attachment => attachment.content.flatMap(part => {
    if (part.type !== "image") return [];
    const mediaType = attachment.contentType ?? "";
    const prefix = `data:${mediaType};base64,`;
    if (!canAttachImage(mediaType) || !part.image.startsWith(prefix)) {
      throw new Error("Unable to read the attached image.");
    }
    return [{media_type: mediaType, data: part.image.slice(prefix.length)}];
  }));
}

export function sameMessageImages(first: readonly MessageImage[], second: readonly MessageImage[]): boolean {
  return first.length === second.length && first.every((image, index) => {
    const other = second.at(index);
    return other !== undefined && image.data === other.data && image.media_type === other.media_type;
  });
}
