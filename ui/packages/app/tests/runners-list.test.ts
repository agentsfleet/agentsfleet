import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { RunnerListItem, RunnerListResponse } from "@/lib/api/runners";

// ── Shared mocks ───────────────────────────────────────────────────────────

const listRunnersActionMock = vi.fn();
const createRunnerActionMock = vi.fn();
const updateRunnerAdminStateActionMock = vi.fn();
const listRunnerEventsActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/runners/actions", () => ({
  listRunnersAction: listRunnersActionMock,
  createRunnerAction: createRunnerActionMock,
  updateRunnerAdminStateAction: updateRunnerAdminStateActionMock,
  listRunnerEventsAction: listRunnerEventsActionMock,
}));

// The "Add runner" trigger ships behind a next/dynamic shim (M101 §5). For the
// mint-flow interaction test (which renders RunnersView), alias the shim back
// to the real dialog so the trigger + form mount synchronously instead of
// behind the loading skeleton.
vi.mock("@/components/domain/island-dynamic/AddRunnerDialogDynamic", async () => ({
  default: (
    await vi.importActual<{ default: unknown }>(
      "@/app/(dashboard)/admin/runners/components/AddRunnerDialog",
    )
  ).default,
}));

const REGISTERED: RunnerListItem = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  host_id: "web-fresh-1",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "registered",
  labels: [],
  last_seen_at: 0,
  created_at: 1_716_000_000_000,
};
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
const BUSY: RunnerListItem = {
  id: "0190cccc-cccc-7ccc-cccc-cccccccccccc",
  host_id: "web-busy-3",
  sandbox_tier: "macos_seatbelt",
  admin_state: "draining",
  liveness: "busy",
  labels: [],
  last_seen_at: 1_716_400_000_000,
  created_at: 1_714_000_000_000,
};
const OFFLINE: RunnerListItem = {
  id: "0190dddd-dddd-7ddd-dddd-dddddddddddd",
  host_id: "web-dead-4",
  sandbox_tier: "dev_none",
  admin_state: "revoked",
  liveness: "offline",
  labels: ["legacy"],
  last_seen_at: 1_700_000_000_000,
  created_at: 1_713_000_000_000,
};

function listResponse(items: RunnerListItem[], total = items.length, page = 1): RunnerListResponse {
  return { items, total, page, page_size: 25 };
}

