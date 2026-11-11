import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// What the shell adds for teams: the Members entry and the invite notice. Only
// the identity provider's widgets, analytics, and Next's routing are stood in
// for; the shell, its sidebar, and the notice render for real.
const { usePathname } = vi.hoisted(() => ({ usePathname: vi.fn<() => string>() }));
vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: vi.fn(), trackNavigationClicked: vi.fn(), setAnalyticsContext: vi.fn(), captureProductEvent: vi.fn(),
}));
vi.mock("@clerk/nextjs", () => ({
  SignIn: () => null, SignUp: () => null,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => children,
  SignOutButton: ({ children }: { children: React.ReactNode }) => children, UserProfile: () => null,
  UserButton: () => React.createElement("div"),
  useUser: () => ({ user: null }),
  useAuth: () => ({ getToken: async () => "token_stub" }),
}));
vi.mock("next/navigation", () => ({ useSearchParams: () => new URLSearchParams(), usePathname, useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }) }));
vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

import { ShellFrame } from "@/components/layout/ShellFrame";


const NOTICE = 'data-testid="invite-notice"';
const WAITING = [{ id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: "1970-01-01T00:00:00.001Z" }];

function shell(props: Omit<React.ComponentProps<typeof ShellFrame>, "children"> = {}): string {
  return renderToStaticMarkup(<ShellFrame {...props}><div /></ShellFrame>);
}

afterEach(() => {cleanup(); vi.clearAllMocks();});

describe("shell for teams", () => {
  it("keeps the served credit balance beside Usage in the account menu", async () => {
    usePathname.mockReturnValue("/w/ws_1/fleets");
    render(<ShellFrame billing={{
      balance_nanos: "3680000000", updated_at: "2026-10-19T00:00:00Z",
      is_exhausted: false, exhausted_at: null,
    }}><p>Current work</p></ShellFrame>);
    await userEvent.setup().click(screen.getByRole("button", {name: "Workspace and account menu"}));
    const usage = await screen.findByRole("menuitem", {name: /Usage/});
    expect(usage.getAttribute("href")).toBe("/settings/billing");
    expect(usage.textContent).toContain("$3.68");
  });

  it("should show the invite notice above the page only while an invite waits", () => {
    usePathname.mockReturnValue("/w/ws_1/fleets");
    expect(shell({ waitingInvites: WAITING })).toContain(NOTICE);
    expect(shell()).not.toContain(NOTICE);
  });
});
