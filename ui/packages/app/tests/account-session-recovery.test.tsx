import type {AccountSession, AccountSessionPage, AccountSettingsClient} from "@/lib/auth/account-types";
import type {ActionResult} from "@/lib/actions/with-token";
import {act, cleanup, renderHook, waitFor} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {useAccountSessions} from "@/components/account/use-account-sessions";

afterEach(cleanup);
const SESSION: AccountSession = {id: "other", browser: "Chrome", device: "macOS", current: false, mobile: false, location: null, lastActiveAt: null};
const PAGE: AccountSessionPage = {sessions: [SESSION], nextOffset: null};
function client(): AccountSettingsClient {
  return {listSessions: vi.fn().mockResolvedValue({ok: true, data: PAGE}), revokeSession: vi.fn().mockResolvedValue({ok: true, data: {id: SESSION.id}}),
    updateProfile: vi.fn(), manageSignIn: vi.fn()};
}

it("keeps an initial read failure retryable and deduplicates sessions across pages", async () => {
  const api = client(); vi.mocked(api.listSessions).mockRejectedValueOnce(new Error("Disconnected"));
  const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("failed"));
  await act(() => hook.result.current.refresh());
  expect(hook.result.current.state).toMatchObject({kind: "ready", page: PAGE});
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {...PAGE, sessions: [{...SESSION, browser: "Safari"}]}});
  await act(() => hook.result.current.refresh(1, true));
  expect(hook.result.current.state).toMatchObject({kind: "ready", page: {sessions: [{id: "other", browser: "Safari"}]}});
});

it("excludes concurrent reads and sign-outs while the current operation is pending", async () => {
  const api = client(); const gate = Promise.withResolvers<ActionResult<AccountSessionPage>>();
  vi.mocked(api.listSessions).mockReturnValue(gate.promise);
  const hook = renderHook(() => useAccountSessions(api));
  await act(async () => {await hook.result.current.refresh(); expect(await hook.result.current.revoke("other")).toBe(false);});
  expect(api.listSessions).toHaveBeenCalledOnce(); expect(api.revokeSession).not.toHaveBeenCalled();
  await act(async () => gate.resolve({ok: true, data: PAGE}));
  const revoke = Promise.withResolvers<ActionResult<{id: string}>>(); vi.mocked(api.revokeSession).mockReturnValue(revoke.promise);
  let pending = Promise.resolve(false);
  act(() => {pending = hook.result.current.revoke("other");});
  await act(async () => {expect(await hook.result.current.revoke("other")).toBe(false);});
  await act(async () => revoke.resolve({ok: true, data: {id: "other"}}));
  expect(await pending).toBe(true); expect(api.revokeSession).toHaveBeenCalledOnce();
});

it("retains known rows after a thrown refresh or sign-out transport error", async () => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.listSessions).mockRejectedValueOnce(new Error("Read disconnected"));
  await act(() => hook.result.current.refresh());
  expect(hook.result.current.state).toMatchObject({kind: "ready", page: PAGE, error: expect.stringContaining("could not be loaded")});
  vi.mocked(api.revokeSession).mockRejectedValueOnce(new Error("Write disconnected"));
  await act(async () => {expect(await hook.result.current.revoke("other")).toBe(false);});
  expect(hook.result.current.state).toMatchObject({kind: "ready", page: PAGE, error: expect.stringContaining("could not be confirmed")});
});

it("requires a confirmed list before sign-out and discards rejected reads after unmount", async () => {
  const api = client(); vi.mocked(api.listSessions).mockResolvedValueOnce({ok: false, error: "List unavailable"});
  const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("failed"));
  expect(await hook.result.current.revoke("other")).toBe(false);
  expect(api.revokeSession).not.toHaveBeenCalled();
  const gate = Promise.withResolvers<ActionResult<AccountSessionPage>>(); vi.mocked(api.listSessions).mockReturnValue(gate.promise);
  let pending = Promise.resolve(); act(() => {pending = hook.result.current.refresh();});
  hook.unmount(); gate.reject(new Error("Late connection failure"));
  await pending;
});

it.each([false, true])("drops a late sign-out after unmount, including rejected promises (%s)", async reject => {
  const api = client(); const gate = Promise.withResolvers<ActionResult<{id: string}>>();
  const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.revokeSession).mockReturnValue(gate.promise);
  let pending = Promise.resolve(false); act(() => {pending = hook.result.current.revoke("other");});
  hook.unmount();
  if (reject) gate.reject(new Error("Connection lost")); else gate.resolve({ok: true, data: {id: "other"}});
  expect(await pending).toBe(false); expect(api.listSessions).toHaveBeenCalledOnce();
});
