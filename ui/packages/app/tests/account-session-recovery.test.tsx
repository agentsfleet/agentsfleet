import type {AccountSession, AccountSessionPage, AccountSettingsClient} from "@/lib/auth/account-types";
import type {ActionResult} from "@/lib/actions/with-token";
import {act, cleanup, renderHook, waitFor} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {useAccountSessions} from "@/components/account/use-account-sessions";

afterEach(cleanup);
const SESSION: AccountSession = {id: "other", browser: "Chrome", device: "macOS", current: false, mobile: false, location: null, lastActiveAt: null};
const PAGE: AccountSessionPage = {sessions: [SESSION], nextOffset: null};
function client(): AccountSettingsClient {
  return {listSessions: vi.fn().mockResolvedValue({ok: true, data: PAGE}), revokeSession: vi.fn().mockResolvedValue({ok: true, data: {id: SESSION.id}})};
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

// Value: protects=complete bulk sign-out with account isolation; fails_when=unread sessions are skipped or this device is revoked;
// why_new=Security now exposes bulk sign-out; seam=provider facade; no_claim=Clerk transport.
it("collects all pages before revoking and excludes any session identified as this device", async () => {
  const api = client();
  const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {sessions: [SESSION, {...SESSION, id: "mine"}], nextOffset: 2}})
    .mockResolvedValueOnce({ok: true, data: {sessions: [{...SESSION, id: "mine", current: true}, {...SESSION, id: "last"}], nextOffset: null}});
  await act(async () => expect(await hook.result.current.revokeOthers()).toBe(true));
  expect(api.revokeSession).toHaveBeenCalledTimes(2);
  expect(api.revokeSession).toHaveBeenNthCalledWith(1, "other");
  expect(api.revokeSession).toHaveBeenNthCalledWith(2, "last");
  expect(hook.result.current.state).toMatchObject({notice: "Other sessions signed out"});
});

it.each(["refused", "repeated"])("does not start bulk changes when a later page is %s", async problem => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {sessions: [SESSION], nextOffset: 1}})
    .mockResolvedValueOnce(problem === "refused" ? {ok: false, error: "Access changed"} : {ok: true, data: {sessions: [SESSION], nextOffset: 1}});
  await act(async () => expect(await hook.result.current.revokeOthers()).toBe(false));
  expect(api.revokeSession).not.toHaveBeenCalled();
  expect(hook.result.current.state).toMatchObject({error: expect.stringContaining("could not be confirmed")});
});

it("stops bulk sign-out at the first refusal without claiming every device was signed out", async () => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {sessions: [SESSION, {...SESSION, id: "last"}], nextOffset: null}});
  vi.mocked(api.revokeSession).mockResolvedValueOnce({ok: false, error: "Session ownership changed"});
  await act(async () => expect(await hook.result.current.revokeOthers()).toBe(false));
  expect(api.revokeSession).toHaveBeenCalledOnce();
  expect(hook.result.current.state).toMatchObject({notice: null, error: expect.stringContaining("Some sessions may remain")});
});

it("does not start a bulk write after the account owning its pending read is disposed", async () => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  const page = Promise.withResolvers<ActionResult<AccountSessionPage>>();
  vi.mocked(api.listSessions).mockReturnValueOnce(page.promise);
  let operation = Promise.resolve(false);
  act(() => {operation = hook.result.current.revokeOthers();});
  hook.unmount(); page.resolve({ok: true, data: PAGE});
  expect(await operation).toBe(false);
  expect(api.revokeSession).not.toHaveBeenCalled();
});

it("removes confirmed bulk sign-outs even when the subsequent list refresh fails", async () => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: PAGE})
    .mockResolvedValueOnce({ok: false, error: "Refresh unavailable"});
  await act(async () => expect(await hook.result.current.revokeOthers()).toBe(true));
  expect(hook.result.current.state).toMatchObject({page: {sessions: []}, notice: "Other sessions signed out",
    error: "The session list could not be refreshed. Try refreshing it again."});
});

it("refuses bulk writes before a complete list exists and bounds a provider that never ends paging", async () => {
  const api = client();
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: false, error: "List unavailable"});
  const hook = renderHook(() => useAccountSessions(api));
  expect(await hook.result.current.revokeOthers()).toBe(false);
  await waitFor(() => expect(hook.result.current.state.kind).toBe("failed"));
  expect(await hook.result.current.revokeOthers()).toBe(false);
  await act(() => hook.result.current.refresh());
  vi.mocked(api.listSessions).mockImplementation(async (offset = 0) => ({ok: true, data: {sessions: [SESSION], nextOffset: offset + 1}}));
  await act(async () => expect(await hook.result.current.revokeOthers()).toBe(false));
  expect(api.listSessions).toHaveBeenCalledTimes(102);
  expect(api.revokeSession).not.toHaveBeenCalled();
});

it.each(["paged-read", "empty-list", "write", "refresh"])("discards bulk sign-out after disposal during %s", async stage => {
  const api = client(); const hook = renderHook(() => useAccountSessions(api));
  await waitFor(() => expect(hook.result.current.state.kind).toBe("ready"));
  const read = Promise.withResolvers<ActionResult<AccountSessionPage>>();
  const write = Promise.withResolvers<ActionResult<{id: string}>>();
  if (stage === "write") vi.mocked(api.revokeSession).mockReturnValueOnce(write.promise);
  else if (stage === "refresh") vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: PAGE}).mockReturnValueOnce(read.promise);
  else vi.mocked(api.listSessions).mockReturnValueOnce(read.promise);
  let operation = Promise.resolve(false); act(() => {operation = hook.result.current.revokeOthers();});
  if (stage === "write") await waitFor(() => expect(api.revokeSession).toHaveBeenCalledOnce());
  if (stage === "refresh") await waitFor(() => expect(api.listSessions).toHaveBeenCalledTimes(3));
  hook.unmount();
  write.resolve({ok: true, data: {id: SESSION.id}});
  read.resolve({ok: true, data: stage === "paged-read" ? {...PAGE, nextOffset: 1} : {sessions: [], nextOffset: null}});
  expect(await operation).toBe(false);
  expect(api.revokeSession).toHaveBeenCalledTimes(stage === "write" || stage === "refresh" ? 1 : 0);
});
