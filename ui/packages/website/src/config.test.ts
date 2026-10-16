import { createElement } from "react";
import { render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

// Pin the user-facing destination independently of the configuration export.
const HOSTED_SIGN_UP = "https://app.agentsfleet.net/sign-up";
const PRODUCTION_APP = "https://app.agentsfleet.net";
const DEVELOPMENT_APP = "https://app-dev.agentsfleet.net";
const SIGNUP_LINKS = [/^Get started$/i, /^Hosted ↗$/, /^Create your first fleet/i];

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe("Application destinations follow the configured build environment", () => {
  it.each([
    { configured: undefined, expected: PRODUCTION_APP, label: "missing" },
    { configured: "", expected: PRODUCTION_APP, label: "empty" },
    { configured: "   ", expected: PRODUCTION_APP, label: "whitespace-only" },
    { configured: PRODUCTION_APP, expected: PRODUCTION_APP, label: "production" },
    { configured: DEVELOPMENT_APP, expected: DEVELOPMENT_APP, label: "development preview" },
    { configured: `${DEVELOPMENT_APP}/`, expected: DEVELOPMENT_APP, label: "trailing slash" },
    { configured: ` ${DEVELOPMENT_APP}/// `, expected: DEVELOPMENT_APP, label: "whitespace and trailing slashes" },
  ])("uses the expected app for $label configuration", async ({ configured, expected }) => {
    vi.stubEnv("VITE_APP_BASE_URL", configured);
    vi.resetModules();
    const { APP_SIGN_IN_URL, APP_SIGN_UP_URL } = await import("./config");
    expect(APP_SIGN_IN_URL).toBe(`${expected}/sign-in`);
    expect(APP_SIGN_UP_URL).toBe(`${expected}/sign-up`);
  });
});

describe("Hosted sign-up destinations in the local website", () => {
  it.each(["/", "/#how-it-works"])("opens the application from every entrance on %s", (path) => {
    const router = createMemoryRouter([{ path: "*", element: createElement(App) }], { initialEntries: [path] });
    render(createElement(RouterProvider, { router }));
    for (const name of SIGNUP_LINKS) {
      for (const link of screen.getAllByRole("link", { name })) {
        expect(link).toHaveAttribute("href", HOSTED_SIGN_UP);
        expect(link).toHaveAttribute("target", "_blank");
        expect(link).toHaveAttribute("rel", "noopener noreferrer");
      }
    }
    const createLabel = path === "/" ? "Create your first fleet" : "Create fleet";
    for (const link of screen.getAllByRole("link", { name: createLabel })) {
      expect(link).toHaveAttribute("href", HOSTED_SIGN_UP);
    }
  });
});
