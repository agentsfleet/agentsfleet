import type { AppendMessage } from "@assistant-ui/react";
import { expect, it } from "vitest";
import { chatMessageText } from "./chat-message-text";

function message(quote?: unknown): AppendMessage {
  return {createdAt: new Date("2026-11-12T12:00:00Z"), role: "user", parentId: null, sourceId: null, content: [{type: "text", text: "Explain this."}],
    attachments: [], runConfig: {}, metadata: {custom: {quote}}};
}

it("persists multiline native quotes with the user's reply in the backend text", () => {
  expect(chatMessageText(message({messageId: "answer", text: "First line\r\n\nSecond line"})))
    .toBe("> First line\n> \n> Second line\n\nExplain this.");
});

it.each([undefined, null, {text: "  "}, {text: 5}])("keeps ordinary messages intact for missing or malformed quote metadata %#", quote => {
  expect(chatMessageText(message(quote))).toBe("Explain this.");
});

it("keeps all text parts without stringifying attachments or data", () => {
  const input = message();
  expect(chatMessageText({...input, content: [{type: "text", text: "One"}, {type: "image", image: "https://example.test/sample.png"}, {type: "text", text: "Two"}]})).toBe("One\nTwo");
  expect(chatMessageText({...input, content: []})).toBe("");
});
