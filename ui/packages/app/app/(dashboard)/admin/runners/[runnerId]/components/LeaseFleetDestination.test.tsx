import { StrictMode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FLEET_DETAIL, FLEET_ID, REVISION_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { LeaseFleetDestination } from "./LeaseFleetDestination";

const fetchMock = vi.fn<typeof fetch>();
beforeEach(() => {fetchMock.mockReset(); vi.stubGlobal("fetch", fetchMock);});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function show() {return render(<LeaseFleetDestination workspaceId={WORKSPACE} fleetId={FLEET_ID} />);}

it("offers a destination only after a current typed fleet read succeeds", async () => {
  const pending = Promise.withResolvers<Response>(); fetchMock.mockReturnValueOnce(pending.promise);
  show();
  expect(screen.getByRole("status").textContent).toContain("Checking fleet");
  expect(screen.queryByRole("link")).toBeNull();
  await act(async () => pending.resolve(response(FLEET_DETAIL)));
  expect((await screen.findByRole("link", {name: /Open fleet/})).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fetchMock).toHaveBeenCalledWith(`/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}`, expect.objectContaining({credentials: "same-origin", cache: "no-store"}));
});
it.each([401, 403, 404, 410])("keeps a missing or inaccessible fleet from becoming a broken link: %s", async (status) => {
  fetchMock.mockResolvedValueOnce(response({}, status)); show();
  expect(await screen.findByText(/This fleet is no longer available/)).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
});
it("keeps a temporary refusal visible and lets the operator retry", async () => {
  fetchMock.mockResolvedValueOnce(response({}, 503)).mockResolvedValueOnce(response(FLEET_DETAIL)); show();
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  expect(await screen.findByRole("link", {name: /Open fleet/})).toBeTruthy();
});
it.each([{}, {...FLEET_DETAIL, id: REVISION_ID}])("rejects a malformed or wrong fleet response", async (value) => {
  fetchMock.mockResolvedValueOnce(response(value)); show();
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
});
it("retains retry after a connection failure", async () => {
  fetchMock.mockRejectedValueOnce(new TypeError("Connection lost")); show();
  expect(await screen.findByRole("button", {name: "Retry"})).toBeTruthy();
  expect(screen.queryByText("Connection lost")).toBeNull();
});
it.each(["success", "failure"])("cancels the read when the dialog closes and ignores its late %s", async (outcome) => {
  const pending = Promise.withResolvers<Response>(); fetchMock.mockReturnValueOnce(pending.promise);
  const view = show(); const signal = fetchMock.mock.calls[0]?.[1]?.signal;
  view.unmount(); expect(signal?.aborted).toBe(true);
  await act(async () => {
    if (outcome === "success") pending.resolve(response(FLEET_DETAIL));
    else pending.reject(new TypeError("Connection lost"));
  });
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});
it("creates a fresh cancellation owner after Strict Mode cleanup", async () => {
  fetchMock.mockImplementation(async () => response(FLEET_DETAIL));
  render(<StrictMode><LeaseFleetDestination workspaceId={WORKSPACE} fleetId={FLEET_ID} /></StrictMode>);
  expect(await screen.findByRole("link", {name: /Open fleet/})).toBeTruthy();
  expect(fetchMock.mock.calls[0]?.[1]?.signal?.aborted).toBe(true);
  expect(fetchMock.mock.calls[1]?.[1]?.signal?.aborted).toBe(false);
});
it("ignores a buffered success when the dialog closes before its continuation runs", async () => {
  const ready = Promise.withResolvers<ReadableStreamDefaultController<Uint8Array>>();
  const stream = new ReadableStream<Uint8Array>({start: (controller) => ready.resolve(controller)});
  const controller = await ready.promise;
  fetchMock.mockResolvedValueOnce(new Response(stream));
  const view = show();
  await act(async () => controller.enqueue(new TextEncoder().encode(JSON.stringify(FLEET_DETAIL))));
  await act(async () => {
    controller.close();
    // Close after the buffered read finishes, before its promise continuation.
    queueMicrotask(() => queueMicrotask(() => queueMicrotask(() => view.unmount())));
  });
  expect(fetchMock.mock.calls[0]?.[1]?.signal?.aborted).toBe(true);
  expect(screen.queryByRole("link")).toBeNull();
});
it("does not carry an authorized destination into a different workspace", async () => {
  fetchMock.mockResolvedValueOnce(response(FLEET_DETAIL)).mockResolvedValueOnce(response({}, 404));
  const view = show(); await screen.findByRole("link", {name: /Open fleet/});
  view.rerender(<LeaseFleetDestination workspaceId="other/workspace" fleetId={FLEET_ID} />);
  expect(screen.queryByRole("link")).toBeNull();
  expect(await screen.findByText(/This fleet is no longer available/)).toBeTruthy();
  expect(fetchMock.mock.calls[1]?.[0]).toContain("other%2Fworkspace");
});
