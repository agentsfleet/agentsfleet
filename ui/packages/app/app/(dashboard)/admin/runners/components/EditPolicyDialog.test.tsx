import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AssignedPolicy } from "@/lib/api/runners-types";

const updateRunnerActionMock = vi.fn();
vi.mock("../actions", () => ({
  updateRunnerAction: (...args: unknown[]) => updateRunnerActionMock(...args),
}));

import { EditPolicyDialog } from "./EditPolicyDialog";
import { EDIT_POLICY_LABEL } from "./RunnerListCells";

afterEach(() => cleanup());
beforeEach(() => {
  updateRunnerActionMock.mockReset();
});

const CURRENT: AssignedPolicy = {
  sandbox_tier: "container_nested",
  network_policy: "allow_all",
  registry_allowlist: ["pypi.org"],
  worker_count: 2,
  extra_binds: [],
};

describe("EditPolicyDialog", () => {
  it("keeps a pending assignment open and preserves edits after an unexpected failure", async () => {
    const pending = Promise.withResolvers<unknown>();
    updateRunnerActionMock.mockReturnValue(pending.promise);
    render(<EditPolicyDialog runnerId="r-edit-1" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
    fireEvent.change(screen.getByLabelText("Workers"), {target: {value: "4"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(screen.getByRole("button", {name: "Cancel"})).toHaveProperty("disabled", true));
    await userEvent.setup().keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeTruthy();
    await act(async () => pending.reject(new Error("private-response-marker")));
    expect(screen.getByRole("alert").textContent).toContain("Try again");
    expect(screen.getByRole("alert").textContent).not.toContain("private-response-marker");
    expect(screen.getByLabelText("Workers")).toHaveProperty("value", "4");
  });
  it("opens the latest assignment while preserving an edit already in progress", async () => {
    const onSaved = vi.fn();
    const view = render(<EditPolicyDialog runnerId="r-edit-1" current={CURRENT} onSaved={onSaved} />);
    view.rerender(<EditPolicyDialog runnerId="r-edit-1" current={{...CURRENT, worker_count: 4}} onSaved={onSaved} />);
    fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
    expect(screen.getByLabelText("Workers")).toHaveProperty("value", "4");
    fireEvent.change(screen.getByLabelText("Workers"), {target: {value: "7"}});
    view.rerender(<EditPolicyDialog runnerId="r-edit-1" current={{...CURRENT, worker_count: 6}} onSaved={onSaved} />);
    expect(screen.getByLabelText("Workers")).toHaveProperty("value", "7");
    fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
    expect(screen.getByLabelText("Workers")).toHaveProperty("value", "6");
  });
  // Every field here is an assignment with a default and a range, and getting one
  // wrong degrades the host silently from the operator's point of view. The form
  // cannot hold that much explanation, so it points at the page that can.
  it("points at the runner policy page", () => {
    render(<EditPolicyDialog runnerId="r-edit-1" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));

    expect(screen.getByRole("link", { name: /^learn more/i }).getAttribute("href")).toBe(
      "https://docs.agentsfleet.net/runners",
    );
  });

  it("leads its trigger with an icon beside the label", () => {
    // The header's icon-presence test covers its own buttons; this trigger
    // mounts there as a stub, so its glyph is pinned here instead.
    render(<EditPolicyDialog runnerId="r-edit-icon" current={CURRENT} onSaved={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: EDIT_POLICY_LABEL });
    expect(trigger.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
  });

  it("pre-fills from the stored assignment and PATCHes the edited one", async () => {
    // The Indy-requested row action: reuse the four-field form, call the
    // landed PATCH — the dashboard is the fix path for a degraded runner.
    updateRunnerActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: "r-edit-1", status: "active", assigned_policy: { ...CURRENT, worker_count: 4 } },
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
      expect(updateRunnerActionMock).toHaveBeenCalledWith("r-edit-1", {
        assigned_policy: {...CURRENT, worker_count: 4, extra_binds: []},
      }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("carries a stored bind through an edit that never touches it", async () => {
    const withBind = {
      ...CURRENT,
      extra_binds: [{ path: "/srv/models", mode: "read_only" as const, note: "gpu weights" }],
    };
    updateRunnerActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: "r-edit-3", status: "active", assigned_policy: withBind },
    });
    render(<EditPolicyDialog runnerId="r-edit-3" current={withBind} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));

    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("/srv/models");
    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(updateRunnerActionMock).toHaveBeenCalledWith("r-edit-3", {
        assigned_policy: {...withBind, worker_count: 4},
      }),
    );
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
    expect(updateRunnerActionMock).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("cancel closes and resets to the stored assignment", async () => {
    render(<EditPolicyDialog runnerId="r-edit-4" current={CURRENT} onSaved={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    fireEvent.change(screen.getByLabelText("Workers"), { target: { value: "9" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByLabelText("Workers")).toBeNull());
    // Re-open: the edit was discarded, the stored assignment is back.
    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe("2");
  });

  it("surfaces a refused re-assignment instead of closing", async () => {
    updateRunnerActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Runner not found",
      status: 404,
      errorCode: "UZ-RUN-002",
    });
    const onSaved = vi.fn();
    render(<EditPolicyDialog runnerId="r-edit-2" current={CURRENT} onSaved={onSaved} />);

    fireEvent.click(screen.getByRole("button", { name: EDIT_POLICY_LABEL }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(updateRunnerActionMock).toHaveBeenCalled());
    // The dialog stays open with the error presented (the set happens inside
    // the transition, so the render is awaited, not assumed).
    await waitFor(() => expect(screen.getByText(/Runner not found/i)).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });
});

const LABELS = "Labels";
it.each([{currentLabels: ["gpu,linux"]}, {currentLabels: ["東京", " has spaces "]}, {currentLabels: [""]}, {currentLabels: ["gpu"]}])("keeps unchanged stored labels $currentLabels out of a policy-only save", async ({currentLabels}) => {
  updateRunnerActionMock.mockResolvedValue({ok: true});
  const saved = vi.fn();
  render(<EditPolicyDialog runnerId="preserve" current={CURRENT} currentLabels={currentLabels} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  fireEvent.change(screen.getByLabelText("Workers"), {target: {value: "4"}});
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  expect(updateRunnerActionMock).toHaveBeenCalledExactlyOnceWith("preserve", {assigned_policy: {...CURRENT, worker_count: 4, extra_binds: []}});
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("does not replace refreshed labels when a policy edit is already open", async () => {
  updateRunnerActionMock.mockResolvedValue({ok: true});
  const saved = vi.fn();
  const view = render(<EditPolicyDialog runnerId="preserve" current={CURRENT} currentLabels={["gpu"]} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  view.rerender(<EditPolicyDialog runnerId="preserve" current={CURRENT} currentLabels={["arm"]} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  expect(updateRunnerActionMock).toHaveBeenCalledExactlyOnceWith("preserve", {assigned_policy: {...CURRENT, extra_binds: []}});
});
it("prefills labels and clears the whole set in one save", async () => {
  updateRunnerActionMock.mockResolvedValue({ok: true, data: {labels: []}});
  const saved = vi.fn();
  render(<EditPolicyDialog runnerId="labels" current={CURRENT} currentLabels={["gpu", "us-east"]} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  expect(screen.getByLabelText(LABELS)).toHaveProperty("value", "gpu, us-east");
  expect(screen.getByText(/clearing removes every label/i)).toBeTruthy();
  fireEvent.change(screen.getByLabelText(LABELS), {target: {value: ""}});
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  expect(updateRunnerActionMock).toHaveBeenCalledExactlyOnceWith("labels", {
    labels: [],
  });
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("saves changed labels and policy together in one edit", async () => {
  updateRunnerActionMock.mockResolvedValue({ok: true});
  const saved = vi.fn();
  render(<EditPolicyDialog runnerId="both" current={CURRENT} currentLabels={["gpu"]} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  fireEvent.change(screen.getByLabelText(LABELS), {target: {value: "arm, linux, arm"}});
  fireEvent.change(screen.getByLabelText("Workers"), {target: {value: "4"}});
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  expect(updateRunnerActionMock).toHaveBeenCalledExactlyOnceWith("both", {assigned_policy: {...CURRENT, worker_count: 4, extra_binds: []}, labels: ["arm", "linux"]});
});
it.each(["bad label!", "x".repeat(65), Array.from({length: 33}, (_, n) => `label-${n}`).join(",")])("rejects invalid labels %s without saving", async (value) => {
  const saved = vi.fn();
  render(<EditPolicyDialog runnerId="labels" current={CURRENT} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  fireEvent.change(screen.getByLabelText(LABELS), {target: {value}});
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", expect.stringMatching(/must be|at most 32/i));
  expect(screen.getByLabelText(LABELS)).toHaveProperty("value", value);
  expect(updateRunnerActionMock).not.toHaveBeenCalled();
  expect(saved).not.toHaveBeenCalled();
});
it("preserves labels and shows the server's 400 sentence", async () => {
  const sentence = "Runner labels must contain at most 32 labels of at most 64 bytes each";
  updateRunnerActionMock.mockResolvedValueOnce({ok: false, status: 400, error: sentence});
  const saved = vi.fn();
  render(<EditPolicyDialog runnerId="labels" current={CURRENT} currentLabels={["gpu"]} onSaved={saved} />);
  fireEvent.click(screen.getByRole("button", {name: EDIT_POLICY_LABEL}));
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", expect.stringContaining(sentence));
  expect(screen.getByLabelText(LABELS)).toHaveProperty("value", "gpu");
  expect(saved).not.toHaveBeenCalled();
});
