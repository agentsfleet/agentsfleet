import { StrictMode, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useDeleteDraft } from "./use-delete-draft";
import { DRAFT_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("deletes through the active chat owner and removes the card only after success", async () => {
  const done = Promise.withResolvers<void>();
  const action = vi.fn(() => done.promise);
  const removed = vi.fn();
  const {result} = renderHook(() => useDeleteDraft(WORKSPACE, action, removed));
  const pending = act(() => result.current.remove(DRAFT_ID));
  expect(removed).not.toHaveBeenCalled();
  done.resolve(); await pending;
  expect(action).toHaveBeenCalledWith(DRAFT_ID, expect.any(AbortSignal));
  expect(removed).toHaveBeenCalledExactlyOnceWith(DRAFT_ID);
  expect(result.current.pending).toBe(false);
});

it("uses the typed delete endpoint outside chat, including after strict-mode effect replay", async () => {
  const fetcher = vi.fn().mockResolvedValue(response(null, 204)); vi.stubGlobal("fetch", fetcher);
  const removed = vi.fn();
  const wrapper = ({children}: {children: ReactNode}) => <StrictMode>{children}</StrictMode>;
  const {result} = renderHook(() => useDeleteDraft(WORKSPACE, null, removed), {wrapper});
  await act(() => result.current.remove(DRAFT_ID));
  expect(fetcher).toHaveBeenCalledWith(`/live/v1/workspaces/${WORKSPACE}/drafts/${DRAFT_ID}`, expect.objectContaining({method: "DELETE", credentials: "same-origin"}));
  expect(removed).toHaveBeenCalledExactlyOnceWith(DRAFT_ID);
});

it("keeps a refused chat and displays the failure until a successful retry", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(response({user_message: "Finish the current request first"}, 409)).mockResolvedValueOnce(response(null, 204));
  vi.stubGlobal("fetch", fetcher); const removed = vi.fn();
  const {result} = renderHook(() => useDeleteDraft(WORKSPACE, null, removed));
  await act(async () => {await expect(result.current.remove(DRAFT_ID)).rejects.toMatchObject({status: 409});});
  expect(result.current.error).toBe("Finish the current request first");
  expect(result.current.pending).toBe(false); expect(removed).not.toHaveBeenCalled();
  act(() => result.current.clearError()); expect(result.current.error).toBeNull();
  await act(() => result.current.remove(DRAFT_ID));
  expect(removed).toHaveBeenCalledOnce();
});

it("aborts an outstanding delete on unmount and ignores its late success", async () => {
  const done = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockReturnValue(done.promise); vi.stubGlobal("fetch", fetcher);
  const removed = vi.fn();
  const {result, unmount} = renderHook(() => useDeleteDraft(WORKSPACE, null, removed));
  const pending = result.current.remove(DRAFT_ID);
  const signal = fetcher.mock.calls[0]?.[1]?.signal;
  unmount(); expect(signal.aborted).toBe(true);
  done.resolve(response(null, 204));
  await expect(pending).rejects.toMatchObject({status: 502});
  expect(removed).not.toHaveBeenCalled();
});

it("rejects an unexpected successful response instead of removing a card", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({deleted: false})));
  const removed = vi.fn(); const {result} = renderHook(() => useDeleteDraft(WORKSPACE, null, removed));
  await act(async () => {await expect(result.current.remove(DRAFT_ID)).rejects.toMatchObject({status: 502});});
  expect(removed).not.toHaveBeenCalled(); expect(result.current.error).toBeTruthy();
});
