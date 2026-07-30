import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { LEASE_OUTCOME, type RunnerLease } from "@/lib/api/runners";
import { ReviewLease } from "./ReviewLease";

// The wrapper here stands in for a real ancestor, not for a missing one:
// ReviewLease only ever mounts inside LeaseTable, which owns the provider (the
// EventsList / EventDetailsDialog arrangement). Rendering the dialog alone is
// the only reason one is needed. LeaseTable's own tests render bare, so a
// provider going missing from the table is still caught there.
afterEach(() => cleanup());

const BASE: RunnerLease = {
  id: "01J2X7NCS8T63ZP0000000000",
  fleet_id: "01J2WQ0000000000000000000",
  fleet_name: "Search Services",
  workspace_id: "01J2WQ1111111111111111111",
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
  lease_expires_at: Date.now() - 60_000,
  created_at: Date.now() - 3_600_000,
};

describe("ReviewLease", () => {
  it("test_review_lease_renders_lease_facts", () => {
    render(<ReviewLease lease={BASE} onOpenChange={vi.fn()} />, { wrapper: TooltipProvider });
    expect(screen.getByText("01J2X7NCS8T63ZP0000000000")).toBeTruthy();
    expect(screen.getByText("fresh")).toBeTruthy();
    expect(screen.getByText("1,884")).toBeTruthy();
    expect(screen.getByText("azure_openai")).toBeTruthy();
    expect(screen.getByText("gpt-4o-mini")).toBeTruthy();
    expect(screen.getByText("metered")).toBeTruthy();
    expect(screen.getByText(/18,204 in/)).toBeTruthy();
    expect(screen.getByText(/4,096 cached/)).toBeTruthy();
    expect(screen.getByText(/2,881 out/)).toBeTruthy();
    expect(screen.getByText(/evt_01J2X7NBQ4M91KD/)).toBeTruthy();
    // The failure reads as the shared sentence, never the tag.
    expect(screen.getByText(/Ran out of memory/)).toBeTruthy();
    expect(screen.queryByText(/oom_kill/)).toBeNull();
    // The Fleet link is present while the fleet exists.
    expect(screen.getByText(/Open Fleet/)).toBeTruthy();
  });

  it("should render the failure sentence alone when the daemon recorded no detail line", () => {
    render(<ReviewLease lease={{ ...BASE, failure_detail: null }} onOpenChange={vi.fn()} />, {
      wrapper: TooltipProvider,
    });
    expect(screen.getByText(/Ran out of memory/)).toBeTruthy();
    expect(screen.queryByText(/2 GiB memory limit/)).toBeNull();
  });

  it("test_review_lease_never_renders_request_payload", () => {
    for (const outcome of Object.values(LEASE_OUTCOME)) {
      cleanup();
      render(<ReviewLease lease={{ ...BASE, outcome }} onOpenChange={vi.fn()} />, {
        wrapper: TooltipProvider,
      });
      expect(screen.queryByText(/request_json/i)).toBeNull();
      expect(screen.queryByText(/payload/i)).toBeNull();
    }
  });

  it("suppresses the Fleet link when the fleet is gone", () => {
    render(<ReviewLease lease={{ ...BASE, fleet_name: null }} onOpenChange={vi.fn()} />, {
      wrapper: TooltipProvider,
    });
    // The id still renders so the operator can quote it; the link does not.
    expect(screen.queryByText(/Open Fleet/)).toBeNull();
  });
});
