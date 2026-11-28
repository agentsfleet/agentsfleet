import React from "react";
import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SignInPage from "@/app/(auth)/sign-in/[[...sign-in]]/page";
import SignUpPage from "@/app/(auth)/sign-up/[[...sign-up]]/page";
import { AuthProvider, AuthSessionKeeper, useCurrentUser, useSessionRefresh } from "./client";

const { reload, useClerk, useSession, useUser } = vi.hoisted(() => ({
  reload: vi.fn<() => Promise<unknown>>(),
  useClerk: vi.fn(),
  useSession: vi.fn(),
  useUser: vi.fn(),
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => children,
  SignIn: ({fallback}: {fallback?: React.ReactNode}) => fallback,
  SignUp: ({fallback}: {fallback?: React.ReactNode}) => fallback,
  UserButton: () => null,
  useClerk,
  useSession,
  useUser,
}));

const USER_ID = "user_session_keeper_fixture";

describe("auth page loading", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it.each([[SignInPage, "Loading sign-in…"], [SignUpPage, "Loading sign-up…"]] as const)(
    "announces that the auth widget is starting",
    (Page, label) => {
      useClerk.mockReturnValue({status: "loading"});
      render(<Page />);
      const status = screen.getByRole("status", {name: label});
      expect(status.textContent).toBe(label);
      expect(status.closest("[data-dashboard-panel]")).toBeNull();
      expect(status.querySelectorAll(".page-loading-stroke")).toHaveLength(8);
      expect(screen.queryByRole("button", {name: "Reload page"})).toBeNull();
    },
  );

  it("offers a same-page reload when the auth widget fails", () => {
    useClerk.mockReturnValue({status: "error"});
    const reloadPage = vi.spyOn(window.location, "reload").mockImplementation(() => {});
    render(<SignInPage />);
    expect(screen.getByRole("alert").textContent).toContain("Couldn’t load sign-in.");
    expect(screen.queryByRole("status")).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Reload page"}));
    expect(reloadPage).toHaveBeenCalledOnce();
  });
});

