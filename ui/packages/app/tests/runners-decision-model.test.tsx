import React, { StrictMode, useCallback, useState } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useForm } from "react-hook-form";
import { Form } from "@agentsfleet/design-system";
import type { AdminModel } from "@/lib/api/admin_model_library";
import { ADMIN_MODEL, JEV_MODEL } from "./helpers/catalogue-fixtures";
import { PolicyDecisionField } from "@/app/(dashboard)/admin/runners/components/PolicyDecisionField";
import { formFromPolicy, policyFromForm, type PolicyFormValues } from "@/app/(dashboard)/admin/runners/components/PolicyFields";

const {read} = vi.hoisted(() => ({read: vi.fn()}));
vi.mock("@/app/(dashboard)/admin/models/actions", () => ({listAdminModelsAction: read}));

const SELECTED = "0199f0c0-0000-7000-8000-000000000001";
const stored = {durable_tasks_enabled: true, decision_model_id: SELECTED, sandbox_tier: "landlock_full" as const,
  network_policy: "allow_all" as const, registry_allowlist: ["registry.npmjs.org"], worker_count: 3,
  extra_binds: [{path: "/srv/models", mode: "read_only" as const, note: "shared cache"}]};
const saved = vi.fn();
function unassigned() { return {...formFromPolicy(stored), decision_model_id: undefined}; }
function submitForm() {
  const form = screen.getByRole("button", {name: "Save assignment"}).closest("form");
  if (!form) throw new Error("Assignment form is missing");
  fireEvent.submit(form);
}
function Harness({defaults = formFromPolicy(stored), observePending}: {defaults?: PolicyFormValues; observePending?: (pending: boolean) => void}) {
  const form = useForm<PolicyFormValues>({defaultValues: defaults});
  const [pending, setPending] = useState(defaults.decision_model_id === undefined);
  const reportPending = useCallback((value: boolean) => {setPending(value); observePending?.(value);}, [observePending]);
  return <Form {...form}><form onSubmit={event => {
    event.preventDefault();
    if (!pending) saved(policyFromForm(form.getValues()));
  }}>
    <PolicyDecisionField control={form.control} onPendingChange={reportPending} />
    <button type="submit" disabled={pending}>Save assignment</button>
  </form></Form>;
}
afterEach(() => {cleanup(); vi.resetAllMocks();});

it("selects the configured TypeSafe Jev default and preserves the rest of the assignment", async () => {
  read.mockResolvedValue({ok: true, data: {models: [JEV_MODEL]}});
  render(<Harness defaults={unassigned()} />);
  expect(screen.getByRole("button", {name: "Save assignment"})).toHaveProperty("disabled", true);
  await waitFor(() => expect(screen.getByRole("combobox", {name: "Decision model"}).textContent).toBe("TypeSafe Jev"));
  await userEvent.setup().click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: {...stored, decision_model_id: JEV_MODEL.id}, error: null});
  expect(read).toHaveBeenCalledOnce();
});

it("retains an override through a catalogue failure and can retry to select Jev", async () => {
  const disabled = {...ADMIN_MODEL, id: SELECTED, decision: {enabled: false, profile: {revision: "reviewed-1", api: "typesafe"}}};
  read.mockRejectedValueOnce(new Error("catalogue unavailable"))
    .mockResolvedValue({ok: true, data: {models: [JEV_MODEL, disabled]}});
  const user = userEvent.setup();
  render(<Harness />);
  expect(await screen.findByText(/Could not load models/)).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: stored, error: null});
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  expect((await screen.findByRole("option", {name: "Current selection (unavailable)"})).getAttribute("aria-disabled")).toBe("true");
  await user.click(screen.getByRole("option", {name: "TypeSafe Jev"}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: {...stored, decision_model_id: JEV_MODEL.id}, error: null});
  expect(read).toHaveBeenCalledTimes(2);
});

it("requires setup when the catalogue has no enabled Jev default", async () => {
  read.mockResolvedValue({ok: true, data: {models: [{...JEV_MODEL, decision: {...JEV_MODEL.decision, enabled: false}}]}});
  render(<Harness defaults={unassigned()} />);
  expect(await screen.findByText(/TypeSafe Jev needs setup/)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Save assignment"})).toHaveProperty("disabled", true);
  submitForm();
  expect(saved).not.toHaveBeenCalled();
});

it("keeps an unassigned form blocked through a refused read and resolves Jev on retry", async () => {
  read.mockResolvedValueOnce({ok: false, error: "Model read access unavailable"})
    .mockResolvedValueOnce({ok: true, data: {models: [JEV_MODEL]}});
  const user = userEvent.setup();
  render(<Harness defaults={unassigned()} />);
  expect(await screen.findByText(/Could not load models/)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Save assignment"})).toHaveProperty("disabled", true);
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.getByRole("button", {name: "Save assignment"})).toHaveProperty("disabled", false));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: {...stored, decision_model_id: JEV_MODEL.id}, error: null});
  expect(read).toHaveBeenCalledTimes(2);
});

