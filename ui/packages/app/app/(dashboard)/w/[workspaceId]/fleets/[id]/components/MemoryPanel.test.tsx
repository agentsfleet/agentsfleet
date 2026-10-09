import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MemoryPanel, {
  MEMORY_ACCESS_PUBLISH_DESCRIPTION,
  MEMORY_ACCESS_PUBLISH_LABEL,
  MEMORY_ACCESS_READ_DESCRIPTION,
  MEMORY_ACCESS_READ_LABEL,
  MEMORY_SHARED_LABEL,
  MEMORY_SHARED_TITLE,
} from "./MemoryPanel";
import type { MemoryEntry } from "@/lib/types";
import { MEMORY_EMPTY_TITLE, MEMORY_FETCH_UNAVAILABLE, MEMORY_FORGET_MISSING, OUTCOME } from "./console-copy";
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
  updated_at: 1_700_000_000_000,
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

  it("shows an unavailable state instead of claiming the fleet learned nothing", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={null} />);
    expect(screen.getByText(MEMORY_FETCH_UNAVAILABLE)).toBeTruthy();
    expect(screen.queryByText(MEMORY_EMPTY_TITLE)).toBeNull();
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

  it("shows a newer entry when a forgotten key is learned again", async () => {
    forgetMemoryAction.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup({ delay: null });
    const view = render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    await user.click(screen.getByRole("button", { name: "Forget convention" }));
    await user.click(screen.getByRole("button", { name: "Forget" }));
    await waitFor(() => expect(screen.queryByText(ENTRY.content)).toBeNull());

    view.rerender(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    expect(screen.queryByText(ENTRY.content)).toBeNull();

    const relearned = { ...ENTRY, content: "Use spaces in generated configs", updated_at: ENTRY.updated_at + 1 };
    view.rerender(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[relearned]} />);
    await waitFor(() => expect(screen.getByText(relearned.content)).toBeTruthy());
  });

  it("keeps the confirm pending until forget finishes", async () => {
    let finish!: (value: unknown) => void;
    forgetMemoryAction.mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[ENTRY]} />);
    await user.click(screen.getByRole("button", { name: "Forget convention" }));

    const confirm = screen.getByRole("button", { name: "Forget" });
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

describe("MemoryPanel shared memory", () => {
  const CLOSED = { read: false, publish: false };
  const describedBy = (el: HTMLElement) => document.getElementById(el.getAttribute("aria-describedby") ?? "")?.textContent;

  it("each shared-memory grant is a named switch with what it does", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} access={{ read: true, publish: false }} canGrant />);

    const group = screen.getByRole("group", { name: MEMORY_SHARED_TITLE });
    const read = within(group).getByRole("switch", { name: MEMORY_ACCESS_READ_LABEL });
    const publish = within(group).getByRole("switch", { name: MEMORY_ACCESS_PUBLISH_LABEL });
    expect(read.getAttribute("aria-checked")).toBe("true");
    expect(publish.getAttribute("aria-checked")).toBe("false");
    expect(describedBy(read)).toBe(MEMORY_ACCESS_READ_DESCRIPTION);
    expect(describedBy(publish)).toBe(MEMORY_ACCESS_PUBLISH_DESCRIPTION);
  });

  it("flipping a grant sends only that grant and shows the route's answer", async () => {
    setMemoryAccessAction.mockResolvedValue({ ok: true, data: { read: false, publish: true } });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} access={CLOSED} canGrant />);

    const publish = screen.getByRole("switch", { name: MEMORY_ACCESS_PUBLISH_LABEL });
    await user.click(publish);

    expect(setMemoryAccessAction).toHaveBeenCalledWith("ws_1", "agt_1", { publish: true });
    await waitFor(() => expect(publish.getAttribute("aria-checked")).toBe("true"));
    expect(screen.getByRole("switch", { name: MEMORY_ACCESS_READ_LABEL }).getAttribute("aria-checked")).toBe("false");
  });

  it("a refused shared-memory change leaves the switch where it was", async () => {
    setMemoryAccessAction.mockResolvedValue({ ok: false, status: 403, error: "scope", errorCode: "UZ-AUTH-022" });
    const user = userEvent.setup({ delay: null });
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} access={CLOSED} canGrant />);

    const read = screen.getByRole("switch", { name: MEMORY_ACCESS_READ_LABEL });
    await user.click(read);

    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
    expect(read.getAttribute("aria-checked")).toBe("false");
    expect((read as HTMLButtonElement).disabled).toBe(false);
  });

  it("shows no switches to a viewer without fleet:write", () => {
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[]} access={CLOSED} canGrant={false} />);
    expect(screen.queryByRole("switch")).toBeNull();
  });

  it("marks a shared entry, and offers no forget on another fleet's", () => {
    const own = { ...ENTRY, visibility: "workspace" as const, writer_fleet_id: "agt_1" };
    const others = { ...ENTRY, key: "deploy_target", content: "deploy 812 broke iad", visibility: "workspace" as const, writer_fleet_id: "agt_2" };
    render(<MemoryPanel workspaceId="ws_1" fleetId="agt_1" entries={[own, others]} />);

    expect(screen.getAllByText(MEMORY_SHARED_LABEL)).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Forget convention" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Forget deploy_target" })).toBeNull();
  });
});
