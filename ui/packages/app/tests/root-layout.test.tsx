import React, { type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// The keeper renders nothing of its own, so a marker stands in for it: what is
// under test is that every route group's root mounts it once.
const { KEEPER_MARKER } = vi.hoisted(() => ({ KEEPER_MARKER: "data-session-keeper" }));

vi.mock("@/lib/auth/client", () => ({
  AuthProvider: ({ children }: { children: ReactNode }) => children,
  AuthSessionKeeper: () => React.createElement("span", { [KEEPER_MARKER]: "" }),
}));
vi.mock("@/components/analytics/AnalyticsBootstrap", () => ({ default: () => null }));

import RootLayout from "@/app/layout";

describe("root layout", () => {
  it("should mount the session keeper once above every route's content", () => {
    const markup = renderToStaticMarkup(RootLayout({ children: null }));
    expect(markup.split(KEEPER_MARKER).length - 1).toBe(1);
    expect(markup).toContain('data-theme="dark"');
  });
});
