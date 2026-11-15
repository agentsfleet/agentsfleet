import type { Definition } from "@/lib/api/draft-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { SetupSummary } from "@/components/domain/setup/summary/SetupSummary";
import { REVIEW } from "@/tests/helpers/draft-fixtures";

const REQUIREMENTS = {credentials: ["github-token"], repositories: ["agentsfleet/web"], tools: ["shell"],
  network: {http_egress: "restricted", package_manager_domains: true, allow: ["api.github.com"]}, budget: {daily_nanos: "1000000000"}};
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: vi.fn()})}));
afterEach(cleanup);

it("uses the approved flat grouping and real model and spending metadata", () => {
  render(<SetupSummary definition={REVIEW.effective.definition} requirements={REQUIREMENTS} modelLabel="MiniMax M3.1 Flash" sourceLabel="Deploy check · v3" />);
  expect(screen.getByText("MiniMax M3.1 Flash")).toBeTruthy();
  expect(screen.getByText("$1.00 / day")).toBeTruthy();
  expect(screen.queryByText("Next wake-up")).toBeNull();
  expect(screen.getByText("From Deploy check · v3")).toBeTruthy();
  expect(within(screen.getByRole("region", {name: "Repositories"})).getByText("agentsfleet/web")).toBeTruthy();
  expect(screen.getByText("shell")).toBeTruthy();
  expect(within(screen.getByRole("region", {name: "Secrets"})).getByText("github-token")).toBeTruthy();
  expect(screen.queryByText("Purpose")).toBeNull();
  expect(screen.queryByText("Source and model")).toBeNull();
  expect(screen.queryByText(/Runs every/)).toBeNull();
});

it("keeps the before-setup state quiet without inventing permissions or a model", () => {
  render(<SetupSummary />);
  expect(screen.getByText("Your setup will appear here as you chat.")).toBeTruthy();
  expect(screen.queryByRole("switch")).toBeNull();
  expect(screen.queryByRole("region")).toBeNull();
});

it("distinguishes reviewed empty selections from unknown requirements", () => {
  const {rerender} = render(<SetupSummary requirements={{...REQUIREMENTS, repositories: [], credentials: [], tools: []}} />);
  expect(screen.getByText("No repositories selected")).toBeTruthy();
  expect(screen.getByText("No secrets selected")).toBeTruthy();
  rerender(<SetupSummary requirements={{}} />);
  expect(screen.getByText("Network policy not reviewed")).toBeTruthy();
  expect(screen.queryByText("No secrets selected")).toBeNull();
  expect(screen.queryByRole("switch")).toBeNull();
});

it("changes only the chosen policy and disables package domains while Allow all is on", () => {
  const change = vi.fn();
  const {rerender} = render(<SetupSummary requirements={REQUIREMENTS} onNetworkChange={change} />);
  fireEvent.click(screen.getByRole("switch", {name: "Allow all public internet access"}));
  expect(change).toHaveBeenLastCalledWith({http_egress: "public"});
  rerender(<SetupSummary requirements={{...REQUIREMENTS, network: {...REQUIREMENTS.network, http_egress: "public"}}} onNetworkChange={change} />);
  const packages = screen.getByRole("switch", {name: "Enable package manager domains"});
  expect(packages.getAttribute("data-state")).toBe("checked");
  expect(packages).toHaveProperty("disabled", true);
  fireEvent.click(packages);
  expect(change).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("switch", {name: "Allow all public internet access"}));
  expect(change).toHaveBeenLastCalledWith({http_egress: "restricted"});
  rerender(<SetupSummary requirements={REQUIREMENTS} onNetworkChange={change} />);
  fireEvent.click(screen.getByRole("switch", {name: "Enable package manager domains"}));
  expect(change).toHaveBeenLastCalledWith({package_manager_domains: false});
});

it("keeps controls read-only without an edit owner and while a request is pending", () => {
  const {rerender} = render(<SetupSummary requirements={REQUIREMENTS} />);
  for (const control of screen.getAllByRole("switch")) expect(control).toHaveProperty("disabled", true);
  rerender(<SetupSummary requirements={REQUIREMENTS} onNetworkChange={vi.fn()} onManage={vi.fn()} disabled />);
  for (const control of [...screen.getAllByRole("switch"), ...screen.getAllByRole("button", {name: /Manage/})]) expect(control).toHaveProperty("disabled", true);
});

it("keeps next wake-up in the composer and can show an incomplete saved setup", () => {
  render(<SetupSummary ready />);
  expect(screen.queryByText("Next wake-up")).toBeNull();
  expect(screen.getByText("Network policy not reviewed")).toBeTruthy();
});

it("retains exact nanos above Number.MAX_SAFE_INTEGER", () => {
  render(<SetupSummary requirements={{...REQUIREMENTS, budget: {daily_nanos: "1", monthly_nanos: "9223372036854775807"}}} />);
  expect(screen.getByText("$0.000000001 / day")).toBeTruthy();
  expect(screen.getByText("$9,223,372,036.854775807 / month")).toBeTruthy();
});

it.each([{}, {...REQUIREMENTS, credentials: null}, {...REQUIREMENTS, budget: {daily_nanos: "0"}},
  {...REQUIREMENTS, budget: {daily_nanos: "-1"}}, {...REQUIREMENTS, budget: {daily_nanos: "01"}},
  {...REQUIREMENTS, network: {http_egress: "unknown"}}, {...REQUIREMENTS, network: {package_manager_domains: "true"}},
])("keeps malformed review requirements unknown instead of falling back to saved defaults: %s", requirements => {
  const definition: Definition = {...REVIEW.effective.definition, config: {"x-agentsfleet": REQUIREMENTS}};
  render(<SetupSummary definition={definition} requirements={requirements} />);
  expect(screen.getByText("Network policy not reviewed")).toBeTruthy();
  expect(screen.queryByText("$1.00 / day")).toBeNull();
});

it.each([{allow: []}, {package_manager_domains: false}, {}])("does not turn an incomplete network policy into public access: %j", network => {
  render(<SetupSummary requirements={{...REQUIREMENTS, network}} />);
  expect(screen.queryByRole("switch", {name: "Allow all public internet access"})?.getAttribute("aria-checked") ?? "false").toBe("false");
});
