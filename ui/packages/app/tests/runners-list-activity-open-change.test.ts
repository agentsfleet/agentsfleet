import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerListItem, RunnerListResponse } from "@/lib/api/runners";

const PAGE_SIZE = 25;

const listRunnersActionMock = vi.fn();
const updateRunnerAdminStateActionMock = vi.fn();
const listRunnerEventsActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/runners/actions", () => ({
  listRunnersAction: listRunnersActionMock,
  createRunnerAction: vi.fn(),
  updateRunnerAdminStateAction: updateRunnerAdminStateActionMock,
  listRunnerEventsAction: listRunnerEventsActionMock,
}));

vi.mock("../app/(dashboard)/admin/runners/components/RunnerDialogs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../app/(dashboard)/admin/runners/components/RunnerDialogs")>();
  return {
    ...actual,
    RunnerActivityDialog: ({
      runner,
      onOpenChange,
    }: {
      runner: RunnerListItem;
      onOpenChange: (open: boolean) => void;
    }) =>
      React.createElement(
        "div",
        { role: "dialog", "aria-label": "Runner activity" },
        React.createElement("p", null, runner.host_id),
        React.createElement("button", { type: "button", onClick: () => onOpenChange(true) }, "Keep open"),
      ),
  };
});

const ONLINE: RunnerListItem = {
  id: "0190bbbb-bbbb-7bbb-bbbb-bbbbbbbbbbbb",
  host_id: "web-idle-2",
  sandbox_tier: "container_nested",
  admin_state: "active",
  liveness: "online",
  labels: ["gpu", "us-east"],
  last_seen_at: 1_716_500_000_000,
  created_at: 1_715_000_000_000,
};

function listResponse(items: RunnerListItem[], total = items.length, page = 1): RunnerListResponse {
  return { items, total, page, page_size: PAGE_SIZE };
}

async function renderList(initial: RunnerListResponse) {
  const { default: RunnerList } = await import(
    "../app/(dashboard)/admin/runners/components/RunnerList"
  );
  render(
    React.createElement(TooltipProvider, null, React.createElement(RunnerList, { initial } as never)),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  listRunnersActionMock.mockResolvedValue({ ok: true, data: listResponse([ONLINE]) });
  updateRunnerAdminStateActionMock.mockResolvedValue({
    ok: true,
    data: { id: ONLINE.id, admin_state: "cordoned" },
  });
  listRunnerEventsActionMock.mockResolvedValue({
    ok: true,
    data: { items: [], total: 0, page: 1, page_size: PAGE_SIZE },
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RunnerList activity open changes", () => {
  it("keeps the activity dialog open when it reports an already-open state", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByRole("dialog", { name: /runner activity/i })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /keep open/i }));
    expect(screen.getByRole("dialog", { name: /runner activity/i })).toBeTruthy();
  });
});
