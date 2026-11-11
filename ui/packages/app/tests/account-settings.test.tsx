import type { AccountAccess, AccountProfile, AccountSession, AccountSettingsClient } from "@/lib/auth/account-types";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ProfileSettings, ProfileSettingsView } from "@/components/account/ProfileSettings";
import { SessionSettings, SessionSettingsView } from "@/components/account/SessionSettings";
import AccountPage from "@/app/(dashboard)/settings/account/[[...account]]/page";
import SecurityPage from "@/app/(dashboard)/settings/security/page";

const access = vi.hoisted(() => ({current: {status: "loading"} as AccountAccess}));
vi.mock("@/lib/auth/account-client", () => ({useAccountSettings: () => access.current}));
vi.mock("@/lib/auth/client", () => ({AuthSignOutButton: ({children}: {children: React.ReactNode}) => children}));
const PROFILE: AccountProfile = {subject: "owner", firstName: "Kishore", lastName: "", email: "you@example.test", imageUrl: null};
const CURRENT: AccountSession = {id: "current", browser: "Chrome", device: "macOS", mobile: false, current: true, location: null, lastActiveAt: null};
const OTHER: AccountSession = {...CURRENT, id: "other", browser: "Safari", device: "iPhone", mobile: true, current: false};
afterEach(cleanup);

function client(): AccountSettingsClient {
  return {updateProfile: vi.fn().mockResolvedValue({ok: true, data: null}),
    listSessions: vi.fn().mockResolvedValue({ok: true, data: {sessions: [CURRENT, OTHER], nextOffset: null}}),
    revokeSession: vi.fn().mockResolvedValue({ok: true, data: {id: OTHER.id}}), manageSignIn: vi.fn()};
}

it("keeps edited names after a failed profile save and only announces confirmed success", async () => {
  const api = client();
  vi.mocked(api.updateProfile).mockResolvedValueOnce({ok: false, error: "Profile unavailable"});
  const user = userEvent.setup();
  render(<ProfileSettingsView profile={PROFILE} client={api} />);
  const firstName = screen.getByRole("textbox", {name: "First name"});
  await user.clear(firstName); await user.type(firstName, "Indy");
  await user.click(screen.getByRole("button", {name: "Save changes"}));
  expect(await screen.findByText("Profile unavailable")).toBeTruthy();
  expect(firstName).toHaveProperty("value", "Indy");
  expect(screen.queryByText("Profile saved")).toBeNull();
  await user.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(api.updateProfile).toHaveBeenCalledTimes(2));
  expect(api.updateProfile).toHaveBeenLastCalledWith({firstName: "Indy", lastName: ""});
  expect(await screen.findByText("Profile saved")).toBeTruthy();
  await user.type(firstName, " changed");
  expect(screen.queryByText("Profile saved")).toBeNull();
  expect(api.updateProfile).toHaveBeenCalledTimes(2);
});

it("excludes duplicate profile submissions before pending renders and retains secure account controls", async () => {
  const api = client();
  const pending = Promise.withResolvers<{ok: true; data: null}>();
  vi.mocked(api.updateProfile).mockReturnValue(pending.promise);
  render(<ProfileSettingsView profile={PROFILE} client={api} />);
  const form = screen.getByRole("form", {name: "Edit personal details"});
  fireEvent.submit(form); fireEvent.submit(form);
  expect(api.updateProfile).toHaveBeenCalledTimes(1);
  await act(async () => pending.resolve({ok: true, data: null}));
  fireEvent.click(screen.getByRole("button", {name: "Manage account"}));
  expect(api.manageSignIn).toHaveBeenCalledOnce();
});

it("adopts native account changes in pristine fields while retaining an edited draft", async () => {
  const api = client();
  const user = userEvent.setup();
  const view = render(<ProfileSettingsView profile={PROFILE} client={api} />);
  view.rerender(<ProfileSettingsView profile={{...PROFILE, firstName: "Indy"}} client={api} />);
  const first = screen.getByRole("textbox", {name: "First name"});
  expect(first).toHaveProperty("value", "Indy");
  await user.type(first, " editing");
  view.rerender(<ProfileSettingsView profile={{...PROFILE, firstName: "Updated elsewhere", lastName: "New family name"}} client={api} />);
  expect(first).toHaveProperty("value", "Indy editing");
  expect(screen.getByRole("textbox", {name: "Last name"})).toHaveProperty("value", "New family name");
});

