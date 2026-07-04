import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { RunnerEventsResponse, RunnerListItem, RunnerListResponse } from "@/lib/api/runners";

const PAGE_SIZE = 25;
const PAGINATED_TOTAL = 30;

const listRunnersActionMock = vi.fn();
const updateRunnerAdminStateActionMock = vi.fn();
const listRunnerEventsActionMock = vi.fn();

type EventsActionResult =
  | { ok: true; data: RunnerEventsResponse }
  | { ok: false; error: string; errorCode: string };

vi.mock("@/app/(dashboard)/admin/runners/actions", () => ({
  listRunnersAction: listRunnersActionMock,
  createRunnerAction: vi.fn(),
  updateRunnerAdminStateAction: updateRunnerAdminStateActionMock,
  listRunnerEventsAction: listRunnerEventsActionMock,
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
const CORDONED: RunnerListItem = {
  ...ONLINE,
  id: "0190eeee-eeee-7eee-eeee-eeeeeeeeeeee",
  host_id: "web-cordoned-5",
  admin_state: "cordoned",
};
const DRAINED: RunnerListItem = {
  ...BUSY,
  id: "0190ffff-ffff-7fff-ffff-ffffffffffff",
  host_id: "web-drained-6",
  admin_state: "drained",
  liveness: "offline",
};
const OFFLINE: RunnerListItem = {
  id: "0190dddd-dddd-7ddd-dddd-dddddddddddd",
  host_id: "web-dead-4",
  sandbox_tier: "dev_none",
  admin_state: "revoked",
  liveness: "offline",
  labels: ["retired"],
  last_seen_at: 1_700_000_000_000,
  created_at: 1_713_000_000_000,
};

function listResponse(items: RunnerListItem[], total = items.length, page = 1): RunnerListResponse {
  return { items, total, page, page_size: PAGE_SIZE };
}

function deferredEvents() {
  let resolve: (value: EventsActionResult) => void = () => {};
  const promise = new Promise<EventsActionResult>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function eventResponse(
  runner: RunnerListItem,
  eventType: RunnerEventsResponse["items"][number]["event_type"],
): RunnerEventsResponse {
  return {
    items: [
      {
        id: runner.id,
        runner_id: runner.id,
        event_type: eventType,
        occurred_at: runner.last_seen_at,
        metadata: { host_id: runner.host_id },
      },
    ],
    total: 1,
    page: 1,
    page_size: PAGE_SIZE,
  };
}

function rowFor(hostId: string) {
  return screen.getByLabelText(`${hostId} runner row`);
}

async function renderList(initial: RunnerListResponse) {
  const { default: RunnerList } = await import(
    "../app/(dashboard)/admin/runners/components/RunnerList"
  );
  render(React.createElement(RunnerList, { initial } as never));
}

beforeEach(() => {
  vi.clearAllMocks();
  listRunnersActionMock.mockResolvedValue({ ok: true, data: listResponse([REGISTERED, ONLINE]) });
  updateRunnerAdminStateActionMock.mockResolvedValue({
    ok: true,
    data: { id: REGISTERED.id, admin_state: "cordoned" },
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

describe("RunnerList row actions", () => {
  it("cordons a runner from the row and updates the admin-state badge", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([REGISTERED, ONLINE]));
    await user.click(within(rowFor(REGISTERED.host_id)).getByRole("button", { name: /^cordon$/i }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^cordon$/i }));
    await waitFor(() => expect(updateRunnerAdminStateActionMock).toHaveBeenCalledWith(REGISTERED.id, "cordon"));
    expect(await screen.findByText("cordoned")).toBeTruthy();
    expect(within(rowFor(ONLINE.host_id)).getByText("active")).toBeTruthy();
    expect(listRunnerEventsActionMock).not.toHaveBeenCalled();
  });

  it("drains a cordoned runner from the row and updates the badge", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: CORDONED.id, admin_state: "draining" },
    });
    const user = userEvent.setup();
    await renderList(listResponse([CORDONED]));
    await user.click(screen.getByRole("button", { name: /^drain$/i }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^drain$/i }));
    await waitFor(() => expect(updateRunnerAdminStateActionMock).toHaveBeenCalledWith(CORDONED.id, "drain"));
    expect(await screen.findByText("draining")).toBeTruthy();
  });

  it("revokes a runner from the row and updates the badge", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: ONLINE.id, admin_state: "revoked" },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /^revoke$/i }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    await waitFor(() => expect(updateRunnerAdminStateActionMock).toHaveBeenCalledWith(ONLINE.id, "revoke"));
    expect(await screen.findByText("revoked")).toBeTruthy();
  });

  it("keeps the confirmation open and surfaces an action error when the state update fails", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: false,
      error: "missing runner",
      errorCode: "UZ-RUN-014",
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /^revoke$/i }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    expect(await screen.findByText(/couldn't revoke this runner/i)).toBeTruthy();
    expect(screen.getByRole("alertdialog")).toBeTruthy();
  });

  it("clears a failed action error when the confirmation is cancelled", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: false,
      error: "missing runner",
      errorCode: "UZ-RUN-014",
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /^revoke$/i }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.queryByText(/couldn't revoke this runner/i)).toBeNull();
  });

  it("limits row actions by admin state", async () => {
    await renderList(listResponse([REGISTERED, CORDONED, BUSY, DRAINED, OFFLINE]));
    expect(within(rowFor(REGISTERED.host_id)).getByRole("button", { name: /^cordon$/i })).toBeTruthy();
    expect(within(rowFor(REGISTERED.host_id)).getByRole("button", { name: /^drain$/i })).toBeTruthy();
    expect(within(rowFor(REGISTERED.host_id)).getByRole("button", { name: /^revoke$/i })).toBeTruthy();

    expect(within(rowFor(CORDONED.host_id)).queryByRole("button", { name: /^cordon$/i })).toBeNull();
    expect(within(rowFor(CORDONED.host_id)).getByRole("button", { name: /^drain$/i })).toBeTruthy();
    expect(within(rowFor(CORDONED.host_id)).getByRole("button", { name: /^revoke$/i })).toBeTruthy();

    expect(within(rowFor(BUSY.host_id)).queryByRole("button", { name: /^cordon$/i })).toBeNull();
    expect(within(rowFor(BUSY.host_id)).queryByRole("button", { name: /^drain$/i })).toBeNull();
    expect(within(rowFor(BUSY.host_id)).getByRole("button", { name: /^revoke$/i })).toBeTruthy();

    expect(within(rowFor(DRAINED.host_id)).queryByRole("button", { name: /^cordon$/i })).toBeNull();
    expect(within(rowFor(DRAINED.host_id)).queryByRole("button", { name: /^drain$/i })).toBeNull();
    expect(within(rowFor(DRAINED.host_id)).getByRole("button", { name: /^revoke$/i })).toBeTruthy();

    expect(within(rowFor(OFFLINE.host_id)).queryByRole("button", { name: /^cordon$/i })).toBeNull();
    expect(within(rowFor(OFFLINE.host_id)).queryByRole("button", { name: /^drain$/i })).toBeNull();
    expect(within(rowFor(OFFLINE.host_id)).queryByRole("button", { name: /^revoke$/i })).toBeNull();
  });
});

