import { fireEvent, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { beforeEach, describe, it, expect, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("./analytics/posthog", () => analytics);

import App from "./App";
import { APP_BASE_URL } from "./config";

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
    const brands = screen.getAllByText(/usezombie/i);
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
      "https://docs.usezombie.com",
    );
  });

  it("renders the install CTA in topbar pointing at APP_BASE_URL", () => {
    renderApp();
    const cta = screen.getByTestId("header-install-cta");
    expect(cta).toHaveAttribute("href", APP_BASE_URL);
    expect(cta.textContent).toMatch(/install/i);
  });

  it("clicking the install CTA fires trackSignupStarted with header_install source", () => {
    renderApp();
    fireEvent.click(screen.getByTestId("header-install-cta"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith(
      expect.objectContaining({
        source: "header_install",
        surface: "header",
        mode: "humans",
      }),
    );
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
      screen.getByRole("heading", { level: 1, name: /the agent already knows why/i }),
    ).toBeInTheDocument();
  });

  it("renders pricing page at /pricing", async () => {
    renderApp("/pricing");
    expect(
      await screen.findByRole("heading", { level: 1 }),
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

  it("renders footer on all routes", () => {
    renderApp("/");
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
  });
});
