import { createElement } from "react";
import { render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

// Pin the user-facing destination independently of the configuration export.
const WAITLIST = "https://accounts.agentsfleet.net/waitlist";
const PREVIEW_WAITLIST = "https://accounts-preview.agentsfleet.net/waitlist";
const ACCESS_LINKS = [/^Early access$/i, /^Hosted waitlist ↗$/, /^Join waitlist/i];

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe("Public access follows the configured waitlist destination", () => {
  it.each([
    { configured: undefined, expected: WAITLIST, label: "missing" },
    { configured: "", expected: WAITLIST, label: "empty" },
    { configured: "   ", expected: WAITLIST, label: "whitespace-only" },
    { configured: WAITLIST, expected: WAITLIST, label: "production" },
    { configured: PREVIEW_WAITLIST, expected: PREVIEW_WAITLIST, label: "preview" },
    { configured: ` ${PREVIEW_WAITLIST} `, expected: PREVIEW_WAITLIST, label: "whitespace" },
  ])("uses the expected destination for $label configuration", async ({ configured, expected }) => {
    vi.stubEnv("VITE_WAITLIST_URL", configured);
    vi.resetModules();
    const { PUBLIC_ACCESS_URL, WAITLIST_URL } = await import("./config");
    expect(WAITLIST_URL).toBe(expected);
    expect(PUBLIC_ACCESS_URL).toBe(expected);
  });
});

describe("Public access destinations in the local website", () => {
  it.each(["/", "/#how-it-works"])("opens the waitlist from every entrance on %s", (path) => {
    const router = createMemoryRouter([{ path: "*", element: createElement(App) }], { initialEntries: [path] });
    render(createElement(RouterProvider, { router }));
    for (const name of ACCESS_LINKS) {
      for (const link of screen.getAllByRole("link", { name })) {
        expect(link).toHaveAttribute("href", WAITLIST);
        expect(link).toHaveAttribute("target", "_blank");
        expect(link).toHaveAttribute("rel", "noopener noreferrer");
      }
    }
    expect(screen.getByRole("contentinfo").querySelector('a[href$="/sign-in"]')).toBeNull();
  });
});
