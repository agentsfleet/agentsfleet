import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MemoryPanel from "./MemoryPanel";
import type { MemoryEntry } from "@/lib/types";
import { MEMORY_EMPTY_TITLE, MEMORY_FORGET_MISSING, OUTCOME } from "./console-copy";
import { EVENTS } from "@/lib/analytics/events";

const forgetMemoryAction = vi.fn();
const captureProductEvent = vi.fn();

vi.mock("../../actions", () => ({ forgetMemoryAction: (...a: unknown[]) => forgetMemoryAction(...a) }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => captureProductEvent(...a) }));

const ENTRY: MemoryEntry = {
  key: "convention",
  content: "Prefer tabs over spaces in generated configs",
  category: "style",
  updated_at: 1_700_000_000_000,
};

beforeEach(() => {
  forgetMemoryAction.mockReset();
  captureProductEvent.mockReset();
});
afterEach(() => cleanup());

describe("MemoryPanel", () => {
  it("test_memory_panel_lists_entries", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    // The field is `content`, not `text` — the entry body renders verbatim,
    // alongside its category and an updated_at <time>.
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
    expect(screen.getByText(ENTRY.category)).toBeTruthy();
    expect(document.body.querySelector("time")).not.toBeNull();
  });

  it("renders the empty state when the fleet has learned nothing", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} />);
    expect(screen.getByText(MEMORY_EMPTY_TITLE)).toBeTruthy();
  });

  it("forgets an entry: DELETE call, row removed, success event (no content in props)", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);

    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget" })); // dialog confirm

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
    await user.click(screen.getByRole("button", { name: "Forget" }));

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
    await user.click(screen.getByRole("button", { name: "Forget" }));

    await waitFor(() => expect(screen.getByText(/Couldn't forget this memory/)).toBeTruthy());
    expect(screen.getByText(ENTRY.content)).toBeTruthy();
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_memory_forgotten, {
      fleet_id: "agt_1",
      outcome: OUTCOME.failure,
    });
  });
});
