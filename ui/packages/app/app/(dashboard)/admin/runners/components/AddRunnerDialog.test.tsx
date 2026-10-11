import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const createRunnerActionMock = vi.fn();
vi.mock("../actions", () => ({
  createRunnerAction: (...args: unknown[]) => createRunnerActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

import AddRunnerDialog from "./AddRunnerDialog";
import { DEFAULT_ASSIGNED_SANDBOX_TIER } from "./PolicyFields";
import { DEFAULT_ASSIGNED_NETWORK_POLICY, DEFAULT_WORKER_COUNT, NETWORK_POLICY_LABELS, SANDBOX_TIER_LABELS } from "@/lib/api/runners-types";

afterEach(() => cleanup());
beforeEach(() => {
  createRunnerActionMock.mockReset();
});

function openDialog() {
  render(<AddRunnerDialog onCreated={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: /create runner/i }));
}

describe("AddRunnerDialog assigns policy", () => {
  it("refuses a bad registry entry in-form and never calls the action", async () => {
    openDialog();
    fireEvent.change(screen.getByLabelText(/host name/i), { target: { value: "web-prod-1" } });
    fireEvent.change(screen.getByLabelText(/allowlist/i), {
      target: { value: "http://not a host" },
    });
    fireEvent.submit(screen.getByLabelText(/host name/i).closest("form") as HTMLFormElement);
    await waitFor(() => expect(screen.getByText(/must be a host name/i)).toBeTruthy());
    expect(createRunnerActionMock).not.toHaveBeenCalled();
  });

  it("test_add_runner_exposes_all_policy_fields", () => {
    // Dimension 4.4 — isolation, network policy, registry allowlist, and
    // workers, each at its documented default (network → allow_all: the
    // explicit interim posture, because allow_list_egress would degrade every
    // runner until its enforcement ships).
    openDialog();

    // All four assignment fields render.
    expect(screen.getByText("Isolation")).toBeTruthy();
    expect(screen.getByText("Network policy")).toBeTruthy();
    expect(screen.getByLabelText(/allowlist/i)).toBeTruthy();
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
    expect((screen.getByLabelText(/allowlist/i) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe(String(DEFAULT_WORKER_COUNT));
  });
});
