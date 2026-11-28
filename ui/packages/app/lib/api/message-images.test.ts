import type { AppendMessage, CompleteAttachment } from "@assistant-ui/react";
import {expect, it} from "vitest";
import {messageImages, sameMessageImages, type MessageImage} from "./message-images";

const PNG = {media_type: "image/png", data: "iVBORw=="};
const GIF = {media_type: "image/gif", data: "R0lGODlh"};

function image(image: string, contentType?: string): CompleteAttachment {
  return {id: "picture", name: "picture.png", type: "image", contentType,
    status: {type: "complete"}, content: [{type: "image", image}]};
}

function draft(attachments?: CompleteAttachment[]): AppendMessage {
  return {role: "user", content: [{type: "text", text: "Look at this picture"}],
    createdAt: new Date(0), parentId: null, sourceId: null, runConfig: undefined, metadata: {custom: {}}, attachments};
}

it("keeps images in file order and leaves preview receipts out of the request", () => {
  const preview: CompleteAttachment = {
    id: "notes", name: "notes.txt", type: "file", contentType: "text/plain",
    status: {type: "complete"}, content: [{type: "text", text: "Preview accepted"}],
  };
  expect(messageImages(draft())).toEqual([]);
  expect(messageImages(draft([
    image("data:image/png;base64,iVBORw==", "image/png"),
    preview,
    image("data:image/gif;base64,R0lGODlh", "image/gif"),
  ]))).toEqual([PNG, GIF]);
});

it("reports an unsupported or unreadable local attachment", () => {
  expect(() => messageImages(draft([image("data:image/png;base64,iVBORw==")]))).toThrow("Unable to read");
  expect(() => messageImages(draft([image("blob:picture", "image/png")]))).toThrow("Unable to read");
});

it("reuses a prior operation only when all attachment bytes, media types and order match", () => {
  expect(sameMessageImages([PNG, GIF], [PNG, GIF])).toBe(true);
  expect(sameMessageImages([PNG], [])).toBe(false);
  expect(sameMessageImages([PNG], Array<MessageImage>(1))).toBe(false);
  expect(sameMessageImages([PNG, GIF], [GIF, PNG])).toBe(false);
  expect(sameMessageImages([PNG], [{...PNG, media_type: GIF.media_type}])).toBe(false);
});
