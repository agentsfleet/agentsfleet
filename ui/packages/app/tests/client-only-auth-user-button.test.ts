import React from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { avatarColor, AVATAR_COLOR_FALLBACK_SEED } from "../lib/avatarColor";

const authUserButtonMock = vi.hoisted(() =>
  vi.fn((props: { appearance?: { elements?: { userButtonAvatarBox?: { background?: string } } } }) =>
    React.createElement("div", {
      "data-has-appearance": String(Boolean(props.appearance)),
      "data-avatar-background": props.appearance?.elements?.userButtonAvatarBox?.background ?? "",
      "data-testid": "auth-user-button",
    }),
  ),
);

const useCurrentUserMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/auth/client", () => ({
  AuthUserButton: authUserButtonMock,
  useCurrentUser: useCurrentUserMock,
}));

vi.mock("@/lib/clerkAppearance", () => ({
  AUTH_APPEARANCE: { elements: { rootBox: "root", userButtonAvatarBox: { backgroundColor: "var(--surface-2)" } } },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("renders a stable placeholder before replacing it with the auth user button", async () => {
  useCurrentUserMock.mockReturnValue({
    isLoaded: true,
    isSignedIn: true,
    userId: "user_1",
    emailAddress: "a@b.com",
  });
  const { default: ClientOnlyAuthUserButton } = await import(
    "../components/layout/ClientOnlyAuthUserButton"
  );

  render(React.createElement(ClientOnlyAuthUserButton));

  await waitFor(() => expect(screen.getByTestId("auth-user-button")).toBeTruthy());
  expect(authUserButtonMock).toHaveBeenCalledWith(
    expect.objectContaining({ appearance: expect.any(Object) }),
    undefined,
  );
});

// The per-user avatar color.
describe("avatar color wiring", () => {
  it("passes a flat background derived from the current user's id instead of the default surface", async () => {
    useCurrentUserMock.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      userId: "user_1",
      emailAddress: "a@b.com",
    });
    const { default: ClientOnlyAuthUserButton } = await import(
      "../components/layout/ClientOnlyAuthUserButton"
    );
    render(React.createElement(ClientOnlyAuthUserButton));
    const el = await screen.findByTestId("auth-user-button");
    expect(el.getAttribute("data-avatar-background")).toBe(avatarColor("user_1"));
    expect(el.getAttribute("data-avatar-background")).not.toBe("var(--surface-2)");
  });

  it("falls back to a stable non-empty seed when no user id is available yet", async () => {
    useCurrentUserMock.mockReturnValue({
      isLoaded: false,
      isSignedIn: false,
      userId: null,
      emailAddress: null,
    });
    const { default: ClientOnlyAuthUserButton } = await import(
      "../components/layout/ClientOnlyAuthUserButton"
    );
    render(React.createElement(ClientOnlyAuthUserButton));
    const el = await screen.findByTestId("auth-user-button");
    expect(el.getAttribute("data-avatar-background")).toBe(avatarColor(AVATAR_COLOR_FALLBACK_SEED));
  });
});
