import React from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

const authUserButtonMock = vi.hoisted(() =>
  vi.fn((props: { appearance?: unknown }) =>
    React.createElement("div", {
      "data-has-appearance": String(Boolean(props.appearance)),
      "data-testid": "auth-user-button",
    }),
  ),
);

vi.mock("@/lib/auth/client", () => ({
  AuthUserButton: authUserButtonMock,
}));

vi.mock("@/lib/clerkAppearance", () => ({
  AUTH_APPEARANCE: { elements: { rootBox: "root" } },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("renders a stable placeholder before replacing it with the auth user button", async () => {
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
