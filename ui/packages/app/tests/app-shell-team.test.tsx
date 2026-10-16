import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

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
const WAITING = [{ id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 1 }];

function shell(props: Omit<React.ComponentProps<typeof ShellFrame>, "children"> = {}): string {
  return renderToStaticMarkup(<ShellFrame {...props}><div /></ShellFrame>);
}

afterEach(() => vi.clearAllMocks());

describe("shell for teams", () => {
  it("should show the invite notice above the page only while an invite waits", () => {
    usePathname.mockReturnValue("/w/ws_1/fleets");
    expect(shell({ waitingInvites: WAITING })).toContain(NOTICE);
    expect(shell()).not.toContain(NOTICE);
  });
});
