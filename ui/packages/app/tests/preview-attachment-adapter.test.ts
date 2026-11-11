import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ZodError } from "zod";
import { PreviewAttachmentAdapter } from "@/lib/api/preview-attachment-adapter";
import { PREVIEW_FILE_LIMIT, PREVIEW_RECEIPT_LIMIT } from "@/lib/api/upload-preview";

const fetchMock = vi.fn();
beforeEach(() => {fetchMock.mockReset(); vi.stubGlobal("fetch", fetchMock);});
afterEach(() => vi.unstubAllGlobals());

it("uses the native pending and complete attachment states and uploads only on send", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  const pending = await adapter.add({file: new File(["demo"], "notes.txt")});
  expect(pending.status).toEqual({type: "requires-action", reason: "composer-send"});
  expect(fetchMock).not.toHaveBeenCalled();
  fetchMock.mockResolvedValue(receipt());
  const complete = await adapter.send(pending);
  expect(complete).toMatchObject({id: pending.id, name: "notes.txt", status: {type: "complete"}, content: [{type: "text", text: "Upload preview accepted. The file was not read, stored, or analyzed."}]});
  expect(complete.file).toBeUndefined();
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  expect(url).toBe("/live/v1/workspaces/workspace/fleets/fleet/uploads");
  expect(init?.body.get("file").name).toBe("notes.txt");
  adapter.dispose();
});

it("rejects oversized files before a request", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  await expect(adapter.add({file: new File([new Uint8Array(PREVIEW_FILE_LIMIT + 1)], "large.bin")})).rejects.toThrow("5 MiB");
  expect(fetchMock).not.toHaveBeenCalled();
  adapter.dispose();
});

it("requires a signed-in owner before accepting a file", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", null);
  await expect(adapter.add({file: new File(["demo"], "notes.txt")})).rejects.toThrow("Sign in");
  expect(fetchMock).not.toHaveBeenCalled();
  adapter.dispose();
});

it("rejects an acknowledgement that claims the preview was stored", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  const pending = await adapter.add({file: new File(["demo"], "notes.txt")});
  fetchMock.mockResolvedValue(Response.json({preview: true, stored: true, reply: "Stored"}));
  await expect(adapter.send(pending)).rejects.toThrow(ZodError);
  adapter.dispose();
});

it("bounds the response body instead of buffering an arbitrary receipt", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  const pending = await adapter.add({file: new File(["demo"], "notes.txt")});
  fetchMock.mockResolvedValue(new Response("x".repeat(PREVIEW_RECEIPT_LIMIT + 1)));
  await expect(adapter.send(pending)).rejects.toThrow("exceeds its limit");
  adapter.dispose();
});

it("aborts in-flight work when the owner leaves and refuses duplicate sends", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  const pending = await adapter.add({file: new File(["demo"], "notes.txt")});
  fetchMock.mockImplementation((_url: string, init: RequestInit) => new Promise((_resolve, reject) => {
    init.signal?.addEventListener("abort", () => reject(new DOMException("Stopped", "AbortError")), {once: true});
  }));
  const sending = adapter.send(pending);
  const rejection = expect(sending).rejects.toMatchObject({name: "AbortError"});
  await expect(adapter.send(pending)).rejects.toThrow("already being uploaded");
  adapter.dispose();
  await rejection;
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("returns an authentication refusal without completing the attachment", async () => {
  const adapter = new PreviewAttachmentAdapter("workspace", "fleet", "owner");
  const pending = await adapter.add({file: new File(["demo"], "notes.txt")});
  fetchMock.mockResolvedValue(Response.json({error: "Sign in"}, {status: 401}));
  await expect(adapter.send(pending)).rejects.toThrow("Sign in");
  adapter.dispose();
});

function receipt() {return Response.json({preview: true, stored: false, reply: "Upload preview accepted. The file was not read, stored, or analyzed."});}