it("shows own browser metadata, unknown activity and separate automation credentials", async () => {
  const api = client();
  render(<TooltipProvider><SessionSettingsView client={api} /></TooltipProvider>);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
  expect(screen.getByText("This device")).toBeTruthy();
  expect(screen.getAllByText("Last active unavailable")).toHaveLength(2);
  expect(screen.getByRole("link", {name: "API keys"}).getAttribute("href")).toBe("/settings/api-keys");
  expect(api.revokeSession).not.toHaveBeenCalled();
});

it("confirms a remote sign-out and retains that success when refresh fails", async () => {
  const api = client();
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {sessions: [CURRENT, OTHER], nextOffset: null}})
    .mockResolvedValueOnce({ok: false, error: "Could not refresh sessions"});
  const user = userEvent.setup();
  render(<TooltipProvider><SessionSettingsView client={api} /></TooltipProvider>);
  await user.click(await screen.findByRole("button", {name: "Sign out Safari on iPhone"}));
  expect(api.revokeSession).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", {name: "Sign out session"}));
  expect(await screen.findByText("Session signed out")).toBeTruthy();
  expect(screen.queryByText("Safari on iPhone")).toBeNull();
  expect(screen.getByText("Could not refresh sessions")).toBeTruthy();
  expect(api.revokeSession).toHaveBeenCalledExactlyOnceWith("other");
});

it("keeps a failed remote session visible and offers a fresh read", async () => {
  const api = client();
  vi.mocked(api.revokeSession).mockResolvedValue({ok: false, error: "Sign-out unconfirmed"});
  const user = userEvent.setup();
  render(<TooltipProvider><SessionSettingsView client={api} /></TooltipProvider>);
  await user.click(await screen.findByRole("button", {name: "Sign out Safari on iPhone"}));
  await user.click(screen.getByRole("button", {name: "Sign out session"}));
  expect(await screen.findByRole("alertdialog")).toBeTruthy();
  expect(screen.getAllByText("Sign-out unconfirmed").length).toBeGreaterThan(0);
  await user.click(screen.getByRole("button", {name: "Cancel"}));
  expect(screen.getByText("Safari on iPhone")).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Refresh sessions"}));
  await waitFor(() => expect(api.listSessions).toHaveBeenCalledTimes(2));
});

it("discards late session reads after the signed-in owner is replaced", async () => {
  const first = client(), next = client();
  const pending = Promise.withResolvers<Awaited<ReturnType<AccountSettingsClient["listSessions"]>>>();
  vi.mocked(first.listSessions).mockReturnValue(pending.promise);
  vi.mocked(next.listSessions).mockResolvedValue({ok: true, data: {sessions: [], nextOffset: null}});
  const view = render(<TooltipProvider><SessionSettingsView key="first" client={first} /></TooltipProvider>);
  view.rerender(<TooltipProvider><SessionSettingsView key="next" client={next} /></TooltipProvider>);
  await act(async () => pending.resolve({ok: true, data: {sessions: [OTHER], nextOffset: null}}));
  expect(await screen.findByText("No active browser sessions were returned")).toBeTruthy();
  expect(screen.queryByText("Safari on iPhone")).toBeNull();
});

it("renders account routes through loading, sign-in and the app-owned ready state", async () => {
  const api = client();
  access.current = {status: "loading"};
  const view = render(<ProfileSettings />);
  expect(screen.getByRole("status")).toBeTruthy();
  access.current = {status: "signed-out"}; view.rerender(<ProfileSettings />);
  expect(screen.getByRole("link", {name: "Sign in"}).getAttribute("href")).toBe("/sign-in");
  access.current = {status: "ready", contextKey: "owner", profile: PROFILE, client: api}; view.rerender(<ProfileSettings />);
  expect(screen.getByRole("heading", {name: "Profile"})).toBeTruthy();
  access.current = {status: "signed-out"}; view.rerender(<SessionSettings />);
  expect(screen.getByRole("link", {name: "Sign in"})).toBeTruthy();
  access.current = {status: "ready", contextKey: "owner", profile: PROFILE, client: api}; view.rerender(<SessionSettings />);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
});

