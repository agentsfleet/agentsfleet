import type { CompleteAttachment } from "@assistant-ui/react";
import { StrictMode } from "react";
import { expect, it, vi } from "vitest";
import { act, render, renderHook, screen } from "@testing-library/react";
import { appendMessage, capturedOnNew, clientUser, ev, mockStream, postSteerMock, renderThread, SUBJECT, threadElement, WS, ZID, FLEET_NAME } from "./fleet-thread/harness";
import {FleetThread} from "@/components/domain/FleetThread";
import { ACCEPTED } from "./fleet-thread/steer-helpers";
import {useFleetUploadPreviews} from "@/components/domain/useFleetUploadPreviews";

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
  await expect(old(appendMessage("Stale normal send"))).rejects.toMatchObject({name: "AbortError"});
  expect(screen.queryByText("notes.txt")).toBeNull();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("keeps a preview before later sends, reconciled replies and incoming events", async () => {
  const earlier = ev({id: "earlier", role: "user", actor: `steer:${SUBJECT}`, text: "Earlier question", reply: "Earlier answer"});
  mockStream([earlier]);
  const view = renderThread();
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage("Preview notes"), attachments: [attachment]}));
  assertBefore(screen.getByText("Earlier answer"), screen.getByText("notes.txt"));
  postSteerMock.mockResolvedValue(ACCEPTED("later"));
  await act(() => onNew(appendMessage("Later question")));
  const later = ev({id: "optimistic-later", role: "user", actor: `steer:${SUBJECT}`, text: "Later question", status: "optimistic", submittedAtMs: 200});
  mockStream([earlier, later]); view.rerender(threadElement());
  assertBefore(screen.getByText("notes.txt"), screen.getByText("Later question"));
  const incoming = ev({id: "webhook", role: "system", actor: "github", text: "Webhook request", reply: "Webhook was received"});
  mockStream([earlier, {...later, id: "later", status: "processed", reply: "Later answer"}, incoming]);
  view.rerender(threadElement());
  assertBefore(screen.getByText("notes.txt"), screen.getByText("Later question"));
  assertBefore(screen.getByText("notes.txt"), screen.getByText("Webhook was received"));
  expect(postSteerMock).toHaveBeenCalledOnce();
});

it("retains a preview's position when its preceding optimistic event receives a server id", async () => {
  const earlier = ev({id: "optimistic-earlier", role: "user", actor: `steer:${SUBJECT}`, text: "Earlier question", status: "optimistic", submittedAtMs: 100});
  mockStream([earlier]);
  const view = renderThread();
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage("Preview notes"), attachments: [attachment]}));
  const later = ev({id: "later", role: "user", actor: `steer:${SUBJECT}`, text: "Later question"});
  mockStream([{...earlier, id: "server-earlier", status: "processed", reply: "Earlier confirmed answer"}, later]);
  view.rerender(threadElement());
  assertBefore(screen.getByText("Earlier confirmed answer"), screen.getByText("notes.txt"));
  assertBefore(screen.getByText("notes.txt"), screen.getByText("Later question"));
});

it("does not group identical integration events across an intervening upload preview", async () => {
  const first = ev({id: "hook-one", role: "system", actor: "github", text: "Issue webhook", reply: ""});
  mockStream([first]);
  const view = renderThread();
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage("Preview notes"), attachments: [attachment]}));
  mockStream([first, {...first, id: "hook-two"}]); view.rerender(threadElement());
  expect(screen.getByText("notes.txt")).toBeTruthy();
  expect(screen.queryByTestId("group-count")).toBeNull();
});

it("inserts a receipt after an existing activity group and keeps it when a newer event arrives", async () => {
  const first = ev({id: "hook-one", role: "system", actor: "github", text: "Issue webhook", reply: ""});
  const second = {...first, id: "hook-two"}; mockStream([first, second]);
  const view = renderThread(); const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage(""), attachments: [attachment]}));
  assertBefore(screen.getByRole("button", {name: /×2/}), screen.getByText("notes.txt"));
  mockStream([first, second, ev({id: "next", role: "system", actor: "slack", text: "New request", reply: "New answer"})]);
  view.rerender(threadElement()); assertBefore(screen.getByText("notes.txt"), screen.getByText("New answer"));
});

