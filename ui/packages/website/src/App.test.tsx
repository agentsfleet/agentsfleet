import { fireEvent, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("./analytics/posthog", () => analytics);

import App from "./App";
import { HERO_HEADLINE } from "./lib/marketing-copy";
import { PUBLIC_ACCESS_URL } from "./config";

async function renderApp(initialRoute = "/examples") {
  const router = createMemoryRouter(
    [{ path: "*", element: <App /> }],
    { initialEntries: [initialRoute] }
  );
  render(<RouterProvider router={router} />);
  await screen.findByRole("main", {}, { timeout: LAZY_ROUTE_TIMEOUT_MS });
}

const APP_TEST_TIMEOUT_MS = 20_000;

// `findBy*` carries its OWN 1s timeout, independent of the describe-level
// timeout above. Every route below is a React.lazy chunk, so the assertion is
// racing a dynamic import: observed failures clocked 1367-1709ms on a loaded
// machine, all of them just past the 1s default rather than genuinely broken.
// The bound the tests mean is "the chunk eventually resolves", so it is stated
// explicitly instead of inherited.
const LAZY_ROUTE_TIMEOUT_MS = 10_000;

describe("App", { timeout: APP_TEST_TIMEOUT_MS }, () => {
  it("offers recovery from an unknown URL with a link to the homepage", async () => {
    await renderApp("/missing-page");
    await screen.findByRole("heading", { name: "This page isn’t here." }, { timeout: LAZY_ROUTE_TIMEOUT_MS });
    expect(screen.getByRole("link", { name: "Explore agents" })).toHaveAttribute("href", "/agents");
    expect(screen.getByRole("link", { name: "Back to home" })).toHaveAttribute("href", "/");
  });

  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the brand name in topbar and footer", async () => {
    await renderApp();
    const brands = screen.getAllByText(/agentsfleet/i);
    expect(brands.length).toBeGreaterThanOrEqual(2);
  });

  it("keeps the brand mark distinct from live activity", async () => {
    await renderApp();
    const mark = screen.getByTestId("brand-mark");
    expect(mark).not.toHaveAttribute("data-live");
  });

  it("renders primary navigation in topbar", async () => {
    await renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    expect(within(nav).getByRole("link", { name: "Pricing" })).toHaveAttribute("href", "/#pricing");
    expect(within(nav).getByRole("link", { name: /^Product$/i })).toHaveAttribute("href", "/#how-it-works");
    expect(within(nav).getByRole("link", { name: "GitHub" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
  });

  it("routes public access to the waitlist", async () => {
    await renderApp();
    const cta = screen.getByTestId("header-install-cta");
    expect(cta.tagName).toBe("A");
    expect(cta).not.toBeDisabled();
    expect(cta).toHaveAttribute("href", PUBLIC_ACCESS_URL);
    expect(cta).toHaveAttribute("target", "_blank");
    expect(cta).toHaveAttribute("rel", "noopener noreferrer");
    expect(cta.textContent).toContain("Join waitlist");
  });

  it("records signup intent when clicking Join waitlist", async () => {
    await renderApp();
    fireEvent.click(screen.getByTestId("header-install-cta"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "header_signup",
      surface: "header",
      mode: "humans",
    });
  });

  it("clicking the header docs link fires trackNavigationClicked with header_nav_docs source", async () => {
    await renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    fireEvent.click(within(nav).getByRole("link", { name: /docs/i }));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "header_nav_docs",
      surface: "header",
      target: "docs",
    });
  });

  it("renders the job composer at /examples", async () => {
    await renderApp("/examples");
    expect(await screen.findByTestId("hero", {}, { timeout: LAZY_ROUTE_TIMEOUT_MS })).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 1, name: HERO_HEADLINE }),
    ).toBeInTheDocument();
  });

  it("renders agents page at /agents", async () => {
    await renderApp("/agents");
    expect(
      await screen.findByRole("heading", { level: 1 }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders privacy page at /privacy", async () => {
    await renderApp("/privacy");
    expect(
      await screen.findByRole("heading", { level: 1, name: /privacy policy/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders terms page at /terms", async () => {
    await renderApp("/terms");
    expect(
      await screen.findByRole("heading", { level: 1, name: /terms of service/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders the design-system gallery at /_design-system", async () => {
    // Exercises the lazy() factory for the gallery chunk — the only route
    // whose code-split import arrow was never fired by the suite, leaving
    // App.tsx one function short of full coverage.
    await renderApp("/_design-system");
    expect(
      await screen.findByRole("heading", { level: 1, name: /design system gallery/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders the shared footer when the route is ready", async () => {
    await renderApp("/examples");
    expect(await screen.findByRole("contentinfo", {}, { timeout: LAZY_ROUTE_TIMEOUT_MS })).toBeInTheDocument();
  });

  describe("hash anchor scroll", () => {
    let scrollIntoViewSpy: ReturnType<typeof vi.fn>;
    let rafSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      scrollIntoViewSpy = vi.fn();
      // jsdom doesn't implement scrollIntoView; install a spy on the prototype.
      Element.prototype.scrollIntoView =
        scrollIntoViewSpy as unknown as Element["scrollIntoView"];
      // Run requestAnimationFrame callbacks synchronously so the effect
      // fires inside the test rather than after teardown.
      rafSpy = vi
        .spyOn(window, "requestAnimationFrame")
        .mockImplementation((cb: FrameRequestCallback) => {
          cb(0);
          return 0;
        });
    });

    afterEach(() => {
      rafSpy.mockRestore();
    });

    it("scrolls #pricing into view when arriving directly at /examples#pricing", async () => {
      await renderApp("/examples#pricing");
      const pricingBlock = await screen.findByTestId("pricing-block", {}, { timeout: LAZY_ROUTE_TIMEOUT_MS });
      expect(scrollIntoViewSpy).toHaveBeenCalled();
      // The element scrolled into view IS the #pricing section
      // (not some other id we happen to render).
      expect(scrollIntoViewSpy.mock.instances[0]).toBe(pricingBlock);
    });

    it("does not call scrollIntoView when location has no hash", async () => {
      await renderApp("/examples");
      expect(scrollIntoViewSpy).not.toHaveBeenCalled();
    });

    it("does not crash when the hash targets an id that is not in the DOM", async () => {
      await renderApp("/examples#does-not-exist");
      // Hook should bail silently — no scroll, no throw.
      expect(scrollIntoViewSpy).not.toHaveBeenCalled();
    });
  });
});

it.each(["/fleets", "/pricing"])("renders not-found at %s", async (path) => {
  await renderApp(path);
  expect(await screen.findByRole("heading", { level: 1, name: "This page isn’t here." }, { timeout: LAZY_ROUTE_TIMEOUT_MS })).toBeInTheDocument();
});
