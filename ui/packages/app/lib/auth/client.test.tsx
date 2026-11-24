import React from "react";
import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SignInPage from "@/app/(auth)/sign-in/[[...sign-in]]/page";
import SignUpPage from "@/app/(auth)/sign-up/[[...sign-up]]/page";
import { AuthSessionKeeper, useCurrentUser } from "./client";

const { reload, useClerk, useUser } = vi.hoisted(() => ({
  reload: vi.fn<() => Promise<unknown>>(),
  useClerk: vi.fn(),
  useUser: vi.fn(),
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => children,
  SignIn: ({fallback}: {fallback?: React.ReactNode}) => fallback,
  SignUp: ({fallback}: {fallback?: React.ReactNode}) => fallback,
  UserButton: () => null,
  useClerk,
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
    reload.mockResolvedValue({});
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
    expect(reload).toHaveBeenCalledTimes(4);

    window.dispatchEvent(new Event("online"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(5);
  });

  it("does not refresh before Clerk has a signed-in user", () => {
    useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
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
    useUser.mockImplementation(() => ({
      isLoaded: true,
      isSignedIn: true,
      user: { id: USER_ID, reload },
    }));
    const view = render(<AuthSessionKeeper />);
    expect(reload).toHaveBeenCalledTimes(1);

    view.rerender(<AuthSessionKeeper />);
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(1);

    await act(async () => vi.advanceTimersByTimeAsync(45_000));
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("coalesces overlapping refresh signals and retries after failure", async () => {
    let releaseRefresh: (() => void) | undefined;
    reload
      .mockImplementationOnce(() => new Promise<void>((resolve) => { releaseRefresh = resolve; }))
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue({});
    render(<AuthSessionKeeper />);

    window.dispatchEvent(new Event("focus"));
    expect(reload).toHaveBeenCalledTimes(1);
    releaseRefresh?.();
    await act(async () => Promise.resolve());

    window.dispatchEvent(new Event("focus"));
    await act(async () => Promise.resolve());
    window.dispatchEvent(new Event("focus"));
    await act(async () => Promise.resolve());
    expect(reload).toHaveBeenCalledTimes(3);
  });
});