it("refuses an attachment receipt while the current signed-in owner is unknown", async () => {
  clientUser.userId = null; mockStream([]);
  render(<FleetThread workspaceId={WS} fleetId={ZID} senderLabel={FLEET_NAME} viewer={null} initial={[]} senderNames={[]} />);
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await expect(onNew({...appendMessage(""), attachments: [attachment]})).rejects.toThrow("Sign in");
  expect(screen.queryByText("notes.txt")).toBeNull();
});

it("keeps preview ownership usable through React StrictMode setup and cleanup", async () => {
  mockStream([]);
  render(<StrictMode>{threadElement()}</StrictMode>);
  const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage(""), attachments: [attachment]}));
  expect(screen.getByText("notes.txt")).toBeTruthy();
});

it("retains only the latest four temporary uploads without losing durable conversation", async () => {
  mockStream([ev({id: "durable", role: "user", actor: `steer:${SUBJECT}`, text: "Keep this message", reply: "Durable answer"})]);
  renderThread(); const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  for (const index of [1, 2, 3, 4, 5]) {
    await act(() => onNew({...appendMessage(`Preview ${index}`), attachments: [{...attachment, id: `file-${index}`, name: `notes-${index}.txt`}]}));
  }
  expect(screen.queryByText("notes-1.txt")).toBeNull();
  for (const index of [2, 3, 4, 5]) expect(screen.getByText(`notes-${index}.txt`)).toBeTruthy();
  expect(screen.getByText("Keep this message")).toBeTruthy();
  expect(screen.getByText("Durable answer")).toBeTruthy();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("keeps a receipt at the oldest visible boundary when every saved event anchor leaves the stream window", async () => {
  mockStream([ev({id: "older", role: "user", actor: `steer:${SUBJECT}`, text: "Earlier question"})]);
  const view = renderThread(); const onNew = capturedOnNew.current;
  if (!onNew) throw new Error("The runtime must receive its send handler");
  await act(() => onNew({...appendMessage("Preview notes"), attachments: [attachment]}));
  mockStream([ev({id: "newer", role: "system", actor: "github", text: "New request", reply: "New response"})]);
  view.rerender(threadElement());
  expect(screen.queryByText("Earlier question")).toBeNull();
  assertBefore(screen.getByText("notes.txt"), screen.getByText("New response"));
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("isolates the upload hook's own account state and reads only text from native rich parts", async () => {
  const onNew = vi.fn();
  const hook = renderHook(({subject}) => useFleetUploadPreviews({workspaceId: WS, fleetId: ZID, subject, events: [], onNew}), {initialProps: {subject: "first-person"}});
  const first = hook.result.current.onNew;
  await act(() => first({...appendMessage("Visible caption"), content: [{type: "text", text: "Visible caption"}, {type: "image", image: "https://example.test/private-image"}],
    attachments: [{...attachment, content: [...attachment.content, {type: "file", filename: "private.txt", mimeType: "text/plain", data: "encoded-body"}]}]}));
  const text = JSON.stringify(hook.result.current.entries.map(entry => entry.message.content));
  expect(text).not.toContain("encoded-body"); expect(text).not.toContain("private-image");
  hook.rerender({subject: "second-person"});
  expect(hook.result.current.entries).toEqual([]);
  await expect(first({...appendMessage("Old caption"), attachments: [attachment]})).rejects.toMatchObject({name: "AbortError"});
  await act(() => hook.result.current.onNew({...appendMessage("New caption"), attachments: [{...attachment, id: "new-file"}]}));
  expect(hook.result.current.entries).toHaveLength(2);
  expect(JSON.stringify(hook.result.current.entries)).toContain("New caption");
  expect(JSON.stringify(hook.result.current.entries)).not.toContain("Visible caption");
  expect(onNew).not.toHaveBeenCalled();
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

function assertBefore(first: HTMLElement, second: HTMLElement) {
  expect(Boolean(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(true);
}
