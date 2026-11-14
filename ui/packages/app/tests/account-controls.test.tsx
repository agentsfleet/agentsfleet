import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AccountSettingsSection } from "@/components/account/AccountSettingsSection";

type Identity = ReturnType<typeof import("@/lib/auth/client").useCurrentUser>;
const identity = vi.hoisted((): {current: Identity} => ({current: {
  isLoaded: false, isSignedIn: false, userId: null, displayName: null, emailAddress: null, imageUrl: null,
}}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity.current}));
afterEach(cleanup);

it("keeps identity loading and signed-out recovery explicit", () => {
  identity.current = {...identity.current, isLoaded: false, isSignedIn: false};
  const view = render(<AccountSettingsSection />);
  expect(screen.getByRole("region", {name: "Your account"})).toBeTruthy();
  expect(screen.queryByText("Signed in as")).toBeNull();
  identity.current = {...identity.current, isLoaded: true};
  view.rerender(<AccountSettingsSection />);
  expect(screen.getByRole("link", {name: "Sign in"}).getAttribute("href")).toBe("/sign-in");
});

it("shows current identity without duplicate profile controls and replaces it when the person changes", () => {
  identity.current = {isLoaded: true, isSignedIn: true, userId: "one", displayName: "Kishore", emailAddress: "you@example.test", imageUrl: "https://example.test/avatar.png"};
  const view = render(<AccountSettingsSection />);
  expect(screen.getByRole("heading", {name: "Kishore"})).toBeTruthy();
  expect(screen.getByText("you@example.test")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Profile and sign-in settings"})).toBeNull();
  identity.current = {...identity.current, userId: "two", displayName: null, emailAddress: null, imageUrl: null};
  view.rerender(<AccountSettingsSection />);
  expect(screen.queryByText("Kishore")).toBeNull();
  expect(screen.queryByText("you@example.test")).toBeNull();
  expect(screen.getByRole("heading", {name: "Your account"})).toBeTruthy();
  expect(screen.getByText("No primary email address")).toBeTruthy();
});
