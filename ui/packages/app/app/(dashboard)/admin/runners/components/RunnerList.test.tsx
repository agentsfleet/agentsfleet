import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerListItem, RunnerListResponse } from "@/lib/api/runners";

// Real design-system (IconAction + Time render Radix Tooltips), so a
// TooltipProvider ancestor is mandatory — the dashboard layout mounts one in
// production; tests must supply their own or the render throws.
vi.mock("@/app/(dashboard)/admin/runners/actions", () => ({
  listRunnersAction: vi.fn(),
  createRunnerAction: vi.fn(),
  updateRunnerAdminStateAction: vi.fn(),
  listRunnerEventsAction: vi.fn(),
}));

const ACTIVE: RunnerListItem = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  host_id: "web-active-1",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "online",
  labels: [],
  last_seen_at: 1_716_500_000_000,
  created_at: 1_716_000_000_000,
};

function listResponse(items: RunnerListItem[], total = items.length, page = 1): RunnerListResponse {
  return { items, total, page, page_size: 25 };
}

async function renderList(initial: RunnerListResponse) {
  const { default: RunnerList } = await import("./RunnerList");
  render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(RunnerList, { initial } as never),
    ),
  );
}

function rowFor(hostId: string): HTMLElement {
  const row = screen.getByText(hostId).closest("tr");
  if (!row) throw new Error(`no runner row for ${hostId}`);
  return row as HTMLElement;
}

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RunnerList icon-action row + relative host timestamps", () => {
  it("test_runner_row_actions_have_accessible_names", async () => {
    await renderList(listResponse([ACTIVE]));
    // active state → Activity + Cordon + Drain + Revoke, all icon-only.
    //
    // The host-id cell also carries a CopyButton. It is an affordance on a value,
    // not an action on the runner, so it is excluded by its `data-slot` rather
    // than folded into the action set — the point of this test is that the set of
    // things you can DO to a runner is exactly those four, each with a name.
    const buttons = within(rowFor(ACTIVE.host_id))
      .getAllByRole("button")
      .filter((b) => b.getAttribute("data-slot") !== "copy-button");
    expect(buttons.length).toBe(4);
    const names = buttons.map((b) => b.getAttribute("aria-label") ?? "");
    for (const name of names) expect(name.length).toBeGreaterThan(0);
    expect(new Set(names)).toEqual(new Set(["Activity", "Cordon", "Drain", "Revoke"]));
  });

  it("test_revoke_destructive_intent", async () => {
    await renderList(listResponse([ACTIVE]));
    const revoke = within(rowFor(ACTIVE.host_id)).getByRole("button", { name: /^revoke$/i });
    expect(revoke.className).toContain("bg-destructive");
  });

  it("test_hostcell_uses_time_no_fmt", async () => {
    await renderList(listResponse([ACTIVE]));
    // enrolled + last-seen both render <time> elements (Time format="relative").
    const times = rowFor(ACTIVE.host_id).querySelectorAll("time");
    expect(times.length).toBe(2);
    for (const t of times) expect(t.getAttribute("datetime")).toBeTruthy();

    const source = readFileSync(resolve(__dirname, "RunnerList.tsx"), "utf8");
    expect(source).not.toMatch(/function fmt\(/);
  });

  it("test_activityrow_uses_time", async () => {
    // The hand-rolled <time> + bare toLocaleString() is gone from RunnerDialogs.
    const source = readFileSync(resolve(__dirname, "RunnerDialogs.tsx"), "utf8");
    expect(source).not.toMatch(/toLocaleString/);
  });
});