beforeEach(() => {
  vi.clearAllMocks();
  listRunnersActionMock.mockResolvedValue({ ok: true, data: listResponse([REGISTERED, ONLINE]) });
  updateRunnerAdminStateActionMock.mockResolvedValue({ ok: true, data: { id: REGISTERED.id, admin_state: "cordoned" } });
  listRunnerEventsActionMock.mockResolvedValue({ ok: true, data: { items: [], total: 0, page: 1, page_size: 25 } });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RunnerList component", () => {
  async function renderList(initial: RunnerListResponse) {
    const { default: RunnerList } = await import(
      "../app/(dashboard)/admin/runners/components/RunnerList"
    );
    render(React.createElement(RunnerList, { initial } as never));
  }

  it("renders the empty-state hint when no runners are enrolled", async () => {
    await renderList(listResponse([]));
    expect(screen.getByText(/No runners yet/i)).toBeTruthy();
  });

  it("renders every derived-liveness badge, admin-state badge, tier, labels, and the never-connected line", async () => {
    await renderList(listResponse([REGISTERED, ONLINE, BUSY, OFFLINE]));
    // All four derived liveness states surface as badge text.
    expect(screen.getByText("registered")).toBeTruthy();
    expect(screen.getByText("online")).toBeTruthy();
    expect(screen.getByText("busy")).toBeTruthy();
    expect(screen.getByText("offline")).toBeTruthy();
    expect(screen.getAllByText("active").length).toBeGreaterThan(0);
    expect(screen.getByText("draining")).toBeTruthy();
    expect(screen.getByText("revoked")).toBeTruthy();
    // Host ids + a tier render (isolation cell shows the friendly label).
    expect(screen.getByText("web-fresh-1")).toBeTruthy();
    expect(screen.getByText("Nested container")).toBeTruthy();
    // last_seen_at == 0 → "never connected"; > 0 → a "last seen" timestamp line.
    expect(screen.getByText(/never connected/i)).toBeTruthy();
    expect(screen.getAllByText(/last seen/i).length).toBeGreaterThan(0);
    expect(screen.getByText("gpu")).toBeTruthy();
    expect(screen.getByText("us-east")).toBeTruthy();
  });

  it("pagination shows when total exceeds the page size and Next re-fetches page 2", async () => {
    listRunnersActionMock.mockResolvedValue({ ok: true, data: listResponse([ONLINE], 30, 2) });
    const user = userEvent.setup();
    await renderList(listResponse([REGISTERED], 30));
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await waitFor(() =>
      expect(listRunnersActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 2, page_size: 25 })),
    );
  });

  it("Previous re-fetches the prior page", async () => {
    const user = userEvent.setup();
    // Render already on page 2 so Previous is enabled and can't race a
    // pending-disabled button under the slower coverage instrumentation.
    await renderList({ ...listResponse([ONLINE], 30), page: 2 });
    await user.click(screen.getByRole("button", { name: /^previous$/i }));
    await waitFor(() =>
      expect(listRunnersActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 1 })),
    );
  });

  it("resets to defaults at most once on UZ-REQ-001 (no infinite retry loop)", async () => {
    // Backend rejects every request, including the defaults the reset falls back
    // to — the `retried` guard must stop after one reset, not loop forever.
    listRunnersActionMock.mockResolvedValue({ ok: false, error: "invalid sort", errorCode: "UZ-REQ-001" });
    const user = userEvent.setup();
    await renderList(listResponse([REGISTERED], 30));
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    // Original click load + exactly one defaults-reset = 2 calls; never a third.
    await waitFor(() => expect(listRunnersActionMock.mock.calls.length).toBe(2));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(listRunnersActionMock.mock.calls.length).toBe(2);
    // The reset targeted page 1 + the default sort.
    expect(listRunnersActionMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ page: 1, sort: "-created_at" }),
    );
  });

  it("surfaces a non-validation load error inline without resetting", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([REGISTERED], 30));
    // ApiError.message is now user_message ?? detail (client.ts)
    // — UZ-INTERNAL-001's friendly copy lives in error_entries.zig, not
    // frontend CODE_MAP anymore; the mock stands in for the resolved value.
    listRunnersActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Something broke on our end. Give it another shot — if it keeps failing, send us the code below.",
      errorCode: "UZ-INTERNAL-001",
    });
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await screen.findByText(/something broke on our end/i);
    // No UZ-REQ-001 reset loop: exactly one load fired by the click.
    expect(listRunnersActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 2 }));
    expect(listRunnersActionMock.mock.calls.length).toBe(1);
  });

  it("re-fetches the first page after a runner is minted and the reveal is closed", async () => {
    const user = userEvent.setup();
    const { default: RunnersView } = await import(
      "../app/(dashboard)/admin/runners/components/RunnersView"
    );
    render(React.createElement(RunnersView, { initial: listResponse([REGISTERED]) } as never));
    createRunnerActionMock.mockResolvedValue({
      ok: true,
      data: { runner_id: "r2", runner_token: "agt_rnew" },
    });
    await user.click(screen.getByRole("button", { name: /create runner/i }));
    await user.type(screen.getByLabelText(/host name/i), "web-prod-9");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /create runner/i }));
    await screen.findByLabelText("Runner token");
    await user.click(screen.getByRole("button", { name: /stored it/i }));
    await waitFor(() =>
      expect(listRunnersActionMock).toHaveBeenCalledWith(
        expect.objectContaining({ page: 1, sort: "-created_at" }),
      ),
    );
  });

  it("never sends a page_size above the backend max (always the fixed default 25)", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([REGISTERED], 30));
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await waitFor(() => expect(listRunnersActionMock).toHaveBeenCalled());
    for (const call of listRunnersActionMock.mock.calls) {
      expect(call[0].page_size).toBeLessThanOrEqual(100);
    }
  });
});
