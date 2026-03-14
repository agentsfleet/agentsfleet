import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { describe, it, expect } from "vitest";
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
  it("renders the brand name in header and footer", () => {
    renderApp();
    const brands = screen.getAllByText("UseZombie");
    expect(brands.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the badge", () => {
    renderApp();
    expect(screen.getByText("agent delivery control plane")).toBeInTheDocument();
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
    expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute("aria-selected", "true");
    // Should now show agents page content
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/autonomous agents/i);
  });

  it("switches to humans mode on click and navigates to /", async () => {
    const user = userEvent.setup();
    renderApp("/agents");

    await user.click(screen.getByRole("tab", { name: "Humans" }));
    expect(screen.getByRole("tab", { name: "Humans" })).toHaveAttribute("aria-selected", "true");
    // Should now show home page content
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/ship ai-generated prs/i);
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

  it("renders only mission control header action in humans mode", () => {
    renderApp("/");
    const header = screen.getByRole("banner");
    expect(within(header).getByRole("link", { name: "Mission Control" })).toHaveAttribute("href", APP_BASE_URL);
    expect(within(header).queryByRole("link", { name: "Connect GitHub, automate PRs" })).not.toBeInTheDocument();
  });

  it("does not render humans header actions in agents mode", () => {
    renderApp("/agents");
    expect(screen.queryByRole("link", { name: "Mission Control" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Connect GitHub, automate PRs" })).not.toBeInTheDocument();
  });

  it("renders home page by default", () => {
    renderApp("/");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/ship ai-generated prs/i);
  });

  it("renders pricing page at /pricing", () => {
    renderApp("/pricing");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/start free\. upgrade when you need stronger control\./i);
  });

  it("renders agents page at /agents", () => {
    renderApp("/agents");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/autonomous agents/i);
  });

  it("renders privacy page at /privacy", () => {
    renderApp("/privacy");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/privacy policy/i);
  });

  it("renders terms page at /terms", () => {
    renderApp("/terms");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/terms of service/i);
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
