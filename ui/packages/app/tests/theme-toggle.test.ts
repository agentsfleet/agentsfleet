import React from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { normalizeTheme, DEFAULT_THEME, THEME_COOKIE } from "@/lib/theme";

afterEach(() => {
  cleanup();
  document.documentElement.removeAttribute("data-theme");
  document.cookie = `${THEME_COOKIE}=; path=/; max-age=0`;
});

describe("normalizeTheme", () => {
  it("accepts the persisted light choice", () => {
    expect(normalizeTheme("light")).toBe("light");
  });
  it("treats dark, unknown, and missing values as the dark default", () => {
    expect(normalizeTheme("dark")).toBe("dark");
    expect(normalizeTheme("garbage")).toBe("dark");
    expect(normalizeTheme(undefined)).toBe("dark");
    expect(DEFAULT_THEME).toBe("dark");
  });
});

describe("ThemeToggle", () => {
  async function renderToggle() {
    const { default: ThemeToggle } = await import("../components/layout/ThemeToggle");
    render(React.createElement(ThemeToggle));
  }

  it("defaults to dark and flips <html data-theme> + persists a cookie on toggle", async () => {
    const user = userEvent.setup();
    await renderToggle();
    // Dark default → the button offers to switch to light.
    const btn = screen.getByRole("button", { name: /switch to light theme/i });
    await user.click(btn);
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.cookie).toContain(`${THEME_COOKIE}=light`);
    // Now offers dark; toggling back removes light.
    await user.click(screen.getByRole("button", { name: /switch to dark theme/i }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE}=dark`);
  });

  it("syncs its icon from the SSR-stamped attribute on mount (no flip)", async () => {
    document.documentElement.dataset.theme = "light";
    await renderToggle();
    // SSR said light → the mounted control offers to switch to dark.
    expect(screen.getByRole("button", { name: /switch to dark theme/i })).toBeTruthy();
  });
});
