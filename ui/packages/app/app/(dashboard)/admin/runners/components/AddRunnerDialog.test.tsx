import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { JEV_MODEL } from "@/tests/helpers/catalogue-fixtures";

const createRunnerActionMock = vi.fn();
const {catalogue} = vi.hoisted(() => ({catalogue: vi.fn()}));
vi.mock("../actions", () => ({
  createRunnerAction: (...args: unknown[]) => createRunnerActionMock(...args),
}));
vi.mock("../../models/actions", () => ({listAdminModelsAction: catalogue}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

import AddRunnerDialog from "./AddRunnerDialog";
import { DEFAULT_ASSIGNED_SANDBOX_TIER } from "./PolicyFields";
import { DEFAULT_ASSIGNED_NETWORK_POLICY, DEFAULT_WORKER_COUNT, NETWORK_POLICY_LABELS, SANDBOX_TIER_LABELS } from "@/lib/api/runners-types";

afterEach(() => cleanup());
beforeEach(() => {
  createRunnerActionMock.mockReset();
  catalogue.mockReset().mockResolvedValue({ok: true, data: {models: [JEV_MODEL]}});
});

async function openDialog() {
  render(<AddRunnerDialog onCreated={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: /create runner/i }));
  await waitFor(() => expect(screen.getByRole("button", {name: "Create"})).toHaveProperty("disabled", false));
}

describe("AddRunnerDialog assigns policy", () => {
  it("retains an in-flight enrollment and shows an unexpected failure inside the dialog", async () => {
    const pending = Promise.withResolvers<unknown>();
    createRunnerActionMock.mockReturnValue(pending.promise);
    await openDialog();
    fireEvent.change(screen.getByLabelText(/host name/i), {target: {value: "web-prod-1"}});
    fireEvent.click(screen.getByRole("button", {name: "Create"}));
    await waitFor(() => expect(screen.getByRole("button", {name: "Cancel"})).toHaveProperty("disabled", true));
    const form = screen.getByLabelText(/host name/i).closest("form");
    if (!form) throw new Error("Enrollment form is missing");
    fireEvent.submit(form);
    expect(createRunnerActionMock).toHaveBeenCalledOnce();
    await userEvent.setup().keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeTruthy();
    await act(async () => pending.reject(new Error("private-response-marker")));
    expect(screen.getByRole("alert").textContent).toContain("Try again");
    expect(screen.getByRole("alert").textContent).not.toContain("private-response-marker");
    expect(screen.getByLabelText(/host name/i)).toHaveProperty("value", "web-prod-1");
    expect(screen.getByRole("button", {name: "Cancel"})).toHaveProperty("disabled", false);
  });
  it("refuses a bad registry entry in-form and never calls the action", async () => {
    await openDialog();
    fireEvent.change(screen.getByLabelText(/host name/i), { target: { value: "web-prod-1" } });
    fireEvent.click(screen.getByRole("radio", {name: /^Approved destinations only/}));
    fireEvent.change(screen.getByLabelText(/package download sites/i), {
      target: { value: "http://not a host" },
    });
    fireEvent.submit(screen.getByLabelText(/host name/i).closest("form") as HTMLFormElement);
    await waitFor(() => expect(screen.getByText(/must be a host name/i)).toBeTruthy());
    expect(createRunnerActionMock).not.toHaveBeenCalled();
  });

  it("exposes the supported policy fields and enrollment defaults", async () => {
    // Dimension 4.4 — isolation, network policy, registry allowlist, and
    // workers, each at its documented default (network → allow_all: the
    // explicit interim posture, because allow_list_egress would degrade every
    // runner until its enforcement ships).
    await openDialog();

    // All four assignment fields render.
    expect(screen.getByText("Isolation")).toBeTruthy();
    expect(screen.getByText("Network policy")).toBeTruthy();
    expect(screen.getByLabelText(/package download sites/i)).toBeTruthy();
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
    expect((screen.getByLabelText(/package download sites/i) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Workers") as HTMLInputElement).value).toBe(String(DEFAULT_WORKER_COUNT));
    expect(screen.getByText("Bubblewrap")).toBeTruthy();
    expect(screen.getByRole("radio", {name: /^Landlock/}).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("switch", {name: "Durable tasks"}).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("combobox", {name: "Decision model"}).textContent).toBe("TypeSafe Jev");
  });
});

it("enrolls with the operator's explicit disabled durable task preference", async () => {
  createRunnerActionMock.mockResolvedValue({ok: true, data: {runner_id: "durable-runner", runner_token: "agt_rdurable"}});
  await openDialog();
  const setting = screen.getByRole("switch", {name: "Durable tasks"});
  expect(setting.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(setting);
  fireEvent.change(screen.getByLabelText(/host name/i), {target: {value: "durable-host"}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await waitFor(() => expect(createRunnerActionMock).toHaveBeenCalledWith(expect.objectContaining({
    host_id: "durable-host", assigned_policy: expect.objectContaining({durable_tasks_enabled: false}),
  })));
});

it("cannot enroll before the default decision model resolves, then saves the configured selection", async () => {
  const response = Promise.withResolvers<{ok: true; data: {models: typeof JEV_MODEL[]}}>();
  catalogue.mockReturnValue(response.promise);
  createRunnerActionMock.mockResolvedValue({ok: true, data: {runner_id: "runner", runner_token: "preview-token"}});
  render(<AddRunnerDialog onCreated={() => {}} />);
  fireEvent.click(screen.getByRole("button", {name: /create runner/i}));
  fireEvent.change(screen.getByLabelText(/host name/i), {target: {value: "build-host"}});
  const create = screen.getByRole("button", {name: "Create"});
  const form = create.closest("form");
  if (!form) throw new Error("Enrollment form is missing");
  expect(create).toHaveProperty("disabled", true);
  fireEvent.submit(form);
  expect(createRunnerActionMock).not.toHaveBeenCalled();
  await act(async () => response.resolve({ok: true, data: {models: [JEV_MODEL]}}));
  // TooltipButton removes its disabled wrapper when the catalogue resolves.
  const ready = screen.getByRole("button", {name: "Create"});
  expect(ready).toHaveProperty("disabled", false);
  fireEvent.click(ready);
  await waitFor(() => expect(createRunnerActionMock).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    host_id: "build-host", assigned_policy: expect.objectContaining({decision_model_id: JEV_MODEL.id}),
  })));
});

it("allows explicit enrollment without a decision model when catalogue access is refused", async () => {
  catalogue.mockResolvedValue({ok: false, error: "Model read access unavailable"});
  createRunnerActionMock.mockResolvedValue({ok: true, data: {runner_id: "runner", runner_token: "preview-token"}});
  const user = userEvent.setup();
  render(<AddRunnerDialog onCreated={() => {}} />);
  await user.click(screen.getByRole("button", {name: /create runner/i}));
  expect(await screen.findByText(/Could not load models/)).toBeTruthy();
  await user.type(screen.getByLabelText(/host name/i), "build-host");
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(screen.getByRole("option", {name: "None", exact: true}));
  await user.click(screen.getByRole("button", {name: "Create", exact: true}));
  await waitFor(() => expect(createRunnerActionMock).toHaveBeenCalledExactlyOnceWith({host_id: "build-host", labels: [],
    assigned_policy: {durable_tasks_enabled: true, sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []}}));
});
