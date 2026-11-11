import type { AccountAccess } from "@/lib/auth/account-types";
import { useCallback, useRef } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useAccountSettings } from "@/lib/auth/account-client";

const mocks = vi.hoisted(() => ({
  user: {current: null as null | {id: string; firstName: string | null; lastName: string | null; primaryEmailAddress: {emailAddress: string} | null; hasImage: boolean; imageUrl: string; update: ReturnType<typeof vi.fn>}},
  session: {id: "session-one"}, loaded: true, gate: null as null | ReturnType<typeof Promise.withResolvers<void>>,
  provider: {openUserProfile: vi.fn()}, list: vi.fn(), revoke: vi.fn(), update: vi.fn(),
}));

// Provider hooks/challenge completion and client Server Action proxies are external boundaries.
vi.mock("@clerk/nextjs", () => ({
  useUser: () => ({isLoaded: mocks.loaded, isSignedIn: !!mocks.user.current, user: mocks.user.current}),
  useSession: () => ({isLoaded: mocks.loaded, session: mocks.user.current ? mocks.session : null}),
  useClerk: () => mocks.provider,
  useReverification: (callback: (...args: unknown[]) => Promise<unknown>) => {
    const latest = useRef(callback);
    latest.current = callback;
    return useCallback(async (...args: unknown[]) => {
      const operation = latest.current;
      if (mocks.gate) await mocks.gate.promise;
      return operation(...args);
    }, []);
  },
}));
vi.mock("@/lib/auth/account-actions", () => ({listAccountSessionsAction: mocks.list, revokeAccountSessionAction: mocks.revoke}));

beforeEach(() => {
  mocks.loaded = true;
  mocks.session = {id: "session-one"};
  mocks.gate = null;
  mocks.update.mockReset().mockResolvedValue(undefined);
  mocks.list.mockReset().mockResolvedValue({ok: true, data: {sessions: [], nextOffset: null}});
  mocks.revoke.mockReset().mockResolvedValue({ok: true, data: {id: "other"}});
  mocks.provider.openUserProfile.mockReset();
  mocks.user.current = {id: "owner", firstName: "Kishore", lastName: "", primaryEmailAddress: {emailAddress: "you@example.test"}, hasImage: false, imageUrl: "unused", update: mocks.update};
});
afterEach(cleanup);

it("returns only app-owned account data and preserves loading/signed-out states", () => {
  mocks.loaded = false;
  const hook = renderHook(useAccountSettings);
  expect(hook.result.current).toEqual({status: "loading"});
  mocks.loaded = true;
  mocks.user.current = null;
  hook.rerender();
  expect(hook.result.current).toEqual({status: "signed-out"});
  expect(mocks.list).not.toHaveBeenCalled();
});

it("trims bounded profile fields, keeps provider resources private and retains secure controls", async () => {
  const hook = renderHook(useAccountSettings);
  const access = ready(hook.result.current);
  expect(access.profile).toEqual({subject: "owner", firstName: "Kishore", lastName: "", email: "you@example.test", imageUrl: null});
  expect(await access.client.updateProfile({firstName: " Indy ", lastName: ""})).toEqual({ok: true, data: null});
  expect(mocks.update).toHaveBeenCalledExactlyOnceWith({firstName: "Indy", lastName: ""});
  expect(await access.client.updateProfile({firstName: "a".repeat(257), lastName: ""})).toMatchObject({ok: false});
  expect(mocks.update).toHaveBeenCalledTimes(1);
  access.client.manageSignIn();
  expect(mocks.provider.openUserProfile).toHaveBeenCalledOnce();
});

it("preserves client identity during ordinary same-owner resource reloads", () => {
  const hook = renderHook(useAccountSettings);
  const before = ready(hook.result.current).client;
  if (!mocks.user.current) throw new Error("A signed-in fixture is required");
  mocks.user.current = {...mocks.user.current, firstName: "Indy"};
  hook.rerender();
  expect(ready(hook.result.current).client).toBe(before);
  expect(ready(hook.result.current).profile.firstName).toBe("Indy");
});

it("dispatches current-owner session operations and reports unknown mutation outcomes without provider internals", async () => {
  const hook = renderHook(useAccountSettings);
  const api = ready(hook.result.current).client;
  expect(await api.listSessions(50)).toEqual({ok: true, data: {sessions: [], nextOffset: null}});
  expect(mocks.list).toHaveBeenCalledExactlyOnceWith("owner", 50);
  expect(await api.revokeSession("other")).toEqual({ok: true, data: {id: "other"}});
  expect(mocks.revoke).toHaveBeenCalledExactlyOnceWith("owner", "other");
  mocks.revoke.mockRejectedValue(new Error("private provider detail"));
  const result = await api.revokeSession("other");
  expect(result).toMatchObject({ok: false, error: expect.stringContaining("could not be confirmed")});
  expect(JSON.stringify(result)).not.toContain("private provider detail");
});

