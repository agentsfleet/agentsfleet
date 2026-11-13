import {mockStream, renderThread, postSteerMock} from "./fleet-thread/harness";
import {act, fireEvent, screen, waitFor} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {PREVIEW_FILE_LIMIT} from "@/lib/api/upload-preview";

afterEach(() => vi.unstubAllGlobals());
function uploadBoundary(reply: () => Promise<Response>) {
  const uploads = vi.fn(reply);
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const path = input instanceof Request ? input.url : String(input);
    if (path.endsWith("/uploads")) return uploads();
    if (path.includes("/tasks?")) return Promise.resolve(Response.json({items: [], next_cursor: null}));
    throw new Error(`Unexpected fixture request: ${path}`);
  });
  return uploads;
}
async function choose(file: File) {
  fireEvent.click(screen.getByRole("button", {name: "Attach a preview file"}));
  const input = document.querySelector<HTMLInputElement>('input[type="file"]');
  if (!input) throw new Error("The native attachment picker must mount its file input");
  await act(async () => {fireEvent.change(input, {target: {files: [file]}});});
}

it("shows and dismisses native preparation errors, then removes a staged file without a request", async () => {
  const network = uploadBoundary(async () => {throw new Error("Preparation must not upload");}); mockStream([]); renderThread();
  await choose(new File([new Uint8Array(PREVIEW_FILE_LIMIT + 1)], "large.bin"));
  expect((await screen.findByRole("alert")).textContent).toContain("up to 5 MiB");
  fireEvent.click(screen.getByRole("button", {name: "Dismiss attachment error"}));
  expect(screen.queryByRole("alert")).toBeNull();
  await choose(new File(["sample"], "notes.txt"));
  expect(await screen.findByText("notes.txt")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Remove attachment"}));
  await waitFor(() => expect(screen.queryByText("notes.txt")).toBeNull());
  expect(network).not.toHaveBeenCalled(); expect(postSteerMock).not.toHaveBeenCalled();
});

it("keeps a failed native upload available for removal instead of completing a receipt", async () => {
  const response = Promise.withResolvers<Response>(); uploadBoundary(() => response.promise); mockStream([]); renderThread();
  await choose(new File(["sample"], "notes.txt"));
  fireEvent.change(screen.getByRole("textbox"), {target: {value: "Review these notes"}});
  fireEvent.click(screen.getByRole("button", {name: "Send"}));
  expect(await screen.findByText("Uploading…")).toBeTruthy();
  expect(screen.getByText("Review these notes")).toBeTruthy();
  await act(async () => response.resolve(new Response("private upstream detail", {status: 503})));
  expect(await screen.findByText(/The file preview was not accepted/)).toBeTruthy();
  expect(screen.queryByText(/Upload preview · Not saved to fleet history/)).toBeNull();
  expect(screen.queryByText("private upstream detail")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Remove attachment"}));
  await waitFor(() => expect(screen.queryByText("notes.txt")).toBeNull());
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("gives a failed upload without a transport message a readable recovery action", async () => {
  uploadBoundary(async () => {throw new Error("");}); mockStream([]); renderThread();
  await choose(new File(["sample"], "notes.txt")); fireEvent.click(screen.getByRole("button", {name: "Send"}));
  expect(await screen.findByText("Upload failed. Remove this file and try again.")).toBeTruthy();
  expect(screen.getByRole("button", {name: "Remove attachment"})).toBeTruthy();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("sends through native preparation and renders the returned receipt as a temporary message", async () => {
  const network = uploadBoundary(async () => Response.json({preview: true, stored: false,
    reply: "Upload preview accepted. The file was not read, stored, or analyzed."}));
  mockStream([]); renderThread();
  await choose(new File(["sample"], "notes.txt"));
  fireEvent.click(screen.getByRole("button", {name: "Send"}));
  expect(await screen.findByText(/Upload preview · Not saved to fleet history/)).toBeTruthy();
  expect(screen.getByText("notes.txt")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Remove attachment"})).toBeNull();
  expect(network).toHaveBeenCalledOnce(); expect(postSteerMock).not.toHaveBeenCalled();
});

it("cancels a pending upload on unmount and exposes removal only in the native composer", async () => {
  const aborted = vi.fn();
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) => {
    const path = input instanceof Request ? input.url : String(input);
    if (path.includes("/tasks?")) return Promise.resolve(Response.json({items: [], next_cursor: null}));
    if (!path.endsWith("/uploads")) throw new Error(`Unexpected fixture request: ${path}`);
    return new Promise<Response>((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => {aborted(); reject(new DOMException("Removed", "AbortError"));}, {once: true});
    });
  });
  mockStream([]); const view = renderThread(); await choose(new File(["sample"], "notes.txt"));
  fireEvent.click(screen.getByRole("button", {name: "Send"}));
  expect(await screen.findByText("Uploading…")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Remove attachment"})).toBeNull();
  view.unmount();
  await waitFor(() => expect(aborted).toHaveBeenCalledOnce());
  expect(aborted).toHaveBeenCalledOnce(); expect(postSteerMock).not.toHaveBeenCalled();
  expect(screen.queryByText(/Upload preview · Not saved to fleet history/)).toBeNull();
});
