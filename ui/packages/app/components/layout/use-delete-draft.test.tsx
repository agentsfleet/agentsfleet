import { StrictMode, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
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
  expect(removed).not.toHaveBeenCalled();
  await act(() => result.current.remove(DRAFT_ID));
  expect(result.current.error).toBeNull();
  expect(removed).toHaveBeenCalledOnce();
});

it("aborts an outstanding delete on unmount and ignores its late success", async () => {
  const done = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockReturnValue(done.promise); vi.stubGlobal("fetch", fetcher);
  const removed = vi.fn();
  const {result, unmount} = renderHook(() => useDeleteDraft(WORKSPACE, null, removed));
  const pending = result.current.remove(DRAFT_ID);
  await waitFor(() => expect(fetcher).toHaveBeenCalledOnce());
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

it("aborts a superseded delete without releasing the replacement request", async () => {
  const first = Promise.withResolvers<void>();
  const second = Promise.withResolvers<void>();
  const action = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const removed = vi.fn();
  const {result, unmount} = renderHook(() => useDeleteDraft(WORKSPACE, action, removed));
  let old!: Promise<void>;
  let current!: Promise<void>;
  act(() => {old = result.current.remove("first");});
  const refused = expect(old).rejects.toMatchObject({name: "AbortError"});
  act(() => {current = result.current.remove("second");});
  expect(action.mock.calls[0]?.[1].aborted).toBe(true);
  await act(async () => {first.resolve(); await refused;});
  expect(action.mock.calls[1]?.[1].aborted).toBe(false);
  expect(removed).not.toHaveBeenCalled();
  const stopped = expect(current).rejects.toMatchObject({name: "AbortError"});
  unmount();
  expect(action.mock.calls[1]?.[1].aborted).toBe(true);
  second.resolve(); await stopped;
  expect(removed).not.toHaveBeenCalled();
});
