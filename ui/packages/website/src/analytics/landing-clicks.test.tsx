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
const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";
const EXAMPLE_PATH = "/#how-it-works";
const LINKS = [
  { area: "hero", name: /Installation guide/, source: "cli_install_guide", surface: "cli_quickstart", target: "installation" },
  { area: "hero", name: /Self hosting in your infrastructure/, source: "cli_self_hosted", surface: "cli_quickstart", target: "self_hosted" },
  { area: "audience-section", name: /Read the product docs/, source: "continuity_docs", surface: "continuity", target: "docs" },
  { area: "audience-section", name: /^GitHub ↗$/, source: "source_github", surface: "source_strip", target: "github" },
  { area: "audience-section", name: /Self hosting in your infrastructure/, source: "source_self_hosted", surface: "source_strip", target: "self_hosted" },
  { area: "pricing-block", name: /Tell us about your workflow/, source: "pricing_contact", surface: "pricing", target: "email" },
] as const;
const FOOTER_LINKS = [
  { name: "use cases", source: "footer_use_cases", target: "example" },
  { name: "join waitlist", source: "footer_sign_in", target: "sign_in" },
  { name: "docs", source: "footer_docs", target: "docs" },
  { name: "agents", source: "footer_agents", target: "agents" },
  { name: "github", source: "footer_github", target: "github" },
  { name: "discord", source: "footer_discord", target: "discord" },
  { name: "privacy", source: "footer_privacy", target: "privacy" },
  { name: "terms", source: "footer_terms", target: "terms" },
  { name: "Contact", source: "footer_contact", target: "contact" },
] as const;
const SUGGESTIONS = [
  { role: "link", name: /Investigate an incident/, target: "incident" },
  { role: "link", name: /Catch test regressions/, target: "tests" },
  { role: "link", name: /Automate recurring work/, target: "agent" },
] as const;

function renderLanding(path = "/") {
  const router = createMemoryRouter([{ path: "*", element: <App /> }], { initialEntries: [path] });
  return render(<RouterProvider router={router} />);
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
    { name: /^Early access$/, event: SIGNUP, source: "header_early_access", mode: "humans" },
    { name: /^Docs$/, event: NAVIGATION, source: "header_nav_docs", target: "docs" },
    { name: /^GitHub ↗$/, event: NAVIGATION, source: "header_nav_source", target: "github" },
  ])("tracks $source from primary navigation", async ({ name, event, ...props }) => {
    renderLanding();
    fireEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("link", { name }));
    await expectEvent(event, { ...props, surface: "header" });
  });
  it.each(LINKS)("tracks $source exactly once", async ({ area, name, ...props }) => {
    renderLanding();
    fireEvent.click(within(screen.getByTestId(area)).getByRole("link", { name }));
    await expectEvent(NAVIGATION, props);
  });
  it.each(FOOTER_LINKS)("tracks $source from the footer", async ({ name, ...props }) => {
    renderLanding();
    fireEvent.click(within(screen.getByRole("contentinfo")).getByRole("link", { name }));
    await expectEvent(NAVIGATION, { ...props, surface: "footer" });
    if (props.target === "example") expect(screen.getByRole("figure")).toBeInTheDocument();
  });
  it.each(SUGGESTIONS)("tracks the $target suggestion without the request", async ({ role, name, target }) => {
    renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole(role, { name }));
    await expectEvent(INTERACTION, { source: `hero_suggestion_${target}`, surface: "hero", target });
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it("tracks the install Copy button and copies the displayed command", async () => {
    const user = userEvent.setup();
    const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    renderLanding();
    await user.click(screen.getByRole("button", { name: "Copy command" }));
    expect(write).toHaveBeenCalledWith(INSTALL_COMMAND);
    await expectEvent(INTERACTION, { source: "cli_install_copy", surface: "cli_quickstart", target: "installation" });
  });
  it("tracks Join waitlist without exposing the private composer text", async () => {
    renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    expect(sdk.capture).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByTestId("hero")).getByRole("link", { name: "Join waitlist" }));
    await expectEvent(SIGNUP, { source: "composer_create_fleet", surface: "hero", mode: "humans" });
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it.each([
    { area: "audience-section", name: /^Hosted waitlist ↗$/, source: "source_early_access", surface: "source_strip" },
    { area: "pricing-block", name: /Join waitlist/, source: "pricing_early_access", surface: "pricing" },
  ])("tracks $source signup intent", async ({ area, name, ...props }) => {
    renderLanding();
    fireEvent.click(within(screen.getByTestId(area)).getByRole("link", { name }));
    await expectEvent(SIGNUP, { ...props, mode: "humans" });
  });
  it("tracks the example's Join waitlist action", async () => {
    renderLanding(EXAMPLE_PATH);
    fireEvent.click(within(screen.getByTestId("how-it-works")).getByRole("link", { name: "Join waitlist" }));
    await expectEvent(SIGNUP, { source: "example_create_fleet", surface: "example", mode: "humans" });
  });
  it("tracks exploration and returning home while preserving private text", async () => {
    renderLanding();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: "Explore an example run" }));
    await expectEvent(NAVIGATION, { source: "composer_example", surface: "hero", target: "incident" });
    sdk.capture.mockClear();
    fireEvent.click(screen.getByRole("link", { name: "Back to home" }));
    await expectEvent(NAVIGATION, { source: "example_back", surface: "example", target: "home" });
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(PRIVATE_REQUEST);
  });
  it("tracks reading the full conversation and exposes the complete report", async () => {
    vi.spyOn(window, "matchMedia").mockImplementation((media) => ({
      media, matches: false, onchange: null, addListener: vi.fn(), removeListener: vi.fn(),
      addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(() => true),
    }));
    renderLanding(EXAMPLE_PATH);
    fireEvent.click(screen.getByRole("button", { name: "Read full conversation" }));
    await expectEvent(INTERACTION, { source: "example_read_full", surface: "example", target: "incident" });
    expect(screen.getByRole("figure")).toHaveTextContent("BROWSER_TIMEOUT=10");
    expect(screen.queryByRole("button", { name: "Read full conversation" })).toBeNull();
  });
  it("tracks the example's repository link", async () => {
    renderLanding(EXAMPLE_PATH);
    fireEvent.click(screen.getByRole("link", { name: /agentsfleet\/linkwarden/ }));
    await expectEvent(NAVIGATION, { source: "example_repository", surface: "example", target: "github" });
  });
  it("keeps the starting price and included credit informational", async () => {
    renderLanding();
    fireEvent.click(screen.getByTestId("pricing-early-access-banner"));
    await flushAnalyticsForTests();
    expect(screen.getByTestId("pricing-early-access-banner")).toHaveTextContent("$0 to start · $25 credit included");
    expect(screen.getByTestId("pricing-early-access-banner")).toHaveClass("ph-no-capture");
    expect(sdk.capture).not.toHaveBeenCalled();
  });
});
