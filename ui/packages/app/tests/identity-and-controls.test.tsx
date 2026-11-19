import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { AgentLabel } from "@/components/domain/shared/identity/AgentLabel";
import { DELETED_AGENT_LABEL, agentDisplayName } from "@/lib/fleets/agent-label";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("AgentLabel — a fleet that no longer exists", () => {
  // Historical rows outlive their fleet: a run or an event keeps pointing at
  // an id nothing resolves. Rendering a callsign derived from a dead id would
  // invent an agent; the row says plainly that it is gone.
  it("names a deleted fleet rather than deriving a callsign for it", () => {
    expect(agentDisplayName(null)).toBe(DELETED_AGENT_LABEL);
    render(<AgentLabel fleetId={null} />);
    const label = screen.getByText(DELETED_AGENT_LABEL);
    expect(label.getAttribute("data-agent-name")).toBe(DELETED_AGENT_LABEL);
  });
});
