import type { AccountAccess } from "@/lib/auth/account-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import AccountPage from "@/app/(dashboard)/settings/account/[[...account]]/page";
import { AccountMethodsSettings } from "@/components/account/AccountMethodsSettings";

const access = vi.hoisted(() => ({current: {status: "loading"} as AccountAccess}));
vi.mock("@/lib/auth/account-client", () => ({useAccountSettings: () => access.current}));
// The official widget owns verified credential operations; no app credential handler is substituted.
vi.mock("@clerk/nextjs", () => ({UserProfile: ({routing, appearance}: {routing: string; appearance: unknown}) =>
  <div data-routing={routing} data-appearance={JSON.stringify(appearance)}><input aria-label="Native account draft" /></div>}));
afterEach(cleanup);

it("keeps a named page and sign-in recovery while native controls are unavailable", () => {
  access.current = {status: "loading"};
  const view = render(<AccountMethodsSettings />);
  expect(screen.getByRole("heading", {name: "Sign-in methods"})).toBeTruthy();
  expect(screen.getByRole("region", {name: "Secure account controls"})).toBeTruthy();
  expect(screen.queryByLabelText("Native account draft")).toBeNull();
  access.current = {status: "signed-out"};
  view.rerender(<AccountMethodsSettings />);
  expect(screen.getByRole("link", {name: "Sign in"}).getAttribute("href")).toBe("/sign-in");
});

it("hosts native hash routing and clears its live widget across account or session changes", async () => {
  access.current = {status: "ready", contextKey: "owner:one", profile: {subject: "owner", firstName: "", lastName: "", email: null, imageUrl: null},
    client: {updateProfile: vi.fn(), listSessions: vi.fn(), revokeSession: vi.fn(), manageSignIn: vi.fn()}};
  const view = render(await AccountPage({params: Promise.resolve({account: ["sign-in"]})}));
  const draft = screen.getByLabelText("Native account draft");
  expect(draft.parentElement?.getAttribute("data-routing")).toBe("hash");
  fireEvent.change(draft, {target: {value: "Unsubmitted verification draft"}});
  view.rerender(<AccountMethodsSettings />);
  expect(screen.getByLabelText("Native account draft")).toHaveProperty("value", "Unsubmitted verification draft");
  access.current = {...access.current, contextKey: "owner:two"};
  view.rerender(<AccountMethodsSettings />);
  expect(screen.getByLabelText("Native account draft")).toHaveProperty("value", "");
  expect(screen.getByRole("link", {name: "Back to Security"}).getAttribute("href")).toBe("/settings/security");
});
