import type { CompleteAttachment } from "@assistant-ui/react";
import { StrictMode } from "react";
import { expect, it } from "vitest";
import { act, render, screen } from "@testing-library/react";
import { appendMessage, capturedOnNew, clientUser, mockStream, postSteerMock, renderThread, threadElement } from "./fleet-thread/harness";

const attachment: CompleteAttachment = {id: "upload-preview", type: "file", name: "notes.txt", status: {type: "complete"},
  content: [{type: "text", text: "Upload preview accepted. The file was not read, stored, or analyzed."}]};

it("shows a file and a plain-text acknowledgement through the real conversation without a model call", async () => {
  mockStream([]);
  const {container} = renderThread();
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage("Please check this file"), attachments: [attachment]}));
  expect(screen.getByText("notes.txt")).toBeTruthy();
  expect(screen.getByText(/Upload preview · Not saved to fleet history/)).toBeTruthy();
  expect(container.textContent).toContain("The file was not read, stored, or analyzed.");
  expect(screen.queryByText(/Message this fleet or wait/)).toBeNull();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("discards receipts and late completions when the account changes", async () => {
  mockStream([]);
  const view = renderThread();
  const old = capturedOnNew.current;
  if (!old) throw new Error("The runtime must receive its send handler");
  await act(() => old({...appendMessage(""), attachments: [attachment]}));
  expect(screen.getByText("notes.txt")).toBeTruthy();
  clientUser.userId = "another-person";
  view.rerender(threadElement());
  expect(screen.queryByText("notes.txt")).toBeNull();
  await expect(old({...appendMessage(""), attachments: [attachment]})).rejects.toMatchObject({name: "AbortError"});
  expect(screen.queryByText("notes.txt")).toBeNull();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("keeps preview ownership usable through React StrictMode setup and cleanup", async () => {
  mockStream([]);
  render(<StrictMode>{threadElement()}</StrictMode>);
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage(""), attachments: [attachment]}));
  expect(screen.getByText("notes.txt")).toBeTruthy();
});

it("does not interpret a preview receipt as model-generated Markdown or HTML", async () => {
  mockStream([]);
  const {container} = renderThread();
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  const text = "Received **literal** <script>do-not-run</script> as a preview upload.";
  await act(() => onNew({...appendMessage(""), attachments: [{...attachment, content: [{type: "text", text}]}]}));
  expect(container.textContent).toContain(text);
  expect(container.querySelector("script, strong")).toBeNull();
});
