import type { AccountAccess } from "@/lib/auth/account-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AccountSettingsSection } from "@/components/account/AccountSettingsSection";
import { AccountSettingsAction } from "@/components/account/AccountSettingsAction";

const access = vi.hoisted(() => ({current: {status: "loading"} as AccountAccess}));
vi.mock("@/lib/auth/account-client", () => ({useAccountSettings: () => access.current}));
vi.mock("@clerk/nextjs", () => ({UserProfile: ({routing}: {routing: string}) => <div data-routing={routing}><input aria-label="Native account draft" /></div>}));
afterEach(cleanup);

it("keeps the section named across loading and signed-out recovery", () => {
  access.current = {status: "loading"};
  const view = render(<AccountSettingsSection />);
  expect(screen.getByRole("region", {name: "Profile and sign-in settings"})).toBeTruthy();
  expect(screen.queryByLabelText("Native account draft")).toBeNull();
  access.current = {status: "signed-out"};
  view.rerender(<AccountSettingsSection />);
  expect(screen.getByRole("link", {name: "Sign in"}).getAttribute("href")).toBe("/sign-in");
});

it("keeps native hash routing mounted and clears drafts on account or session change", () => {
  access.current = {status: "ready", contextKey: "owner:one", client: {listSessions: vi.fn(), revokeSession: vi.fn()}};
  const view = render(<AccountSettingsSection />);
  const draft = screen.getByLabelText("Native account draft");
  expect(draft.parentElement?.getAttribute("data-routing")).toBe("hash");
  fireEvent.change(draft, {target: {value: "Unsubmitted verification draft"}});
  view.rerender(<AccountSettingsSection />);
  expect(screen.getByLabelText("Native account draft")).toHaveProperty("value", "Unsubmitted verification draft");
  access.current = {...access.current, contextKey: "owner:two"};
  view.rerender(<AccountSettingsSection />);
  expect(screen.getByLabelText("Native account draft")).toHaveProperty("value", "");
});

it("scrolls and focuses the section without changing the provider's verification hash", () => {
  history.replaceState({}, "", "#/verify");
  access.current = {status: "loading"};
  const view = render(<><AccountSettingsAction /><AccountSettingsSection /></>);
  const section = screen.getByRole("region", {name: "Profile and sign-in settings"});
  const scroll = vi.fn();
  section.scrollIntoView = scroll;
  fireEvent.click(screen.getByRole("button", {name: "Profile and sign-in settings"}));
  expect(document.activeElement).toBe(section);
  expect(scroll).toHaveBeenCalledWith({block: "start"});
  expect(location.hash).toBe("#/verify");
  view.rerender(<AccountSettingsAction />);
  fireEvent.click(screen.getByRole("button", {name: "Profile and sign-in settings"}));
  expect(location.hash).toBe("#/verify");
  history.replaceState({}, "", "/");
});
