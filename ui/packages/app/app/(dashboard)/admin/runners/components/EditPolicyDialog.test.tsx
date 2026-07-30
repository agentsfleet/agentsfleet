import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { AssignedPolicy } from "@/lib/api/runners";

const updateRunnerPolicyActionMock = vi.fn();
vi.mock("../actions", () => ({
  updateRunnerPolicyAction: (...args: unknown[]) => updateRunnerPolicyActionMock(...args),
}));

import { EDIT_POLICY_LABEL, EditPolicyDialog } from "./EditPolicyDialog";

afterEach(() => cleanup());
beforeEach(() => {
  updateRunnerPolicyActionMock.mockReset();
});

const CURRENT: AssignedPolicy = {
  sandbox_tier: "container_nested",
  network_policy: "allow_all",
  registry_allowlist: ["pypi.org"],
  worker_count: 2,
};

describe("EditPolicyDialog", () => {
  it("pre-fills from the stored assignment and PATCHes the edited one", async () => {
    // The Indy-requested row action: reuse the four-field form, call the
    // landed PATCH — the dashboard is the fix path for a degraded runner.
    updateRunnerPolicyActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: "r-edit-1", admin_state: "active", assigned_policy: { ...CURRENT, worker_count: 4 } },
    });
    const onSaved = vi.fn();
    render(<EditPolicyDialog runnerId="r-edit-1" current={CURRENT} onSaved={onSaved} />);

    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    expect((screen.getByLabelText(/registry allowlist/i) as HTMLInputElement).value).toBe("pypi.org");
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe("2");

    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "Save assignment" }));

    await waitFor(() =>
      expect(updateRunnerPolicyActionMock).toHaveBeenCalledWith("r-edit-1", { ...CURRENT, worker_count: 4 }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("surfaces a refused re-assignment instead of closing", async () => {
    updateRunnerPolicyActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Runner not found",
      status: 404,
      errorCode: "UZ-RUN-002",
    });
    const onSaved = vi.fn();
    render(<EditPolicyDialog runnerId="r-edit-2" current={CURRENT} onSaved={onSaved} />);

    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    fireEvent.click(screen.getByRole("button", { name: "Save assignment" }));

    await waitFor(() => expect(updateRunnerPolicyActionMock).toHaveBeenCalled());
    expect(onSaved).not.toHaveBeenCalled();
    // The dialog stays open with the error presented.
    expect(screen.getByText(/Runner not found/i)).toBeTruthy();
  });
});