it("keeps saving blocked across strict-mode effect cleanup while Jev is unresolved", async () => {
  const response = Promise.withResolvers<{ok: true; data: {models: AdminModel[]}}>();
  read.mockReturnValue(response.promise);
  render(<StrictMode><Harness defaults={unassigned()} /></StrictMode>);
  const save = screen.getByRole("button", {name: "Save assignment"});
  expect(save).toHaveProperty("disabled", true);
  submitForm();
  expect(saved).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledOnce();
  await act(async () => response.resolve({ok: true, data: {models: [JEV_MODEL]}}));
  await waitFor(() => expect(save).toHaveProperty("disabled", false));
  await userEvent.setup().click(save);
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: {...stored, decision_model_id: JEV_MODEL.id}, error: null});
});

it("retains an enabled alternative and does not reload a ready catalogue", async () => {
  const alternative = {...ADMIN_MODEL, id: SELECTED, decision: {enabled: true, profile: {revision: "reviewed-1", api: "other"}}};
  read.mockResolvedValue({ok: true, data: {models: [JEV_MODEL, alternative, ADMIN_MODEL]}});
  const user = userEvent.setup();
  render(<Harness />);
  const selector = screen.getByRole("combobox", {name: "Decision model"});
  await waitFor(() => expect(selector.textContent).toBe("GLM 5.2 · fireworks"));
  await user.click(selector);
  expect(await screen.findByRole("option", {name: "GLM 5.2 · fireworks"})).toBeTruthy();
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: stored, error: null});
  expect(read).toHaveBeenCalledOnce();
});

it("does not use a similarly named decision model from another API as the default", async () => {
  const alternative = {...JEV_MODEL, decision: {enabled: true, profile: {revision: "reviewed-1", api: "other"}}};
  read.mockResolvedValue({ok: true, data: {models: [alternative]}});
  const user = userEvent.setup();
  render(<Harness defaults={{...unassigned(), decision_model_id: undefined}} />);
  expect(await screen.findByText(/TypeSafe Jev needs setup/)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Save assignment"})).toHaveProperty("disabled", true);
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(screen.getByRole("option", {name: "TypeSafe Jev · typesafe"}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: {...stored, decision_model_id: JEV_MODEL.id}, error: null});
});

it.each(["success", "failure"] as const)("ignores a late catalogue %s after the editor closes", async outcome => {
  const response = Promise.withResolvers<{ok: true; data: {models: AdminModel[]}}>();
  const pending = vi.fn();
  read.mockReturnValue(response.promise);
  const editor = render(<Harness defaults={unassigned()} observePending={pending} />);
  expect(pending).toHaveBeenCalledExactlyOnceWith(true);
  editor.unmount();
  await act(async () => {
    if (outcome === "success") response.resolve({ok: true, data: {models: [JEV_MODEL]}});
    else response.reject(new Error("late failure"));
  });
  expect(pending).toHaveBeenCalledExactlyOnceWith(true);
  expect(saved).not.toHaveBeenCalled();
});

it("retains an explicit None choice when a pending default lookup completes", async () => {
  const response = Promise.withResolvers<{ok: true; data: {models: AdminModel[]}}>();
  read.mockReturnValue(response.promise);
  const user = userEvent.setup();
  render(<Harness defaults={unassigned()} />);
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(screen.getByRole("option", {name: /^None$/}));
  await act(async () => response.resolve({ok: true, data: {models: [JEV_MODEL]}}));
  expect(screen.getByRole("combobox", {name: "Decision model"}).textContent).toBe("None");
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  const {decision_model_id: _omitted, ...withoutDecision} = stored;
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: withoutDecision, error: null});
});

it("preserves None through a catalogue retry instead of silently assigning Jev", async () => {
  read.mockResolvedValueOnce({ok: false, error: "Model read access unavailable"})
    .mockResolvedValue({ok: true, data: {models: [JEV_MODEL]}});
  const user = userEvent.setup();
  render(<Harness defaults={unassigned()} />);
  expect(await screen.findByText(/Could not load models/)).toBeTruthy();
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(screen.getByRole("option", {name: /^None$/}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  const {decision_model_id: _omitted, ...withoutDecision} = stored;
  expect(saved).toHaveBeenCalledExactlyOnceWith({policy: withoutDecision, error: null});
});
