import React from "react";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useForm } from "react-hook-form";
import { Form } from "@agentsfleet/design-system";
import { adminModelListSchema } from "@/lib/api/catalogue-types";
import { ADMIN_MODEL } from "./helpers/catalogue-fixtures";
import { PolicyDecisionField } from "@/app/(dashboard)/admin/runners/components/PolicyDecisionField";
import { formFromPolicy, policyFromForm, type PolicyFormValues } from "@/app/(dashboard)/admin/runners/components/PolicyFields";

const {read} = vi.hoisted(() => ({read: vi.fn()}));
vi.mock("@/app/(dashboard)/admin/models/actions", () => ({listAdminModelsAction: read}));

const SELECTED = "0199f0c0-0000-7000-8000-000000000001";
const AVAILABLE = "0199f0c0-0000-7000-8000-000000000002";
const stored = { durable_tasks_enabled: true,decision_model_id: SELECTED, sandbox_tier: "landlock_full" as const,
  network_policy: "allow_all" as const, registry_allowlist: ["registry.npmjs.org"], worker_count: 3,
  extra_binds: [{path: "/srv/models", mode: "read_only" as const, note: "shared cache"}]};
const saved = vi.fn();
function Harness({defaults = formFromPolicy(stored)}: {defaults?: PolicyFormValues}) {
  const form = useForm<PolicyFormValues>({defaultValues: defaults});
  return <Form {...form}><form onSubmit={event => {event.preventDefault(); saved(policyFromForm(form.getValues()));}}>
    <PolicyDecisionField control={form.control} /><button type="submit">Save assignment</button>
  </form></Form>;
}
afterEach(() => {cleanup(); vi.resetAllMocks();});

it("retains the assignment through a catalogue failure, retries, selects an enabled model and clears without losing policy", async () => {
  const catalogue = adminModelListSchema.parse({models: [
    {...ADMIN_MODEL, id: AVAILABLE, prices: [], metadata: {...ADMIN_MODEL.metadata, display_name: "Jev"},
      decision: {enabled: true, profile: {revision: "reviewed-1", api: "typesafe"}}},
    {...ADMIN_MODEL, id: SELECTED, decision: {enabled: false, profile: {revision: "reviewed-1", api: "typesafe"}}},
  ]});
  read.mockRejectedValueOnce(new Error("catalogue unavailable")).mockResolvedValue({ok: true, data: catalogue});
  const user = userEvent.setup();
  render(<Harness />);
  expect(read).not.toHaveBeenCalled();
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await screen.findByText(/Could not load models/);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: stored, error: null});
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await screen.findByRole("option", {name: "Jev · fireworks"});
  expect(screen.getByRole("option", {name: "Current selection (unavailable)"}).getAttribute("aria-disabled")).toBe("true");
  await user.click(screen.getByRole("option", {name: "Jev · fireworks"}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: {...stored, decision_model_id: AVAILABLE}, error: null});
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(screen.getByRole("option", {name: "None"}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  const {decision_model_id: _removed, ...cleared} = stored;
  await waitFor(() => expect(saved).toHaveBeenLastCalledWith({policy: cleared, error: null}));
  expect(read).toHaveBeenCalledTimes(2);
});

it("keeps an unassigned runner's policy through a refused catalogue read before selecting a reviewed model", async () => {
  const defaults = formFromPolicy(stored);
  delete defaults.decision_model_id;
  const {decision_model_id: _unassigned, ...unassigned} = stored;
  const catalogue = adminModelListSchema.parse({models: [{...ADMIN_MODEL, id: AVAILABLE, prices: [],
    metadata: {...ADMIN_MODEL.metadata, display_name: "Jev"},
    decision: {enabled: true, profile: {revision: "reviewed-1", api: "typesafe"}}}]});
  read.mockResolvedValueOnce({ok: false, error: "Model read access unavailable"})
    .mockResolvedValueOnce({ok: true, data: catalogue});
  const user = userEvent.setup();
  render(<Harness defaults={defaults} />);
  expect(screen.getByRole("combobox", {name: "Decision model"}).textContent).toBe("None");
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  expect(await screen.findByText(/Could not load models/)).toBeTruthy();
  expect(screen.queryByRole("option", {name: /Current selection/})).toBeNull();
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: unassigned, error: null});
  await user.click(screen.getByRole("combobox", {name: "Decision model"}));
  await user.click(await screen.findByRole("option", {name: "Jev · fireworks"}));
  await user.click(screen.getByRole("button", {name: "Save assignment"}));
  expect(saved).toHaveBeenLastCalledWith({policy: {...unassigned, decision_model_id: AVAILABLE}, error: null});
  expect(read).toHaveBeenCalledTimes(2);
});
