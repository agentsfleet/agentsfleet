import React from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TabNav, type TabNavItem } from "./TabNav";
import { TAB_LIST_CLASS, TAB_TRIGGER_CLASS_LINK } from "./tab-styles";

const ITEMS: TabNavItem[] = [
  { label: "Basic Info", href: "/settings" },
  { label: "API Keys", href: "/settings/api-keys" },
];

describe("TabNav", () => {
  it("renders each item as a link inside the labelled nav landmark", () => {
    render(<TabNav label="Settings sections" items={ITEMS} activeHref="/settings" />);
    expect(screen.getByRole("navigation", { name: "Settings sections" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Basic Info" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "API Keys" })).toBeTruthy();
  });

  it("marks only the active tab with aria-current + data-active", () => {
    render(<TabNav label="x" items={ITEMS} activeHref="/settings/api-keys" />);
    const active = screen.getByRole("link", { name: "API Keys" });
    expect(active.getAttribute("aria-current")).toBe("page");
    expect(active.getAttribute("data-active")).toBe("true");
    const inactive = screen.getByRole("link", { name: "Basic Info" });
    expect(inactive.getAttribute("aria-current")).toBeNull();
    expect(inactive.getAttribute("data-active")).toBeNull();
  });

  it("renders through a custom link component (framework-agnostic injection)", () => {
    const CustomLink = (props: Record<string, unknown>) =>
      React.createElement("a", { ...props, "data-custom": "1" });
    render(<TabNav label="x" items={ITEMS} activeHref="/settings" linkComponent={CustomLink} />);
    expect(screen.getByRole("link", { name: "Basic Info" }).getAttribute("data-custom")).toBe("1");
  });

  it("fires onNavigate with the clicked item's href", () => {
    const onNavigate = vi.fn();
    render(<TabNav label="x" items={ITEMS} activeHref="/settings" onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("link", { name: "API Keys" }));
    expect(onNavigate).toHaveBeenCalledWith("/settings/api-keys");
  });

  it("falls back to a native <a href> when no linkComponent is injected", () => {
    render(<TabNav label="x" items={ITEMS} activeHref="/settings" />);
    const link = screen.getByRole("link", { name: "Basic Info" });
    expect(link.tagName).toBe("A");
    expect(link.getAttribute("href")).toBe("/settings");
  });

  // TabNav shares the one underline tab style (pill retired).
  it("uses the shared underline visual; active lights to --pulse, no pill", () => {
    render(<TabNav label="x" items={ITEMS} activeHref="/settings" />);
    const nav = screen.getByRole("navigation");
    expect(nav.className).toContain("border-b");
    expect(nav.className).not.toContain("bg-muted");
    expect(TAB_LIST_CLASS).toContain("border-b");
    const active = screen.getByRole("link", { name: "Basic Info" });
    expect(active.className).toContain("border-b-2");
    expect(active.className).toContain("data-[active=true]:border-pulse");
    expect(active.className).not.toContain("data-[active=true]:bg-background");
    expect(active.className).not.toContain("rounded-md");
    expect(TAB_TRIGGER_CLASS_LINK).not.toContain("bg-background");
  });
});
