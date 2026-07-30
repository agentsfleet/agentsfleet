import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const createRunnerActionMock = vi.fn();
vi.mock("../actions", () => ({
  createRunnerAction: (...args: unknown[]) => createRunnerActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

import AddRunnerDialog from "./AddRunnerDialog";
import { DEFAULT_ASSIGNED_SANDBOX_TIER } from "./PolicyFields";
import {
  DEFAULT_ASSIGNED_NETWORK_POLICY,
  DEFAULT_WORKER_COUNT,
  NETWORK_POLICY_LABELS,
  SANDBOX_TIER_LABELS,
} from "@/lib/api/runners";

afterEach(() => cleanup());
beforeEach(() => {
  createRunnerActionMock.mockReset();
});

function openDialog() {
  render(<AddRunnerDialog onCreated={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: /create runner/i }));
}

describe("AddRunnerDialog assigns policy", () => {
  it("test_add_runner_copy_describes_an_assignment", () => {
    // Dimension 4.3 — the selection is an assignment the host must satisfy,
    // never a description of the host.
    openDialog();
    expect(screen.getByText(/assigned to the host/i)).toBeTruthy();
    expect(screen.getByText(/the isolation this host must enforce/i)).toBeTruthy();
    // The pre-inversion framing is gone: nothing calls the tier self-reported.
    expect(screen.queryByText(/self-reported/i)).toBeNull();
  });

  it("test_add_runner_exposes_all_policy_fields", () => {
    // Dimension 4.4 — isolation, network policy, registry allowlist, and
    // workers, each at its documented default (network → allow_all: the
    // explicit interim posture, because allow_list_egress would degrade every
    // runner until its enforcement ships).
    openDialog();

    // All four assignment fields render.
    expect(screen.getByText("Isolation to assign")).toBeTruthy();
    expect(screen.getByText("Network policy")).toBeTruthy();
    expect(screen.getByLabelText(/registry allowlist/i)).toBeTruthy();
    expect(screen.getByLabelText("Workers")).toBeTruthy();

    // Isolation defaults to the strongest tier.
    const defaultTier = screen
      .getAllByRole("radio")
      .find((r) => r.textContent?.includes(SANDBOX_TIER_LABELS[DEFAULT_ASSIGNED_SANDBOX_TIER]));
    expect(defaultTier?.getAttribute("aria-checked")).toBe("true");

    // Network defaults to the open interim posture, rendered by its label
    // (twice: the visible trigger value + the hidden native option).
    expect(screen.getAllByText(NETWORK_POLICY_LABELS[DEFAULT_ASSIGNED_NETWORK_POLICY]).length).toBeGreaterThan(0);

    // Registry starts empty; workers start at the shared default.
    expect((screen.getByLabelText(/registry allowlist/i) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe(String(DEFAULT_WORKER_COUNT));
  });
});
