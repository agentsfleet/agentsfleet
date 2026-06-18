import { fireEvent, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("./analytics/posthog", () => analytics);

import App from "./App";
import { HERO_HEADLINE, HERO_PRIMARY_LABEL } from "./lib/marketing-copy";
import { WAITLIST_URL } from "./config";

function renderApp(initialRoute = "/") {
  const router = createMemoryRouter(
    [{ path: "*", element: <App /> }],
    { initialEntries: [initialRoute] }
  );
  return render(<RouterProvider router={router} />);
}

describe("App", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the brand name in topbar and footer", () => {
    renderApp();
    const brands = screen.getAllByText(/agentsfleet/i);
    expect(brands.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the WakePulse brand-mark with data-live attribute", () => {
    renderApp();
    const mark = screen.getByTestId("brand-mark");
    expect(mark).toHaveAttribute("data-live", "true");
  });

  it("renders primary navigation in topbar", () => {
    renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    expect(within(nav).getByRole("link", { name: /home/i })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /pricing/i })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /agents/i })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
  });

  it("renders the early-access CTA in topbar as a waitlist link", () => {
    renderApp();
    const cta = screen.getByTestId("header-install-cta");
    expect(cta.tagName).toBe("A");
    expect(cta).not.toBeDisabled();
    expect(cta).toHaveAttribute("href", WAITLIST_URL);
    expect(cta.textContent).toContain(HERO_PRIMARY_LABEL.toLowerCase());
  });

  it("clicking the early-access CTA tracks a signup", () => {
    renderApp();
    fireEvent.click(screen.getByTestId("header-install-cta"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "header_early_access",
      surface: "header",
      mode: "humans",
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
      await screen.findByRole("heading", { level: 1 }),
    ).toBeInTheDocument();
  });

  it("renders privacy page at /privacy", async () => {
    renderApp("/privacy");
    expect(
      await screen.findByRole("heading", { level: 1, name: /privacy policy/i }),
    ).toBeInTheDocument();
  });

  it("renders terms page at /terms", async () => {
    renderApp("/terms");
    expect(
      await screen.findByRole("heading", { level: 1, name: /terms of service/i }),
    ).toBeInTheDocument();
  });

  it("renders the design-system gallery at /_design-system", async () => {
    // Exercises the lazy() factory for the gallery chunk — the only route
    // whose code-split import arrow was never fired by the suite, leaving
    // App.tsx one function short of full coverage.
    renderApp("/_design-system");
    expect(
      await screen.findByRole("heading", { level: 1, name: /design system gallery/i }),
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

    it("scrolls #pricing into view when /pricing redirects to /#pricing (greptile fix)", () => {
      // Bug-fix coverage: <Navigate to="/#pricing"> via the /pricing route
      // must end up scrolling, not just updating the URL bar. Without
      // useScrollToHash this assertion failed and the bookmarked
      // /pricing URL stranded the user at the top of Home.
      renderApp("/pricing");
      const pricingBlock = screen.getByTestId("pricing-block");
      expect(scrollIntoViewSpy).toHaveBeenCalled();
      expect(scrollIntoViewSpy.mock.instances.at(-1)).toBe(pricingBlock);
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