describe("AuthSessionKeeper", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    reload.mockReset().mockResolvedValue({});
    const id = "keeper-session";
    useSession.mockImplementation(() => ({ isLoaded: true, isSignedIn: true, session: { id, user: { id: USER_ID, reload } } }));
    useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: { id: USER_ID, reload } });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("test_dashboard_session_refresh_survives_long_journeys", async () => {
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
    render(<AuthSessionKeeper />);

    expect(reload).toHaveBeenCalledTimes(1);
    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(2);

    visibility.mockReturnValue("hidden");
    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(2);

    visibility.mockReturnValue("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(3);

    window.dispatchEvent(new Event("focus"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(3);

    window.dispatchEvent(new Event("online"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(3);
    await act(async () => vi.advanceTimersByTimeAsync(44_999));
    window.dispatchEvent(new Event("focus"));
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(reload).toHaveBeenCalledTimes(4);
  });

  it("waits for the SDK token expiry headroom and observes SDK rotation on focus", async () => {
    const now = Math.floor(Date.now() / 1000);
    vi.setSystemTime(now * 1000);
    const claims = { exp: now + 60 };
    useSession.mockReturnValue({ isSignedIn: true, session: { id: "fresh", user: { id: USER_ID, reload },
      lastActiveToken: { jwt: { claims } } } });
    render(<AuthSessionKeeper />);
    await act(async () => vi.advanceTimersByTimeAsync(44_000));
    expect(reload).not.toHaveBeenCalled();
    claims.exp += 40;
    window.dispatchEvent(new Event("focus"));
    await act(async () => vi.advanceTimersByTimeAsync(40_999));
    expect(reload).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(reload).toHaveBeenCalledOnce();
  });

  it("reschedules a shorter expiry for the same visible session without waiting for focus", async () => {
    const now = Math.floor(Date.now() / 1000);
    vi.setSystemTime(now * 1000);
    const session = { id: "rotating", user: { id: USER_ID, reload } };
    const rotate = (lifetime: number) => useSession.mockReturnValue({ isSignedIn: true,
      session: { ...session, lastActiveToken: { jwt: { claims: { exp: now + lifetime } } } } });
    rotate(60);
    const view = render(<AuthSessionKeeper />);
    await act(async () => Promise.resolve());
    rotate(20);
    view.rerender(<AuthSessionKeeper />);
    await act(async () => vi.advanceTimersByTimeAsync(4_999));
    expect(reload).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(reload).toHaveBeenCalledOnce();
  });

  it("keeps the newer expiry when a pending reload settles", async () => {
    const now = Math.floor(Date.now() / 1000);
    vi.setSystemTime(now * 1000);
    let settle = () => {};
    reload.mockImplementationOnce(() => new Promise<void>(resolve => { settle = resolve; }));
    const session = { id: "pending-rotation", user: { id: USER_ID, reload } };
    useSession.mockReturnValue({ isSignedIn: true, session });
    const view = render(<AuthSessionKeeper />);
    const claims = { exp: now + 20 };
    useSession.mockReturnValue({ isSignedIn: true, session: { ...session, lastActiveToken: { jwt: { claims } } } });
    view.rerender(<AuthSessionKeeper />);
    await act(async () => { settle(); });
    await act(async () => vi.advanceTimersByTimeAsync(4_999));
    expect(reload).toHaveBeenCalledOnce();
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("falls back to a proactive reload when the SDK stops reporting an expiry", async () => {
    const session = { id: "lost-expiry", user: { id: USER_ID, reload } };
    const claims = { exp: Math.floor(Date.now() / 1000) + 60 };
    useSession.mockReturnValue({ isSignedIn: true, session: { ...session, lastActiveToken: { jwt: { claims } } } });
    const view = render(<AuthSessionKeeper />);
    await act(async () => Promise.resolve());
    expect(reload).not.toHaveBeenCalled();
    useSession.mockReturnValue({ isSignedIn: true, session: { ...session, lastActiveToken: null } });
    view.rerender(<AuthSessionKeeper />);
    window.dispatchEvent(new Event("focus"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledOnce();
    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("retries after a slow failed reload even if focus returned before it settled", async () => {
    let reject = () => {};
    reload.mockImplementationOnce(() => new Promise<void>((_, fail) => { reject = () => fail(new Error("offline")); }));
    render(<AuthSessionKeeper />);
    await act(async () => vi.advanceTimersByTimeAsync(60_000));
    window.dispatchEvent(new Event("focus"));
    await act(async () => { reject(); });
    expect(reload).toHaveBeenCalledOnce();
    await act(async () => vi.advanceTimersByTimeAsync(15_000));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("discards a forced refresh callback after its session unmounts", async () => {
    const view = renderHook(useSessionRefresh);
    const { refresh } = view.result.current;
    view.unmount();
    await refresh();
    expect(reload).not.toHaveBeenCalled();
  });

  it("does not refresh before Clerk has a signed-in user", () => {
    useSession.mockReturnValue({ isLoaded: true, isSignedIn: false, session: null });
    render(<AuthSessionKeeper />);
    vi.advanceTimersByTime(90_000);
    window.dispatchEvent(new Event("focus"));
    expect(reload).not.toHaveBeenCalled();
  });

  it("projects the current account photo and clears identity after sign-out", () => {
    const imageUrl = "https://images.example.test/avatar.png";
    useUser.mockReturnValue({isLoaded: true, isSignedIn: true,
      user: {id: USER_ID, fullName: "Kishore", primaryEmailAddress: {emailAddress: "you@example.test"}, imageUrl}});
    const {result, rerender} = renderHook(useCurrentUser);
    expect(result.current).toMatchObject({displayName: "Kishore", emailAddress: "you@example.test", imageUrl});
    useUser.mockReturnValue({isLoaded: true, isSignedIn: false, user: null});
    rerender();
    expect(result.current).toMatchObject({userId: null, displayName: null, emailAddress: null, imageUrl: null});
  });

  it("test_keeper_single_refresh_despite_identity_churn", async () => {
    // Clerk re-instantiates the User resource after a reload, so every
    // render sees a fresh object with the same id. An effect keyed on the
    // object identity would fire its immediate refresh once per render —
    // this pins that only the 45s cadence adds calls.
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
    const view = render(<AuthSessionKeeper />);
    expect(reload).toHaveBeenCalledTimes(1);

    view.rerender(<AuthSessionKeeper />);
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(1);

    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("a forced onboarding refresh joins the current request and bypasses freshness afterward", async () => {
    let settle = () => {};
    const attempt = new Promise<void>(resolve => { settle = resolve; });
    reload.mockReturnValueOnce(attempt);
    const { result } = renderHook(useSessionRefresh, {
      wrapper: ({ children }) => <AuthProvider><AuthSessionKeeper />{children}</AuthProvider>,
    });
    let completed = false;
    const forced = result.current.refresh().then(() => { completed = true; });
    window.dispatchEvent(new Event("focus"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(1);
    expect(completed).toBe(false);
    await act(async () => { settle(); await forced; });
    expect(completed).toBe(true);
    await act(() => result.current.refresh());
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("retries a failed proactive refresh even when the page stays focused", async () => {
    reload.mockRejectedValueOnce(new Error("offline"));
    render(<AuthSessionKeeper />);
    await act(async () => vi.advanceTimersByTimeAsync(15_000));
    expect(reload).toHaveBeenCalledTimes(2);
    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(3);
  });

  it("a visibility or online signal recovers immediately after failure", async () => {
    reload.mockRejectedValueOnce(new Error("offline"));
    render(<AuthSessionKeeper />);
    await act(async () => Promise.resolve());
    window.dispatchEvent(new Event("online"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(2);
    await act(async () => vi.advanceTimersByTimeAsync(15_000));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it.each([["replacement_session", USER_ID], ["another_session", "another_user"]])(
    "isolates a switch to %s during a reload", async (id, userId) => {
      let settle = () => {};
      const previous = new Promise<void>(resolve => { settle = resolve; });
      reload.mockReturnValueOnce(previous);
      const view = render(<AuthSessionKeeper />);
      const next = vi.fn().mockResolvedValue({});
      useSession.mockReturnValue({ isLoaded: true, isSignedIn: true, session: { id, user: { id: userId, reload: next } } });
      view.rerender(<AuthSessionKeeper />);
      expect(next).toHaveBeenCalledTimes(1);
      await act(async () => { settle(); await previous; });
      window.dispatchEvent(new Event("focus"));
      await act(async () => vi.advanceTimersByTimeAsync(45_000));
      expect(next).toHaveBeenCalledTimes(2);
      expect(reload).toHaveBeenCalledTimes(1);
    });
});
