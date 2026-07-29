import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { RunnerStatus, runnerIsAwake } from "./RunnerStatus";

afterEach(() => cleanup());

describe("RunnerStatus", () => {
  it("test_runner_status_renders_admin_state_before_liveness", () => {
    render(<RunnerStatus adminState="active" liveness="busy" />);
    const status = screen.getByText(/active/i).closest("span");
    // Administrative state leads liveness in the accessible text, and the
    // treatment is the uppercase mono dot everywhere.
    expect(status?.textContent).toMatch(/active\s*·\s*busy/);
    expect(status?.className).toContain("uppercase");
  });

  it("test_runner_status_wake_ring_only_when_live", () => {
    const { container: busy } = render(<RunnerStatus adminState="active" liveness="busy" />);
    expect(busy.querySelector("[data-awake='true']")).not.toBeNull();

    const { container: cordoned } = render(<RunnerStatus adminState="cordoned" liveness="online" />);
    expect(cordoned.querySelector("[data-awake='true']")).toBeNull();

    const { container: offline } = render(<RunnerStatus adminState="active" liveness="offline" />);
    expect(offline.querySelector("[data-awake='true']")).toBeNull();
  });

  it("treats an administratively parked host as not awake regardless of liveness", () => {
    expect(runnerIsAwake("active", "busy")).toBe(true);
    expect(runnerIsAwake("active", "online")).toBe(true);
    expect(runnerIsAwake("active", "offline")).toBe(false);
    expect(runnerIsAwake("active", "registered")).toBe(false);
    expect(runnerIsAwake("cordoned", "busy")).toBe(false);
    expect(runnerIsAwake("draining", "online")).toBe(false);
    expect(runnerIsAwake("revoked", "busy")).toBe(false);
  });
});
