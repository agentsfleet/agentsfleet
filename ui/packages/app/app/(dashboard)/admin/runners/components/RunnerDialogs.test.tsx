import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerListItem, RunnerEventsResponse } from "@/lib/api/runners";
import { RunnerActivityDialog } from "./RunnerDialogs";

const RUNNER: RunnerListItem = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  host_id: "web-active-1",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "online",
  labels: [],
  last_seen_at: 1_716_500_000_000,
  created_at: 1_716_000_000_000,
};

const NO_ACTIVITY = "No activity yet.";
// Skeleton's only stable render signature (design-system Skeleton.tsx).
const SKELETON_SELECTOR = ".animate-pulse";

function eventsResponse(items: RunnerEventsResponse["items"]): RunnerEventsResponse {
  return { items, total: items.length, page: 1, page_size: 25 };
}

function renderDialog(props: { data: RunnerEventsResponse | null; error?: string | null; pending?: boolean }) {
  render(
    <TooltipProvider>
      <RunnerActivityDialog
        runner={RUNNER}
        data={props.data}
        error={props.error ?? null}
        pending={props.pending ?? false}
        onOpenChange={() => {}}
        onPage={() => {}}
      />
    </TooltipProvider>,
  );
}

afterEach(cleanup);

describe("RunnerActivityDialog body states", () => {
  it("test_shows_skeleton_while_events_load", () => {
    renderDialog({ data: null, pending: true });
    expect(document.querySelectorAll(SKELETON_SELECTOR).length).toBeGreaterThan(0);
    expect(screen.queryByText(NO_ACTIVITY)).toBeNull();
  });

  it("test_empty_page_shows_no_activity_not_skeleton", () => {
    renderDialog({ data: eventsResponse([]) });
    expect(screen.getByText(NO_ACTIVITY)).toBeTruthy();
    expect(document.querySelectorAll(SKELETON_SELECTOR).length).toBe(0);
  });

  it("test_loaded_events_render_without_skeleton", () => {
    renderDialog({
      data: eventsResponse([
        {
          id: "0190bbbb-bbbb-7bbb-bbbb-bbbbbbbbbbbb",
          runner_id: RUNNER.id,
          event_type: "runner_registered",
          occurred_at: 1_716_000_000_000,
          metadata: { host_id: RUNNER.host_id },
        },
      ]),
    });
    expect(screen.getByText("runner_registered")).toBeTruthy();
    expect(document.querySelectorAll(SKELETON_SELECTOR).length).toBe(0);
  });

  it("test_error_replaces_skeleton", () => {
    renderDialog({ data: null, error: "could not load runner activity" });
    expect(screen.getByText("could not load runner activity")).toBeTruthy();
    expect(document.querySelectorAll(SKELETON_SELECTOR).length).toBe(0);
  });
});
