import React from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The screen a new account waits on mints a fresh session token and re-runs the
// entry page each tick, until the entry page redirects it away or it gives up
// and says so. Clerk is faked at the SDK boundary: `user.reload()` is what
// mints the token, so the real `useSessionRefresh` runs under the screen.

const { reload, routerRefresh, useUser } = vi.hoisted(() => ({
  reload: vi.fn<() => Promise<unknown>>(),
  routerRefresh: vi.fn(),
  useUser: vi.fn(),
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => children,
  SignIn: () => null,
  SignUp: () => null,
  useUser,
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh }),
}));

import {
  ACCOUNT_SETUP_GIVE_UP_MS,
  ACCOUNT_SETUP_POLL_MS,
  AccountSetup,
} from "@/app/(dashboard)/AccountSetup";

const PAGE_TITLE = "Setting up your workspace";
const WAITING_TITLE = "Creating your account";
const STALLED_TITLE = "This is taking longer than usual";

/** Lets the awaited token refresh settle before the router refresh is read. */
async function tick(ms: number): Promise<void> {
  await act(async () => vi.advanceTimersByTimeAsync(ms));
}

describe("AccountSetup", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    reload.mockResolvedValue({});
    useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: { id: "user_new", reload } });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("test_account_setup_mints_a_token_then_rereads_each_tick", async () => {
    render(<AccountSetup />);
    await tick(0);
    // At once, not after the first interval: a new account's webhook usually
    // lands within seconds of the redirect.
    expect(reload).toHaveBeenCalledTimes(1);
    expect(routerRefresh).toHaveBeenCalledTimes(1);

    await tick(ACCOUNT_SETUP_POLL_MS);
    expect(reload).toHaveBeenCalledTimes(2);
    expect(routerRefresh).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId("account-setup-retry")).toBeNull();
  });

  it("re-reads only after the fresh token: the entry page must see the new cookie", async () => {
    let settle: () => void = () => undefined;
    reload.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    render(<AccountSetup />);
    await tick(0);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(routerRefresh).not.toHaveBeenCalled();

    await act(async () => settle());
    expect(routerRefresh).toHaveBeenCalledTimes(1);
  });

  it("keeps trying when a token mint fails: the next tick mints again", async () => {
    reload.mockRejectedValueOnce(new Error("offline"));
    render(<AccountSetup />);
    await tick(0);
    expect(reload).toHaveBeenCalledTimes(1);
    // A transient Clerk failure is not the end of the wait.
    expect(screen.queryByTestId("account-setup-retry")).toBeNull();

    await tick(ACCOUNT_SETUP_POLL_MS);
    expect(reload).toHaveBeenCalledTimes(2);
    expect(routerRefresh).toHaveBeenCalledTimes(2);
  });

  it("test_account_setup_gives_up_and_can_try_again", async () => {
    render(<AccountSetup />);
    await tick(ACCOUNT_SETUP_GIVE_UP_MS);
    const calls = reload.mock.calls.length;
    expect(screen.getByTestId("account-setup-retry")).toBeTruthy();

    // Stalled means stopped: no more tokens are minted until the person asks.
    await tick(ACCOUNT_SETUP_POLL_MS * 5);
    expect(reload.mock.calls.length).toBe(calls);

    fireEvent.click(screen.getByTestId("account-setup-retry"));
    await tick(0);
    expect(reload.mock.calls.length).toBe(calls + 1);
    expect(screen.queryByTestId("account-setup-retry")).toBeNull();
  });

  it("does not re-read for a token that lands after the screen has gone", async () => {
    let settle: () => void = () => undefined;
    reload.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    const { unmount } = render(<AccountSetup />);
    await tick(0);
    unmount();
    await act(async () => settle());
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("stops minting once the entry page redirects it away", async () => {
    const { unmount } = render(<AccountSetup />);
    await tick(0);
    unmount();
    await tick(ACCOUNT_SETUP_POLL_MS * 5);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("titles the page once and states what is happening in its own heading", async () => {
    render(<AccountSetup />);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(PAGE_TITLE);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe(WAITING_TITLE);

    await tick(ACCOUNT_SETUP_GIVE_UP_MS);
    // The state changes; the page's title does not, and never repeats beneath it.
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(PAGE_TITLE);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe(STALLED_TITLE);
  });

  it("uses the shared decorative loader only while waiting", async () => {
    const {container} = render(<AccountSetup />);
    expect(container.querySelector("[data-signal-spinner]")?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getAllByRole("status")).toHaveLength(1);

    await tick(ACCOUNT_SETUP_GIVE_UP_MS);
    expect(container.querySelector("[data-signal-spinner]")).toBeNull();
  });
});
