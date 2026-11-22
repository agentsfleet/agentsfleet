import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { REVIEW, SCHEDULE } from "@/tests/helpers/draft-fixtures";
import { SetupSummary } from "./SetupSummary";

afterEach(cleanup);

it.each([false, true])("keeps saved network access in setup and disables package domains when public access is on (%s)", publicInternet => {
  const editHref = "/w/workspace/fleets/fleet/setup";
  const network = {http_egress: publicInternet ? "public" : "restricted", package_manager_domains: true};
  render(<SetupSummary editHref={editHref} ready requirements={{...REVIEW.requirements, network}} />, {wrapper: TooltipProvider});
  const publicLink = screen.getByRole("link", {name: `Allow all public internet access (${publicInternet ? "on" : "off"}). Edit setup`});
  expect(publicLink.getAttribute("href")).toBe(editHref);
  expect(screen.getByRole("link", {name: "Manage secrets"}).getAttribute("href")).toBe(editHref + "#secrets");
  if (publicInternet) {
    const packages = screen.getByRole("switch", {name: "Enable package manager domains"});
    expect(packages.getAttribute("aria-checked")).toBe("true");
    expect(packages).toHaveProperty("disabled", true);
  } else {
    expect(screen.getByRole("link", {name: "Enable package manager domains (on). Edit setup"})).toBeTruthy();
  }
});

it("shows the earliest scheduled wake-up and describes a paused or proposed schedule", () => {
  const date = (days: number) => new Date(Date.now() + days * 24 * 60 * 60_000).toISOString();
  const early = {...SCHEDULE, next_run_at: date(1)};
  const late = {...SCHEDULE, next_run_at: date(2)};
  const current = render(<SetupSummary ready schedules={[late, early, late]} />, {wrapper: TooltipProvider});
  expect(current.container.querySelector("time")?.getAttribute("datetime")).toBe(early.next_run_at);
  const today = {...SCHEDULE, next_run_at: new Date().toISOString()};
  current.rerender(<SetupSummary ready schedules={[today]} />);
  expect(current.container.querySelector("time")?.getAttribute("datetime")).toBe(today.next_run_at);
  current.rerender(<SetupSummary ready schedules={[{...early, status: "paused"}]} />);
  expect(screen.getByText("Paused")).toBeTruthy();
  const definition = {...REVIEW.effective.definition, config: {name: "repairer", "x-agentsfleet": {
    tools: [],
    budget: REVIEW.requirements.budget,
    triggers: [{type: "cron", schedule: SCHEDULE.cron, timezone: SCHEDULE.timezone}],
  }}};
  current.rerender(<SetupSummary definition={definition} ready />);
  expect(screen.getByText("Pending save")).toBeTruthy();
  current.rerender(<SetupSummary definition={definition} ready schedules={[{...SCHEDULE, status: "removed"}]} />);
  expect(screen.getByText("Not scheduled")).toBeTruthy();
});
