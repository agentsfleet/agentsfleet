import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { resolvePeopleActionMock } = vi.hoisted(() => ({
  resolvePeopleActionMock: vi.fn(),
}));
vi.mock("@/app/actions/identity", () => ({
  resolvePeopleAction: resolvePeopleActionMock,
}));

import {
  AgentLabel,
  DELETED_AGENT_LABEL,
  agentDisplayName,
} from "@/components/domain/AgentLabel";
import { RefreshButton } from "@/components/domain/RefreshButton";
import {
  nameFor,
  requestName,
  resetPersonDirectory,
} from "@/lib/identity/person-directory";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  resolvePeopleActionMock.mockReset();
});

describe("AgentLabel — a fleet that no longer exists", () => {
  // Historical rows outlive their fleet: a lease or an event keeps pointing at
  // an id nothing resolves. Rendering a callsign derived from a dead id would
  // invent an agent; the row says plainly that it is gone.
  it("names a deleted fleet rather than deriving a callsign for it", () => {
    expect(agentDisplayName(null)).toBe(DELETED_AGENT_LABEL);
    render(<AgentLabel fleetId={null} />);
    const label = screen.getByText(DELETED_AGENT_LABEL);
    expect(label.getAttribute("data-agent-name")).toBe(DELETED_AGENT_LABEL);
  });
});

describe("RefreshButton — repeated refreshes", () => {
  // The acknowledgement is a timer. A second refresh inside its window has to
  // clear the first, or the earlier timeout fires mid-way through the new read
  // and wipes the tick off a refresh that has not finished.
  it("replaces the pending acknowledgement instead of letting it fire late", async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(<RefreshButton onRefresh={onRefresh} />);

    const button = () => screen.getByRole("button", { name: /refresh/i }) as HTMLButtonElement;
    fireEvent.click(button());
    await waitFor(() => expect(screen.getByTestId("refresh-ack").textContent).toMatch(/refreshed/i));
    await waitFor(() => expect(button().disabled).toBe(false));

    // Second click while the first acknowledgement is still standing.
    fireEvent.click(button());
    await waitFor(() => expect(onRefresh).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByTestId("refresh-ack").textContent).toMatch(/refreshed/i));
  });

  it("clears the acknowledgement once its window passes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(<RefreshButton onRefresh={onRefresh} />);

    fireEvent.click(screen.getByRole("button", { name: /refresh/i }));
    await waitFor(() => expect(screen.getByTestId("refresh-ack").textContent).toMatch(/refreshed/i));

    // The tick is proof that a read happened, not a permanent state: it stands
    // for its window and then the control is a refresh button again.
    await vi.advanceTimersByTimeAsync(2_100);
    await waitFor(() => expect(screen.getByTestId("refresh-ack").textContent).toBe(""));
  });
});

describe("person-directory — the batch that empties before it flushes", () => {
  beforeEach(() => {
    resetPersonDirectory();
    resolvePeopleActionMock.mockResolvedValue({});
  });

  it("asks nobody when the queue is cleared before the microtask runs", async () => {
    requestName("user_2abcRealPersonId");
    // The reset drains the queue that the scheduled flush was going to read —
    // a component unmounting between the render and the microtask. The flush
    // must return without a round trip rather than asking for an empty list.
    resetPersonDirectory();
    await Promise.resolve();
    await Promise.resolve();
    expect(resolvePeopleActionMock).not.toHaveBeenCalled();
  });

  it("records a subject the directory did not answer for as unknown", async () => {
    const missing = "user_2abcNobodyKnowsThis";
    resolvePeopleActionMock.mockResolvedValue({});
    requestName(missing);
    await waitFor(() => expect(resolvePeopleActionMock).toHaveBeenCalled());
    // Recorded as empty, not left open, so the same table does not ask again
    // on every render.
    await waitFor(() => expect(nameFor(missing)).toBe(""));
  });
});