it.each(["switch", "round-trip"])("rejects a pending sign-out challenge after a same-user session %s", async transition => {
  const hook = renderHook(useAccountSettings);
  const before = ready(hook.result.current).client;
  const gate = Promise.withResolvers<void>(); mocks.gate = gate;
  const pending = before.revokeSession("other");
  mocks.session = {id: "session-two"}; hook.rerender();
  if (transition === "round-trip") {mocks.session = {id: "session-one"}; hook.rerender();}
  await act(async () => gate.resolve());
  expect(await pending).toMatchObject({ok: false, status: 409});
  expect(mocks.revoke).not.toHaveBeenCalled();
});

it("does not revive a pending profile change after an account round trip", async () => {
  const hook = renderHook(useAccountSettings);
  const before = ready(hook.result.current).client;
  const original = mocks.user.current;
  const gate = Promise.withResolvers<void>(); mocks.gate = gate;
  const pending = before.updateProfile({firstName: "Stale", lastName: ""});
  if (!original) throw new Error("A signed-in fixture is required");
  mocks.user.current = {...original, id: "other-person"}; hook.rerender();
  mocks.user.current = original; hook.rerender();
  await act(async () => gate.resolve());
  expect(await pending).toMatchObject({ok: false});
  expect(mocks.update).not.toHaveBeenCalled();
});

it("drops late session reads after the account changes and blocks stale clients", async () => {
  const response = Promise.withResolvers<Awaited<ReturnType<typeof mocks.list>>>();
  mocks.list.mockReturnValue(response.promise);
  const hook = renderHook(useAccountSettings);
  const before = ready(hook.result.current).client;
  const pending = before.listSessions();
  if (!mocks.user.current) throw new Error("A signed-in fixture is required");
  mocks.user.current = {...mocks.user.current, id: "other-person"}; hook.rerender();
  response.resolve({ok: true, data: {sessions: [], nextOffset: null}});
  expect(await pending).toMatchObject({ok: false, status: 409});
  expect(await before.revokeSession("other")).toMatchObject({ok: false, status: 409});
  expect(await before.listSessions()).toMatchObject({ok: false, status: 409});
  expect(await before.updateProfile({firstName: "Stale", lastName: ""})).toMatchObject({ok: false});
  before.manageSignIn();
  expect(mocks.provider.openUserProfile).not.toHaveBeenCalled();
  expect(mocks.revoke).not.toHaveBeenCalled();
});

it("keeps malformed and failed session responses unavailable", async () => {
  const hook = renderHook(useAccountSettings);
  const api = ready(hook.result.current).client;
  mocks.list.mockResolvedValueOnce({ok: true, data: {sessions: [{id: "opaque"}], nextOffset: null}});
  expect(await api.listSessions()).toMatchObject({ok: false});
  mocks.list.mockResolvedValueOnce({ok: false, error: "Read denied", status: 403});
  expect(await api.listSessions()).toMatchObject({ok: false, status: 403, error: "Read denied"});
});

it("maps absent personal fields without inventing a name or address", () => {
  if (!mocks.user.current) throw new Error("A signed-in fixture is required");
  mocks.user.current = {...mocks.user.current, firstName: null, lastName: null, primaryEmailAddress: null, hasImage: true, imageUrl: "https://example.test/photo"};
  expect(ready(renderHook(useAccountSettings).result.current).profile).toMatchObject({firstName: "", lastName: "", email: null, imageUrl: "https://example.test/photo"});
});

it("drops a profile save acknowledgement when its owner changes after dispatch", async () => {
  const gate = Promise.withResolvers<void>(); mocks.update.mockReturnValue(gate.promise);
  const hook = renderHook(useAccountSettings); const before = ready(hook.result.current).client;
  const pending = before.updateProfile({firstName: "Indy", lastName: ""});
  mocks.user.current = null; hook.rerender(); gate.resolve();
  expect(await pending).toMatchObject({ok: false, status: 409});
  expect(mocks.update).toHaveBeenCalledOnce();
});

it("drops a sign-out acknowledgement when its owner changes after dispatch", async () => {
  const gate = Promise.withResolvers<{ok: true; data: {id: string}}>(); mocks.revoke.mockReturnValue(gate.promise);
  const hook = renderHook(useAccountSettings); const before = ready(hook.result.current).client;
  const pending = before.revokeSession("other");
  mocks.user.current = null; hook.rerender(); gate.resolve({ok: true, data: {id: "other"}});
  expect(await pending).toMatchObject({ok: false, status: 409});
  expect(mocks.revoke).toHaveBeenCalledOnce();
});

function ready(access: AccountAccess) {
  if (access.status !== "ready") throw new Error("Account settings must be ready");
  return access;
}
