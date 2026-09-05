import { FLEET_NAME, WS, ZID, ev, mockStream, onRunCompletedMock, renderThread } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { FleetThread } from "@/components/domain/FleetThread";

describe("FleetThread — role rendering: row kinds and badges", () => {
  it("does not repeat the fleet name beside a reply", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", reply: "reviewed it" }),
    ]);
    renderThread();
    expect(screen.queryByText(FLEET_NAME)).toBeNull();
    expect(screen.getByText("reviewed it")).toBeTruthy();
  });

  it("does not add a fallback sender label when no fleet name is known", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", reply: "reviewed it" }),
    ]);
    render(
      React.createElement(FleetThread, {
        workspaceId: WS,
        fleetId: ZID,
        fleetName: "",
        onRunCompleted: onRunCompletedMock,
        initial: [],
      }),
    );
    expect(screen.queryByText("Fleet")).toBeNull();
    expect(screen.getByText("reviewed it")).toBeTruthy();
  });

  it("keeps conversation rows free of sender and timestamp chrome", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", text: "reviewed it" }),
    ]);
    const { container } = renderThread();
    expect(container.querySelector("[data-chip]")).toBeNull();
    expect(container.querySelector("time")).toBeNull();
  });

  it("renders an assistant reply", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", reply: "snapshot taken." }),
    ]);
    renderThread();
    expect(screen.getByText(/snapshot taken/)).toBeTruthy();
  });

  it("renders a system meta-row with the actor as the chip label", () => {
    mockStream([
      ev({
        role: "system",
        actor: "cron",
        text: "tick · */30 * * * * · 09:30 UTC",
      }),
    ]);
    renderThread();
    expect(screen.getByText("Schedule")).toBeTruthy();
    expect(screen.getByText(/tick/)).toBeTruthy();
  });

  it("renders a continuation system row with its chip label", () => {
    mockStream([
      ev({ role: "system", actor: "continuation", text: "resumed after gate" }),
    ]);
    renderThread();
    expect(screen.getByText("Continuation")).toBeTruthy();
    expect(screen.getByText(/resumed after gate/)).toBeTruthy();
  });

  it("renders a gate_blocked system row with its chip label", () => {
    mockStream([
      ev({
        role: "system",
        actor: "gate_blocked",
        text: "blocked on approval",
      }),
    ]);
    renderThread();
    expect(screen.getByText("Approval gate")).toBeTruthy();
    expect(screen.getByText(/blocked on approval/)).toBeTruthy();
  });

  it("offers the payload disclosure for a platform identity, not only a prefixed actor", () => {
    // A GitHub App event arrives as the actor `github-app`, not `webhook:…`.
    // Gating the payload on the prefix is why those rows rendered blank.
    mockStream([
      ev({
        role: "system",
        actor: "github-app",
        text: "opened · owner/repo#7",
        custom: { requestJson: '{"repo":"owner/repo"}' },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(screen.getByText(/opened · owner\/repo#7/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"repo":\s*"owner\/repo"/)).toBeTruthy();
  });

  it("renders a webhook row with the source tag and collapsible payload", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "workflow_run · main · success",
        custom: { requestJson: '{"action":"completed"}' },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(screen.getByText(/workflow_run · main · success/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"action":\s*"completed"/)).toBeTruthy();
  });

  it("uses the canonical outcome when a webhook has no response text", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
        custom: { requestJson: "{}" },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(
      screen.getByText("This fleet needs instructions before it can respond."),
    ).toBeTruthy();
  });

  it("renders an optimistic user message with the queued badge", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:pending",
        text: "investigate the spike",
        status: "optimistic",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/investigate the spike/)).toBeTruthy();
    expect(screen.getByText(/^sending$/i)).toBeTruthy();
  });

  it("renders a failed user message with the destructive failed badge", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:pending",
        text: "this steer did not land",
        status: "failed",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/this steer did not land/)).toBeTruthy();
    expect(screen.getByText(/^not sent$/i)).toBeTruthy();
    // The in-flight annotation must not also render for a failed row.
    expect(screen.queryByText(/^sending$/i)).toBeNull();
  });

  it("renders a fleet_error as a destructive fleet reply", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reply: "Provider returned 429; retry budget exhausted",
        status: "fleet_error",
      }),
    ]);
    renderThread();
    expect(screen.queryByText("fleet_error")).toBeNull();
    expect(screen.getByText(/Provider returned 429/)).toBeTruthy();
  });
});
