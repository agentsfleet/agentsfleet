import type { AccountAccess, AccountSession, AccountSettingsClient } from "@/lib/auth/account-types";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { SessionSettings, SessionSettingsView } from "@/components/account/SessionSettings";

const access = vi.hoisted(() => ({current: {status: "loading"} as AccountAccess}));
vi.mock("@/lib/auth/account-client", () => ({useAccountSettings: () => access.current}));
vi.mock("@/lib/auth/client", () => ({AuthSignOutButton: ({children}: {children: React.ReactNode}) => children}));

const CURRENT: AccountSession = {id: "current", browser: "Chrome", device: "macOS", mobile: false, current: true, location: null, lastActiveAt: null};
const OTHER: AccountSession = {...CURRENT, id: "other", browser: "Safari", device: "iPhone", mobile: true, current: false};
afterEach(cleanup);

function client(): AccountSettingsClient {
  return {listSessions: vi.fn().mockResolvedValue({ok: true, data: {sessions: [CURRENT, OTHER], nextOffset: null}}),
    revokeSession: vi.fn().mockResolvedValue({ok: true, data: {id: OTHER.id}})};
}







it("shows own browser metadata, unknown activity and separate automation credentials", async () => {
  const api = client();
  render(<TooltipProvider><SessionSettingsView client={api} /></TooltipProvider>);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
  expect(screen.getByText("This device")).toBeTruthy();
  expect(screen.getAllByText("Last active unavailable")).toHaveLength(2);
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
  expect(screen.getByText("The session list could not be refreshed. Try refreshing it again.")).toBeTruthy();
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
  const view = render(<SessionSettings />);
  expect(screen.getByRole("status")).toBeTruthy();
  access.current = {status: "signed-out"}; view.rerender(<SessionSettings />);
  expect(screen.getByRole("link", {name: "Sign in"}).getAttribute("href")).toBe("/sign-in");
  access.current = {status: "ready", contextKey: "owner", client: api}; view.rerender(<SessionSettings />);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
  access.current = {status: "signed-out"}; view.rerender(<SessionSettings />);
  expect(screen.getByRole("link", {name: "Sign in"})).toBeTruthy();
  access.current = {status: "ready", contextKey: "owner", client: api}; view.rerender(<SessionSettings />);
  expect(await screen.findByText("Chrome on macOS")).toBeTruthy();
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
  expect(screen.getByRole("button", {name: "Sign out other sessions"})).toBeTruthy();
});
