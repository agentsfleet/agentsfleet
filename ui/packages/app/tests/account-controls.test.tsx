import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AccountAvatar } from "@/components/account/AccountAvatar";
import { WorkspaceSwitcherTrigger } from "@/components/layout/WorkspaceSwitcherTrigger";

type Identity = ReturnType<typeof import("@/lib/auth/client").useCurrentUser>;
const identity = vi.hoisted((): {current: Identity} => ({current: {
  isLoaded: false, isSignedIn: false, userId: null, displayName: null, emailAddress: null, imageUrl: null,
}}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity.current}));
afterEach(cleanup);

it("keeps initials and a generic avatar available when no image can be shown", () => {
  const view = render(<AccountAvatar userId="one" name="Kishore Kumar" imageUrl={null} />);
  expect(screen.getByText("KK")).toBeTruthy();
  view.rerender(<AccountAvatar userId={null} name={null} imageUrl={null} />);
  expect(screen.queryByText("KK")).toBeNull();
  expect(view.container.querySelector("svg")).toBeTruthy();
  view.rerender(<AccountAvatar userId={null} name={null} imageUrl={null} busy />);
  expect(screen.getByRole("status").textContent).toContain("Loading workspace menu");
});

it("shows the current email on avatar hover and replaces the identity when the person changes", async () => {
  identity.current = {isLoaded: true, isSignedIn: true, userId: "one", displayName: "Kishore", emailAddress: "you@example.test", imageUrl: null};
  const user = userEvent.setup();
  const view = render(<WorkspaceSwitcherTrigger activeLabel="Workspace" collapsed />);
  await user.hover(screen.getByRole("button", {name: /you@example.test/}));
  expect((await screen.findByRole("tooltip")).textContent).toBe("you@example.test");
  identity.current = {...identity.current, userId: "two", displayName: "Another person", emailAddress: "other@example.test"};
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="Workspace" collapsed />);
  expect(screen.queryByRole("button", {name: /you@example.test/})).toBeNull();
  expect(screen.getByRole("button", {name: /other@example.test/})).toBeTruthy();
  expect(screen.queryByText("K")).toBeNull();
  expect(screen.getByText("AP")).toBeTruthy();
});
