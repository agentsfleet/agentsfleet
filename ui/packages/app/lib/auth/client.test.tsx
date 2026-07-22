import React from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { reload, useUser } = vi.hoisted(() => ({
  reload: vi.fn<() => Promise<unknown>>(),
  useUser: vi.fn(),
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => children,
  SignIn: () => null,
  SignUp: () => null,
  UserButton: () => null,
  useUser,
}));

import { AuthSessionKeeper } from "./client";

describe("AuthSessionKeeper", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    reload.mockResolvedValue({});
    useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: { reload } });
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
