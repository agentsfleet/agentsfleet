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
    expect((screen.getByLabelText(/allowlist/i) as HTMLInputElement).value).toBe("pypi.org");
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe("2");

    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // PATCH replaces the WHOLE assignment, so the bind list is always sent —
    // explicitly empty here, never omitted. Omitting it is what wiped an
    // operator's mounts on any edit that did not touch them.
    await waitFor(() =>
      expect(updateRunnerPolicyActionMock).toHaveBeenCalledWith("r-edit-1", {
        ...CURRENT,
        worker_count: 4,
        extra_binds: [],
      }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("carries a stored bind through an edit that never touches it", async () => {
    const withBind = {
      ...CURRENT,
      extra_binds: [{ path: "/srv/models", mode: "read_only" as const, note: "gpu weights" }],
    };
    updateRunnerPolicyActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: "r-edit-3", admin_state: "active", assigned_policy: withBind },
    });
    render(<EditPolicyDialog runnerId="r-edit-3" current={withBind} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));

    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("/srv/models");
    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(updateRunnerPolicyActionMock).toHaveBeenCalledWith("r-edit-3", {
        ...withBind,
        worker_count: 4,
      }),
    );
  });

  // Reported from a real assignment attempt: the dialog could not be scrolled,
  // so the Save control sat below the fold and the policy could only be saved
  // by maximising the window. Assigning a policy is the one action that makes a
  // runner able to take work, so an unreachable footer blocks the whole flow.
  it("test_policy_dialog_body_scrolls: keeps the footer reachable on a short viewport", () => {
    render(<EditPolicyDialog runnerId="r-edit-2" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));

    const dialogClasses = screen.getByRole("dialog").className;
    expect(dialogClasses).toContain("max-h-svh");
    expect(dialogClasses).toContain("overflow-y-auto");
  });

  // Three isolation tiers in a two-column grid wrapped the third onto its own
  // row, reading as an afterthought rather than a peer of the other two.
  it("test_isolation_options_share_one_row: the tier options resolve to one column each", () => {
    render(<EditPolicyDialog runnerId="r-edit-5" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));

    const group = screen.getByRole("radiogroup", { name: /isolation/i });
    expect(group.className).toContain("sm:grid-cols-3");
    // Pinned against the tier list: a fourth tier would re-orphan the layout.
    expect(screen.getAllByRole("radio")).toHaveLength(3);
  });

  it("refuses a bad registry entry in-form and never calls the action", async () => {
    const onSaved = vi.fn();
    render(<EditPolicyDialog runnerId="r-edit-3" current={CURRENT} onSaved={onSaved} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    fireEvent.change(screen.getByLabelText(/allowlist/i), {
      target: { value: "http://not a host" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText(/must be a host name/i)).toBeTruthy());
    expect(updateRunnerPolicyActionMock).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("cancel closes and resets to the stored assignment; a policy-less runner resets to the defaults", async () => {
    render(<EditPolicyDialog runnerId="r-edit-4" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "9" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByLabelText("Workers")).toBeNull());
    // Re-open: the edit was discarded, the stored assignment is back.
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe("2");
    cleanup();

    // A pre-policy row (current = null) opens — and resets — at the defaults.
    render(<EditPolicyDialog runnerId="r-edit-5" current={null} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe("1");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByLabelText("Workers")).toBeNull());
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
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(updateRunnerPolicyActionMock).toHaveBeenCalled());
    // The dialog stays open with the error presented (the set happens inside
    // the transition, so the render is awaited, not assumed).
    await waitFor(() => expect(screen.getByText(/Runner not found/i)).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });
});
