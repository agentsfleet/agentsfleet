import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { type RunnerLease } from "@/lib/api/runners";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import { ReviewLease } from "./ReviewLease";
import { FLEET_DETAIL, FLEET_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

// The wrapper here stands in for a real ancestor, not for a missing one:
// ReviewLease only ever mounts inside LeaseTable, which owns the provider (the
// EventsList / EventDetailsDialog arrangement). Rendering the dialog alone is
// the only reason one is needed. LeaseTable's own tests render bare, so a
// provider going missing from the table is still caught there.
const fetchMock = vi.fn();
beforeEach(() => {fetchMock.mockReset().mockImplementation(async () => response(FLEET_DETAIL)); vi.stubGlobal("fetch", fetchMock);});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

const BASE: RunnerLease = {
  id: "01J2X7NCS8T63ZP0000000000",
  fleet_id: FLEET_ID,
  fleet_name: "Search Services",
  workspace_id: WORKSPACE,
  event_id: "evt_01J2X7NBQ4M91KD",
  event_type: "index_build",
  actor: "system",
  outcome: "failed",
  failure_label: "oom_kill",
  failure_detail: "Container exceeded its 2 GiB memory limit and was terminated.",
  kind: "fresh",
  fencing_token: 1884,
  provider: "azure_openai",
  model: "gpt-4o-mini",
  posture: "metered",
  metered_input_tokens: 18204,
  metered_cached_tokens: 4096,
  metered_output_tokens: 2881,
  wall_ms: 242_000,
  lease_expires_at: new Date(Date.now() - 60_000).toISOString(),
  created_at: new Date(Date.now() - 3_600_000).toISOString(),
};

it("ReviewLease > test_review_lease_renders_lease_facts", async () => {
  render(<ReviewLease lease={BASE} onOpenChange={vi.fn()} />, { wrapper: TooltipProvider });
  expect(screen.getByText("Status")).toBeTruthy();
  expect(screen.queryByText("Outcome")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Technical details" }));
  expect(screen.getByText("New work")).toBeTruthy();
  expect(screen.getByText(BASE.id)).toBeTruthy();
  expect((await screen.findByRole("link", {name: /Open fleet/})).getAttribute("href")).toBe(`/w/${BASE.workspace_id}/fleets/${BASE.fleet_id}`);
  expect(screen.getByText("1,884")).toBeTruthy();
  expect(screen.getByText("Azure OpenAI")).toBeTruthy();
  expect(screen.getByText(BASE.model)).toBeTruthy();
  expect(screen.getByText("Metered")).toBeTruthy();
  expect(screen.getByText(/18,204 in/)).toBeTruthy();
  expect(screen.getByText(/4,096 cached/)).toBeTruthy();
  expect(screen.getByText(/2,881 out/)).toBeTruthy();
  expect(screen.getByText(/evt_01J2X7NBQ4M91KD/)).toBeTruthy();
  // The failure reads as the shared sentence, never the tag.
  expect(screen.getByText(/Ran out of memory/)).toBeTruthy();
  expect(screen.queryByText(/oom_kill/)).toBeNull();
  // The Fleet link is present while the fleet exists.
  expect(screen.getByText(/Open fleet/)).toBeTruthy();
});

it("ReviewLease > should render the failure sentence alone when the daemon recorded no detail line", () => {
  render(<ReviewLease lease={{ ...BASE, failure_detail: null }} onOpenChange={vi.fn()} />, {
    wrapper: TooltipProvider,
  });
  expect(screen.getByText(/Ran out of memory/)).toBeTruthy();
  expect(screen.queryByText(/2 GiB memory limit/)).toBeNull();
});

it("ReviewLease > test_review_lease_never_renders_request_payload", () => {
  for (const outcome of Object.values(LEASE_OUTCOME)) {
    cleanup();
    render(<ReviewLease lease={{ ...BASE, outcome }} onOpenChange={vi.fn()} />, {
      wrapper: TooltipProvider,
    });
    expect(screen.queryByText(/request_json/i)).toBeNull();
    expect(screen.queryByText(/payload/i)).toBeNull();
  }
});

it("ReviewLease > suppresses the Fleet link when the fleet is gone", async () => {
  fetchMock.mockResolvedValueOnce(response({}, 404));
  render(<ReviewLease lease={{ ...BASE, fleet_name: null }} onOpenChange={vi.fn()} />, {
    wrapper: TooltipProvider,
  });
  expect(await screen.findByText(/This fleet is no longer available/)).toBeTruthy();
  expect(screen.queryByRole("link", {name: /Open fleet/})).toBeNull();
});

it("ReviewLease > keeps an unknown billing posture visible in secondary details", () => {
  render(<ReviewLease lease={{ ...BASE, posture: "new_posture" }} onOpenChange={vi.fn()} />, {
    wrapper: TooltipProvider,
  });
  fireEvent.click(screen.getByRole("button", { name: "Technical details" }));
  expect(screen.getByText("new_posture")).toBeTruthy();
});