describe("RunnerList activity dialog", () => {
  it("opens runner activity and renders the event timeline", async () => {
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            id: "01902222-2222-7222-8222-222222222222",
            runner_id: ONLINE.id,
            event_type: "runner_online",
            occurred_at: 1_716_500_000_000,
            metadata: { last_seen_at: 1_716_499_000_000 },
          },
        ],
        total: 1,
        page: 1,
        page_size: PAGE_SIZE,
      },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    await waitFor(() => expect(listRunnerEventsActionMock).toHaveBeenCalledWith(ONLINE.id, { page: 1, page_size: PAGE_SIZE }));
    expect(await screen.findByText("runner_online")).toBeTruthy();
    expect(screen.getByText(/last_seen_at/i)).toBeTruthy();
  });

  it("ignores stale activity responses from a previously selected runner", async () => {
    const onlineEvents = deferredEvents();
    const busyEvents = deferredEvents();
    listRunnerEventsActionMock
      .mockReturnValueOnce(onlineEvents.promise)
      .mockReturnValueOnce(busyEvents.promise);

    const user = userEvent.setup();
    await renderList(listResponse([ONLINE, BUSY]));
    await user.click(within(rowFor(ONLINE.host_id)).getByRole("button", { name: /activity/i }));
    await waitFor(() => expect(listRunnerEventsActionMock).toHaveBeenCalledWith(ONLINE.id, { page: 1, page_size: PAGE_SIZE }));
    await user.click(screen.getByRole("button", { name: /^close$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /runner activity/i })).toBeNull());
    await user.click(within(rowFor(BUSY.host_id)).getByRole("button", { name: /activity/i }));
    await waitFor(() => expect(listRunnerEventsActionMock).toHaveBeenCalledWith(BUSY.id, { page: 1, page_size: PAGE_SIZE }));

    await act(async () => {
      busyEvents.resolve({ ok: true, data: eventResponse(BUSY, "runner_draining") });
    });
    expect(await screen.findByText("runner_draining")).toBeTruthy();

    await act(async () => {
      onlineEvents.resolve({ ok: true, data: eventResponse(ONLINE, "runner_online") });
    });
    const dialog = await screen.findByRole("dialog", { name: /runner activity/i });
    expect(screen.queryByText("runner_online")).toBeNull();
    expect(within(dialog).getByText(BUSY.host_id)).toBeTruthy();
  });

  it("opens runner activity with an empty event list", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByText(/no activity yet/i)).toBeTruthy();
  });

  it("closes runner activity from the dialog close button", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByRole("dialog", { name: /runner activity/i })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /^close$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /runner activity/i })).toBeNull());
  });

  it("surfaces runner activity load errors inside the activity dialog", async () => {
    // ApiError.message is user_message ?? detail (client.ts) —
    // UZ-INTERNAL-001's friendly copy lives in error_entries.zig now.
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Something broke on our end. Give it another shot — if it keeps failing, send us the code below.",
      errorCode: "UZ-INTERNAL-001",
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByText(/something broke on our end/i)).toBeTruthy();
  });

  it("pages runner activity without reloading the runner list", async () => {
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [],
        total: PAGINATED_TOTAL,
        page: 1,
        page_size: PAGE_SIZE,
      },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    const dialog = await screen.findByRole("dialog", { name: /runner activity/i });
    await within(dialog).findByText(/page 1 of 2/i);
    const next = within(dialog).getByRole("button", { name: /^next$/i });
    await waitFor(() => expect(next.hasAttribute("disabled")).toBe(false));
    await user.click(next);
    await waitFor(() => expect(listRunnerEventsActionMock).toHaveBeenCalledWith(ONLINE.id, { page: 2, page_size: PAGE_SIZE }));
    expect(listRunnersActionMock).not.toHaveBeenCalled();
  });

  it("pages runner activity backward from the second page", async () => {
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [],
        total: PAGINATED_TOTAL,
        page: 2,
        page_size: PAGE_SIZE,
      },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    const dialog = await screen.findByRole("dialog", { name: /runner activity/i });
    await within(dialog).findByText(/page 2 of 2/i);
    const previous = within(dialog).getByRole("button", { name: /^previous$/i });
    await waitFor(() => expect(previous.hasAttribute("disabled")).toBe(false));
    await user.click(previous);
    await waitFor(() => expect(listRunnerEventsActionMock).toHaveBeenCalledWith(ONLINE.id, { page: 1, page_size: PAGE_SIZE }));
  });

  it("renders empty metadata when event metadata cannot be serialized", async () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            id: "01901111-1111-7111-8111-111111111111",
            runner_id: ONLINE.id,
            event_type: "runner_online",
            occurred_at: 1_716_500_000_000,
            metadata: circular,
          },
        ],
        total: 1,
        page: 1,
        page_size: PAGE_SIZE,
      },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByText("{}")).toBeTruthy();
  });

  it("renders empty metadata when event metadata is null", async () => {
    listRunnerEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            id: "01903333-3333-7333-8333-333333333333",
            runner_id: ONLINE.id,
            event_type: "runner_online",
            occurred_at: 1_716_500_000_000,
            metadata: null,
          },
        ],
        total: 1,
        page: 1,
        page_size: PAGE_SIZE,
      },
    });
    const user = userEvent.setup();
    await renderList(listResponse([ONLINE]));
    await user.click(screen.getByRole("button", { name: /activity/i }));
    expect(await screen.findByText("{}")).toBeTruthy();
  });
});
