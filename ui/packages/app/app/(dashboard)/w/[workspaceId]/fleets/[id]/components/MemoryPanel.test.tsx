import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MemoryPanel from "./MemoryPanel";
import type { MemoryEntry } from "@/lib/types";
import { MEMORY_EMPTY_TITLE, MEMORY_FORGET_MISSING, OUTCOME } from "./console-copy";
import { EVENTS } from "@/lib/analytics/events";

const forgetMemoryAction = vi.fn();
const setMemoryAccessAction = vi.fn();
const captureProductEvent = vi.fn();

vi.mock("../../actions", () => ({
  forgetMemoryAction: (...a: unknown[]) => forgetMemoryAction(...a),
  setMemoryAccessAction: (...a: unknown[]) => setMemoryAccessAction(...a),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => captureProductEvent(...a) }));

const ENTRY: MemoryEntry = {
  key: "convention",
  content: "Prefer tabs over spaces in generated configs",
  category: "style",
  visibility: "fleet",
  writer_fleet_id: "agt_1",
  updated_at: "2023-11-14T22:13:20Z",
};

beforeEach(() => {
  forgetMemoryAction.mockReset();
  setMemoryAccessAction.mockReset();
  captureProductEvent.mockReset();
});
afterEach(() => cleanup());

describe("MemoryPanel", () => {
  it("test_memory_panel_lists_entries", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    const owned = within(screen.getByRole("region", {name: "Saved by this fleet"}));
    expect(owned.getByText(ENTRY.content)).toBeTruthy();
    expect(owned.getByText("Private")).toBeTruthy();
    expect(document.body.querySelector("time")).not.toBeNull();
  });

  it("renders the empty state when the fleet has learned nothing", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} />);
    expect(screen.getByText(MEMORY_EMPTY_TITLE)).toBeTruthy();
  });

  it("reconciles memories delivered by a server refresh", () => {
    const view = render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} />);
    expect(screen.queryByText(ENTRY.content)).toBeNull();
    view.rerender(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
  });

  it("forgets an entry: DELETE call, row removed, success event (no content in props)", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);

    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget memory" })); // dialog confirm

    await waitFor(() => expect(forgetMemoryAction).toHaveBeenCalledWith("ws_1", "agt_1", "convention"));
    await waitFor(() => expect(screen.queryByText(ENTRY.content)).toBeNull());
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_memory_forgotten, {
      fleet_id: "agt_1",
      outcome: OUTCOME.success,
    });
    // Privacy: no key text, no content in the event props.
    const props = captureProductEvent.mock.calls[0]?.[1] ?? {};
    expect(Object.keys(props)).toEqual(["fleet_id", "outcome"]);
  });

  it("shows a newer entry when a forgotten key is learned again", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup({ delay: null });
    const view = render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget memory" }));
    await waitFor(() => expect(screen.queryByText(ENTRY.content)).toBeNull());

    view.rerender(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    expect(screen.queryByText(ENTRY.content)).toBeNull();

    const relearned = { ...ENTRY, content: "Use spaces in generated configs", updated_at: new Date(Date.parse(ENTRY.updated_at) + 1).toISOString() };
    view.rerender(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[relearned]} />);
    await waitFor(() => expect(screen.getByText(relearned.content)).toBeTruthy());
  });

  it("keeps the confirm pending until forget finishes", async () => {
    let finish!: (value: unknown) => void;
    forgetMemoryAction.mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    await user.click(screen.getByRole("button", { name: "Forget convention" }));

    const confirm = screen.getByRole("button", { name: "Forget memory" });
    await user.click(confirm);
    expect(confirm).toHaveProperty("disabled", true);
    await user.click(confirm);
    expect(forgetMemoryAction).toHaveBeenCalledTimes(1);

    finish({ ok: true, data: undefined });
    await waitFor(() => expect(screen.queryByText(ENTRY.content)).toBeNull());
  });

  it("Cancel closes the forget dialog without deleting the entry", async () => {
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);

    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(forgetMemoryAction).not.toHaveBeenCalled();
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
  });

  it("surfaces a missing key (404) and leaves the list unchanged", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: false, status: 404, error: "gone", errorCode: "UZ-MEM-004" });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);

    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget memory" }));

    await waitFor(() => expect(screen.getByText(MEMORY_FORGET_MISSING)).toBeTruthy());
    // The entry stays — a mistyped/already-gone key does not blank the list.
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_memory_forgotten, {
      fleet_id: "agt_1",
      outcome: OUTCOME.failure,
    });
  });

  it("surfaces a generic forget failure and leaves the list unchanged", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: false, status: 500, error: "storage refused", errorCode: "UZ-MEM-500" });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);

    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget memory" }));

    await waitFor(() => expect(screen.getByText(/Couldn't forget this memory/)).toBeTruthy());
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_memory_forgotten, {
      fleet_id: "agt_1",
      outcome: OUTCOME.failure,
    });
  });
});

describe("MemoryPanel shared memory", () => {
  it("explains the workspace-wide effect of forgetting a shared entry", async () => {
    const own = { ...ENTRY, visibility: "workspace" as const, writer_fleet_id: "agt_1" };
    const others = { ...ENTRY, key: "deploy_target", content: "deploy 812 broke iad", visibility: "workspace" as const, writer_fleet_id: "agt_2" };
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[own, others]} />);

    expect(within(screen.getByRole("region", {name: "Saved by this fleet"})).getByText("Shared")).toBeTruthy();
    const shared = within(screen.getByRole("region", {name: "Shared by other fleets"}));
    expect(shared.getByText(others.content)).toBeTruthy();
    expect(shared.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("button", { name: "Forget deploy_target" })).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", {name: "Forget convention"}));
    const confirmation = screen.getByRole("alertdialog", {name: "Forget this memory?"});
    expect(within(confirmation).getByText("Fleets in this workspace will lose access to this memory.")).toBeTruthy();
    expect(within(confirmation).getByText(ENTRY.content)).toBeTruthy();
  });
});
