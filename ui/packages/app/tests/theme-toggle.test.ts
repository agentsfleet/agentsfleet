import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { normalizeTheme, DEFAULT_THEME, THEME_COOKIE } from "@/lib/theme";

afterEach(() => {
  cleanup();
  document.documentElement.removeAttribute("data-theme");
  document.cookie = `${THEME_COOKIE}=; path=/; max-age=0`;
});

describe("normalizeTheme", () => {
  it("treats light, dark, unknown, and missing values as the dark default", () => {
    expect(normalizeTheme("light")).toBe("dark");
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

  it("renders no chrome and normalizes the document + cookie back to dark", async () => {
    document.documentElement.dataset.theme = "light";
    await renderToggle();
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("dark");
      expect(document.cookie).toContain(`${THEME_COOKIE}=dark`);
    });
  });
});
