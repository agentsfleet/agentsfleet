import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

// ── Shared mocks ───────────────────────────────────────────────────────────
// SettingsTabs is the thin Next adapter over the design-system <TabNav>: it
// injects usePathname + <Link> + nav analytics. Mock those three boundaries and
// drive the real TabNav so the active-tab computation is proven end to end.

const { usePathnameMock, trackNavigationClickedMock } = vi.hoisted(() => ({
  usePathnameMock: vi.fn(),
  trackNavigationClickedMock: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  usePathname: usePathnameMock,
}));

vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  trackNavigationClicked: trackNavigationClickedMock,
}));

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => cleanup());

async function renderTabs(pathname: string) {
  usePathnameMock.mockReturnValue(pathname);
  const { default: SettingsTabs } = await import("../components/layout/SettingsTabs");
  render(React.createElement(SettingsTabs));
}

describe("SettingsTabs active-tab computation", () => {
  it("marks Basic Info active on the /settings index (exact match)", async () => {
    await renderTabs("/settings");
    expect(screen.getByRole("link", { name: "Basic Info" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "API Keys" }).getAttribute("aria-current")).toBeNull();
  });

  it("marks API Keys active on /settings/api-keys", async () => {
    await renderTabs("/settings/api-keys");
    expect(screen.getByRole("link", { name: "API Keys" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Basic Info" }).getAttribute("aria-current")).toBeNull();
  });

  it("keeps API Keys active on a nested child route (prefix match)", async () => {
    await renderTabs("/settings/api-keys/0190abc");
    expect(screen.getByRole("link", { name: "API Keys" }).getAttribute("aria-current")).toBe("page");
  });

  it("does not light up Basic Info for a nested api-keys route (index is exact-match only)", async () => {
    // Regression guard: the /settings index must NOT prefix-match every sub-route,
    // otherwise both tabs would read aria-current on any deeper page.
    await renderTabs("/settings/api-keys");
    expect(screen.getByRole("link", { name: "Basic Info" }).getAttribute("aria-current")).toBeNull();
  });

  it("falls back to Basic Info for a settings route absent from the tab bar", async () => {
    // /settings/defaults is a masked route with no tab entry → no tab should
    // crash or double-activate; activeHref defaults to the index.
    await renderTabs("/settings/defaults");
    expect(screen.getByRole("link", { name: "Basic Info" }).getAttribute("aria-current")).toBe("page");
  });

  it("emits namespaced nav analytics with the clicked tab's slug on click", async () => {
    await renderTabs("/settings");
    fireEvent.click(screen.getByRole("link", { name: "API Keys" }));
    expect(trackNavigationClickedMock).toHaveBeenCalledWith({
      source: "settings_tabs_api-keys",
      surface: "settings_tabs",
      target: "/settings/api-keys",
    });
  });

  it("maps the index tab to the 'basic' analytics slug", async () => {
    await renderTabs("/settings/api-keys");
    fireEvent.click(screen.getByRole("link", { name: "Basic Info" }));
    expect(trackNavigationClickedMock).toHaveBeenCalledWith({
      source: "settings_tabs_basic",
      surface: "settings_tabs",
      target: "/settings",
    });
  });
});
