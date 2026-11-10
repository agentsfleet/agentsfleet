import React from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// M245 §10: the screen a new account waits on mints a fresh session token and
// re-runs the entry page each tick, until the entry page redirects it away or
// it gives up and says so.

const { refreshSession, routerRefresh } = vi.hoisted(() => ({
  refreshSession: vi.fn<() => Promise<void>>(),
  routerRefresh: vi.fn(),
}));

vi.mock("@/lib/auth/client", () => ({
  useSessionRefresh: () => ({ signedInUserId: "user_new", refresh: refreshSession }),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh }),
}));

import AccountSetup, {
  ACCOUNT_SETUP_GIVE_UP_MS,
  ACCOUNT_SETUP_POLL_MS,
} from "@/app/(dashboard)/AccountSetup";

/** Lets the awaited token refresh settle before the router refresh is read. */
async function tick(ms: number): Promise<void> {
  await act(async () => vi.advanceTimersByTimeAsync(ms));
}

describe("AccountSetup", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    refreshSession.mockResolvedValue(undefined);
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
    expect(refreshSession).toHaveBeenCalledTimes(1);
    expect(routerRefresh).toHaveBeenCalledTimes(1);

    await tick(ACCOUNT_SETUP_POLL_MS);
    expect(refreshSession).toHaveBeenCalledTimes(2);
    expect(routerRefresh).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId("account-setup-retry")).toBeNull();
  });

  it("re-reads only after the fresh token: the entry page must see the new cookie", async () => {
    let settle: () => void = () => undefined;
    refreshSession.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    render(<AccountSetup />);
    await tick(0);
    expect(refreshSession).toHaveBeenCalledTimes(1);
    expect(routerRefresh).not.toHaveBeenCalled();

    await act(async () => settle());
    expect(routerRefresh).toHaveBeenCalledTimes(1);
  });

  it("test_account_setup_gives_up_and_can_try_again", async () => {
    render(<AccountSetup />);
    await tick(ACCOUNT_SETUP_GIVE_UP_MS);
    const calls = refreshSession.mock.calls.length;
    expect(screen.getByTestId("account-setup-retry")).toBeTruthy();

    // Stalled means stopped: no more tokens are minted until the person asks.
    await tick(ACCOUNT_SETUP_POLL_MS * 5);
    expect(refreshSession.mock.calls.length).toBe(calls);

    fireEvent.click(screen.getByTestId("account-setup-retry"));
    await tick(0);
    expect(refreshSession.mock.calls.length).toBe(calls + 1);
    expect(screen.queryByTestId("account-setup-retry")).toBeNull();
  });

  it("does not re-read for a token that lands after the screen has gone", async () => {
    let settle: () => void = () => undefined;
    refreshSession.mockImplementationOnce(
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
    expect(refreshSession).toHaveBeenCalledTimes(1);
  });
});
