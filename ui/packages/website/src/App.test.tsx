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
import { APP_SIGN_IN_URL, APP_SIGN_UP_URL } from "./config";

function renderApp(initialRoute = "/") {
  const router = createMemoryRouter(
    [{ path: "*", element: <App /> }],
    { initialEntries: [initialRoute] }
  );
  return render(<RouterProvider router={router} />);
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
  it("offers recovery from an unknown URL and follows its home link", async () => {
    renderApp("/missing-page");
    await screen.findByRole("heading", { name: "This page isn’t here." }, { timeout: LAZY_ROUTE_TIMEOUT_MS });
    expect(screen.getByRole("link", { name: "Explore agents" })).toHaveAttribute("href", "/agents");
    fireEvent.click(screen.getByRole("link", { name: "Back to home" }));
    expect(await screen.findByRole("heading", { name: HERO_HEADLINE }, { timeout: LAZY_ROUTE_TIMEOUT_MS })).toBeInTheDocument();
  });

  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the brand name in topbar and footer", () => {
    renderApp();
    const brands = screen.getAllByText(/agentsfleet/i);
    expect(brands.length).toBeGreaterThanOrEqual(2);
  });

  it("keeps the brand mark distinct from live activity", () => {
    renderApp();
    const mark = screen.getByTestId("brand-mark");
    expect(mark).not.toHaveAttribute("data-live");
  });

  it("renders primary navigation in topbar", () => {
    renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    const earlyAccess = within(nav).getByRole("link", { name: /Get started/i });
    expect(earlyAccess).toHaveAttribute("href", APP_SIGN_UP_URL);
    expect(earlyAccess).toHaveAttribute("target", "_blank");
    expect(earlyAccess).toHaveAttribute("rel", "noopener noreferrer");
    expect(within(nav).getByRole("link", { name: /^Product$/i })).toHaveAttribute("href", "/#how-it-works");
    expect(within(nav).getByRole("link", { name: "GitHub ↗" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
  });

  it("routes sign-in to the existing app", () => {
    renderApp();
    const cta = screen.getByTestId("header-install-cta");
    expect(cta.tagName).toBe("A");
    expect(cta).not.toBeDisabled();
    expect(cta).toHaveAttribute("href", APP_SIGN_IN_URL);
    expect(cta).toHaveAttribute("target", "_blank");
    expect(cta).toHaveAttribute("rel", "noopener noreferrer");
    expect(cta.textContent).toContain("Sign in");
  });

  it("clicking Dashboard tracks navigation", () => {
    renderApp();
    fireEvent.click(screen.getByTestId("header-install-cta"));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "header_dashboard",
      surface: "header",
      target: "dashboard",
    });
  });

  it("clicking the header docs link fires trackNavigationClicked with header_nav_docs source", () => {
    renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    fireEvent.click(within(nav).getByRole("link", { name: /docs/i }));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "header_nav_docs",
      surface: "header",
      target: "docs",
    });
  });

  it("renders home hero by default", () => {
    renderApp("/");
    expect(screen.getByTestId("hero")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 1, name: HERO_HEADLINE }),
    ).toBeInTheDocument();
  });

  it("renders agents page at /agents", async () => {
    renderApp("/agents");
    expect(
      await screen.findByRole("heading", { level: 1 }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders privacy page at /privacy", async () => {
    renderApp("/privacy");
    expect(
      await screen.findByRole("heading", { level: 1, name: /privacy policy/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders terms page at /terms", async () => {
    renderApp("/terms");
    expect(
      await screen.findByRole("heading", { level: 1, name: /terms of service/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders the design-system gallery at /_design-system", async () => {
    // Exercises the lazy() factory for the gallery chunk — the only route
    // whose code-split import arrow was never fired by the suite, leaving
    // App.tsx one function short of full coverage.
    renderApp("/_design-system");
    expect(
      await screen.findByRole("heading", { level: 1, name: /design system gallery/i }, { timeout: LAZY_ROUTE_TIMEOUT_MS }),
    ).toBeInTheDocument();
  });

  it("renders footer on all routes", () => {
    renderApp("/");
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
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

    it("scrolls #pricing into view when arriving directly at /#pricing", () => {
      renderApp("/#pricing");
      const pricingBlock = screen.getByTestId("pricing-block");
      expect(scrollIntoViewSpy).toHaveBeenCalled();
      // The element scrolled into view IS the #pricing section
      // (not some other id we happen to render).
      expect(scrollIntoViewSpy.mock.instances[0]).toBe(pricingBlock);
    });

    it("does not call scrollIntoView when location has no hash", () => {
      renderApp("/");
      expect(scrollIntoViewSpy).not.toHaveBeenCalled();
    });

    it("does not crash when the hash targets an id that is not in the DOM", () => {
      renderApp("/#does-not-exist");
      // Hook should bail silently — no scroll, no throw.
      expect(scrollIntoViewSpy).not.toHaveBeenCalled();
    });
  });
});

it.each(["/fleets", "/pricing"])("renders not-found at %s", async (path) => {
  renderApp(path);
  expect(await screen.findByRole("heading", { level: 1, name: "This page isn’t here." }, { timeout: LAZY_ROUTE_TIMEOUT_MS })).toBeInTheDocument();
});
