import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { beforeEach, describe, it, expect, vi } from "vitest";

// Hoisted analytics mock — App.tsx fires trackSignupStarted on the header
// "Try usezombie" CTA. Pinning this lets us assert click → analytics call
// shape without booting PostHog in jsdom.
const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("./analytics/posthog", () => analytics);

import App from "./App";
import { APP_BASE_URL } from "./config";

function renderApp(initialRoute = "/") {
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: <App />,
      },
    ],
    { initialEntries: [initialRoute] }
  );

  return render(
    <RouterProvider router={router} />
  );
}

describe("App", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the brand name in header and footer", () => {
    renderApp();
    const brands = screen.getAllByText("usezombie");
    expect(brands.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the badge surfacing the always-on event-driven framing", () => {
    renderApp();
    expect(screen.getByText("always-on · event-driven · markdown-defined")).toBeInTheDocument();
  });

  it("'Try usezombie' header CTA carries the gradient pill class + drop-in hand", () => {
    const { container } = renderApp("/");
    const link = within(screen.getByRole("banner")).getByRole("link", { name: /try usezombie/i });
    // The gradient pill is the only fully-coloured header CTA — class name is the
    // anchor for that visual treatment.
    expect(link.className).toMatch(/header-mission-control/);
    // Hand drops in via AnimatedIcon animation="drop"; in jsdom we assert the
    // utility class chain that produces the hover/focus animate-drop trigger.
    const glyph = container.querySelector(
      ".header-mission-control [data-animated-glyph]",
    ) as HTMLElement | null;
    expect(glyph).not.toBeNull();
    expect(glyph!.className).toContain("group-hover:animate-drop");
    expect(glyph!.className).toContain("group-focus-visible:animate-drop");
  });

  it("renders the mode switch", () => {
    renderApp();
    expect(screen.getByRole("tablist", { name: /mode switch/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Humans" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Agents" })).toBeInTheDocument();
  });

  it("defaults to humans mode on /", () => {
    renderApp("/");
    expect(screen.getByRole("tab", { name: "Humans" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute("aria-selected", "false");
  });

  it("shows agents mode when on /agents", () => {
    renderApp("/agents");
    expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Humans" })).toHaveAttribute("aria-selected", "false");
  });

  it("switches to agents mode on click and navigates to /agents", async () => {
    const user = userEvent.setup();
    renderApp("/");

    await user.click(screen.getByRole("tab", { name: "Agents" }));
    // Wait for the lazy /agents route to resolve so the URL-driven
    // aria-selected flip settles before assertion.
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/autonomous agents/i),
    );
  });

  it("switches to humans mode on click and navigates to /", async () => {
    const user = userEvent.setup();
    renderApp("/agents");

    await user.click(screen.getByRole("tab", { name: "Humans" }));
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Humans" })).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/agents that wake on every event/i),
    );
  });

  it("renders primary navigation in header", () => {
    renderApp();
    const nav = screen.getByRole("navigation", { name: /primary/i });
    expect(within(nav).getByRole("link", { name: "Home" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Pricing" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Agents" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Docs" })).toHaveAttribute(
      "href",
      "https://docs.usezombie.com"
    );
  });

  it("renders only the Try usezombie header action in humans mode", () => {
    renderApp("/");
    const header = screen.getByRole("banner");
    expect(within(header).getByRole("link", { name: "Try usezombie" })).toHaveAttribute("href", APP_BASE_URL);
    expect(within(header).queryByRole("link", { name: /start an agent/i })).not.toBeInTheDocument();
  });

  it("clicking 'Try usezombie' fires trackSignupStarted with header_mission_control source", () => {
    renderApp("/");
    const link = within(screen.getByRole("banner")).getByRole("link", { name: "Try usezombie" });
    fireEvent.click(link);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith(
      expect.objectContaining({
        source: "header_mission_control",
        surface: "header",
        mode: "humans",
      }),
    );
  });

  it("'Try usezombie' is aria-hidden + non-focusable in agents mode (a11y guard)", () => {
    const { container } = renderApp("/agents");
    // The link still mounts to keep the layout grid stable, but it must be
    // hidden from the a11y tree and unreachable by keyboard navigation.
    // Button is rendered with asChild — the anchor is the root element with
    // the .header-mission-control class on it directly (no inner <a>).
    const anchor = container.querySelector(
      "a.header-mission-control",
    ) as HTMLAnchorElement | null;
    expect(anchor).not.toBeNull();
    expect(anchor!.getAttribute("aria-hidden")).toBe("true");
    expect(anchor!.tabIndex).toBe(-1);
    expect(anchor!.className).toContain("is-hidden");
  });

  it("does not render humans header actions in agents mode", () => {
    renderApp("/agents");
    const header = screen.getByRole("banner");
    expect(within(header).queryByRole("link", { name: "Try usezombie" })).not.toBeInTheDocument();
    expect(within(header).queryByRole("link", { name: /start an agent/i })).not.toBeInTheDocument();
  });

  it("renders home page by default", () => {
    renderApp("/");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/agents that wake on every event/i);
  });

  it("renders pricing page at /pricing", async () => {
    renderApp("/pricing");
    // Lazy-loaded route — Suspense defers render until the chunk resolves.
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: /start free\. upgrade when you need stronger control\./i,
      }),
    ).toBeInTheDocument();
  });

  it("renders agents page at /agents", async () => {
    renderApp("/agents");
    expect(
      await screen.findByRole("heading", { level: 1, name: /autonomous agents/i }),
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

  it("renders particle field for animated background", () => {
    const { container } = renderApp();
    expect(container.querySelector(".particle-field")).toBeInTheDocument();
  });
});