it("keeps absent profile fields unknown and shows the current avatar without a second credential", () => {
  const api = client();
  const view = render(<ProfileSettingsView profile={{...PROFILE, firstName: "", email: null}} client={api} />);
  expect(screen.getByText("Your profile")).toBeTruthy();
  expect(screen.getByText("No primary email address")).toBeTruthy();
  view.rerender(<ProfileSettingsView profile={{...PROFILE, firstName: ""}} client={api} />);
  expect(screen.getByText("Y")).toBeTruthy();
  view.rerender(<ProfileSettingsView profile={{...PROFILE, firstName: "", imageUrl: "https://example.test/avatar.png"}} client={api} />);
  expect(view.container.querySelector("img")?.getAttribute("src")).toBe("https://example.test/avatar.png");
});

it("loads the custom Profile and Security routes without the former embedded profile router", async () => {
  access.current = {status: "ready", contextKey: "owner", profile: PROFILE, client: client()};
  const view = render(await AccountPage({params: Promise.resolve({})}));
  expect(screen.getByRole("heading", {name: "Profile"})).toBeTruthy();
  view.rerender(await AccountPage({params: Promise.resolve({account: []})}));
  expect(screen.getByRole("form", {name: "Edit personal details"})).toBeTruthy();
  view.rerender(<TooltipProvider><SecurityPage /></TooltipProvider>);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
});

it("refuses a form missing a name field before calling the profile provider", async () => {
  const api = client(); const user = userEvent.setup();
  render(<ProfileSettingsView profile={PROFILE} client={api} />);
  screen.getByRole("textbox", {name: "First name"}).removeAttribute("name");
  await user.click(screen.getByRole("button", {name: "Save changes"}));
  expect(await screen.findByText("Enter a first and last name.")).toBeTruthy();
  expect(api.updateProfile).not.toHaveBeenCalled();
});

it("retains family-name edits across provider updates and hides saved feedback after the next edit", async () => {
  const api = client(); const user = userEvent.setup();
  const view = render(<ProfileSettingsView profile={PROFILE} client={api} />);
  const last = screen.getByRole("textbox", {name: "Last name"});
  await user.type(last, "Edited");
  view.rerender(<ProfileSettingsView profile={{...PROFILE, lastName: "External"}} client={api} />);
  expect(last).toHaveProperty("value", "Edited");
  await user.click(screen.getByRole("button", {name: "Save changes"}));
  expect(await screen.findByText("Profile saved")).toBeTruthy();
  await user.type(last, " again");
  expect(screen.queryByText("Profile saved")).toBeNull();
  vi.mocked(api.updateProfile).mockRejectedValueOnce(new Error("Connection lost"));
  await user.click(screen.getByRole("button", {name: "Save changes"}));
  expect(await screen.findByText(/Your profile change could not be confirmed/)).toBeTruthy();
  expect(last).toHaveProperty("value", "Edited again");
});

it("shows paginated sessions with actual activity and keeps secure sign-in controls reachable", async () => {
  const api = client();
  vi.mocked(api.listSessions).mockResolvedValueOnce({ok: true, data: {sessions: [{...CURRENT, location: "Chennai, IN", lastActiveAt: "2026-11-11T10:00:00Z"}], nextOffset: 1}})
    .mockResolvedValueOnce({ok: true, data: {sessions: [OTHER], nextOffset: null}});
  const user = userEvent.setup();
  render(<TooltipProvider><SessionSettingsView client={api} /></TooltipProvider>);
  await user.click(await screen.findByRole("button", {name: "Load more sessions"}));
  expect(await screen.findByText("Safari on iPhone")).toBeTruthy();
  expect(screen.getByText("Chrome on macOS")).toBeTruthy();
  expect(screen.getByText(/Chennai, IN/)).toBeTruthy();
  expect(api.listSessions).toHaveBeenLastCalledWith(1);
  await user.click(screen.getByRole("button", {name: "Sign-in methods"}));
  expect(api.manageSignIn).toHaveBeenCalledOnce();
});
