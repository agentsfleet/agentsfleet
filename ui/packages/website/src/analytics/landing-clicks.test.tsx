import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import App from "../App";
import { flushAnalyticsForTests, resetAnalyticsForTests } from "./posthog";

vi.mock("posthog-js", () => ({ default: { init: vi.fn(), capture: vi.fn() } }));
const sdk = vi.mocked(posthog, true);
const NAVIGATION = "navigation_clicked";
const SIGNUP = "signup_started";
const INTERACTION = "interaction_clicked";
const PRIVATE_REQUEST = "Investigate private-customer-outage-742";
const EXAMPLE_PATH = "/examples#how-it-works";
const FOOTER_LINKS = [
  { name: "Examples", source: "footer_examples", target: "examples" },
  { name: "Docs", source: "footer_docs", target: "docs" },
  { name: "Agents", source: "footer_agents", target: "agents" },
  { name: "GitHub", source: "footer_github", target: "github" },
  { name: "Discord", source: "footer_discord", target: "discord" },
  { name: "Privacy", source: "footer_privacy", target: "privacy" },
  { name: "Terms", source: "footer_terms", target: "terms" },
  { name: "Contact", source: "footer_contact", target: "contact" },
] as const;
const SUGGESTIONS = [
  { role: "link", name: /Investigate an incident/, target: "incident" },
  { role: "link", name: /Catch test regressions/, target: "tests" },
  { role: "link", name: /Automate recurring work/, target: "agent" },
] as const;

async function renderLanding(path = "/examples") {
  const router = createMemoryRouter([{ path: "*", element: <App /> }], { initialEntries: [path] });
  render(<RouterProvider router={router} />);
  await screen.findByTestId(path === "/agents" ? "fleets-page" : "hero", {}, { timeout: 10_000 });
}

async function expectEvent(event: string, props: Record<string, string | undefined>) {
  await flushAnalyticsForTests();
  expect(sdk.capture).toHaveBeenCalledTimes(1);
  expect(sdk.capture).toHaveBeenCalledWith(event, { ...props, path: "/" });
}

beforeEach(() => {
  resetAnalyticsForTests();
  sdk.init.mockReset();
  sdk.capture.mockReset();
  (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
    enabled: true, key: ["phc", "synthetic", "fixture"].join("_"), host: "https://us.i.posthog.com",
  };
});
afterEach(() => {
  resetAnalyticsForTests();
  vi.restoreAllMocks();
  delete (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__;
});

describe("website click attribution", () => {
  it.each([
    { name: /^Product$/, event: NAVIGATION, source: "header_nav_product", target: "example" },
    { name: /^Pricing$/, event: NAVIGATION, source: "header_pricing", target: "pricing" },
    { name: /^Docs$/, event: NAVIGATION, source: "header_nav_docs", target: "docs" },
    { name: /^Examples$/, event: NAVIGATION, source: "header_examples", target: "examples" },
  ])("tracks $source from primary navigation", async ({ name, event, ...props }) => {
    await renderLanding();
    fireEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("link", { name }));
    await expectEvent(event, { ...props, surface: "header" });
  });
  it("tracks the product link on the examples page", async () => {
    await renderLanding();
    fireEvent.click(within(screen.getByTestId("audience-section")).getByRole("link", { name: "Read how fleets work" }));
    await expectEvent(NAVIGATION, { source: "continuity_product", surface: "continuity", target: "product" });
  });
  it.each(FOOTER_LINKS)("tracks $source from the footer", async ({ name, ...props }) => {
    await renderLanding();
    fireEvent.click(within(screen.getByRole("contentinfo")).getByRole("link", { name }));
    await expectEvent(NAVIGATION, { ...props, surface: "footer" });
  });
  it.each(SUGGESTIONS)("tracks the $target suggestion without the request", async ({ role, name, target }) => {
    await renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole(role, { name }));
    await expectEvent(INTERACTION, { source: `hero_suggestion_${target}`, surface: "hero", target });
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it("tracks Copy on the displayed fleet commands", async () => {
    const user = userEvent.setup();
    const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    await renderLanding("/agents");
    await user.click(screen.getByRole("button", { name: "Copy command" }));
    expect(write).toHaveBeenCalledWith(expect.stringContaining(`agentsfleet login\nagentsfleet fleet create`));
    await expectEvent(INTERACTION, { source: "cli_quickstart_copy", surface: "cli_quickstart", target: "fleet_create" });
  });
  it("tracks Join waitlist without exposing the private composer text", async () => {
    await renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    expect(sdk.capture).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByTestId("hero")).getByRole("link", { name: "Join waitlist" }));
    await expectEvent(SIGNUP, { source: "composer_create_fleet", surface: "hero", mode: "humans" });
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it("tracks the example's Join waitlist action", async () => {
    await renderLanding(EXAMPLE_PATH);
    fireEvent.click(within(screen.getByTestId("how-it-works")).getByRole("link", { name: "Join waitlist" }));
    await expectEvent(SIGNUP, { source: "example_create_fleet", surface: "example", mode: "humans" });
  });
  it("tracks exploration and returning home while preserving private text", async () => {
    await renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: "Explore an example run" }));
    await expectEvent(NAVIGATION, { source: "composer_example", surface: "hero", target: "incident" });
    sdk.capture.mockClear();
    fireEvent.click(screen.getByRole("link", { name: "Back to examples" }));
    await expectEvent(NAVIGATION, { source: "example_back", surface: "example", target: "home" });
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it("tracks reading the full conversation and exposes the complete report", async () => {
    vi.spyOn(window, "matchMedia").mockImplementation((media) => ({
      media, matches: false, onchange: null, addListener: vi.fn(), removeListener: vi.fn(),
      addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(() => true),
    }));
    await renderLanding(EXAMPLE_PATH);
    fireEvent.click(screen.getByRole("button", { name: "Read full conversation" }));
    await expectEvent(INTERACTION, { source: "example_read_full", surface: "example", target: "incident" });
    expect(screen.getByRole("figure")).toHaveTextContent("BROWSER_TIMEOUT=10");
    expect(screen.queryByRole("button", { name: "Read full conversation" })).toBeNull();
  });
  it("tracks the example's repository link", async () => {
    await renderLanding(EXAMPLE_PATH);
    fireEvent.click(screen.getByRole("link", { name: /agentsfleet\/linkwarden/ }));
    await expectEvent(NAVIGATION, { source: "example_repository", surface: "example", target: "github" });
  });
});
