import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useForm } from "react-hook-form";
import userEvent from "@testing-library/user-event";
import { zodResolver } from "@hookform/resolvers/zod";
import { Form } from "@agentsfleet/design-system";
import { POLICY_FORM_DEFAULTS, policyFormSchema, policyFromForm, type PolicyFormValues } from "./PolicyFields";
import { PolicyDurableTasksField } from "./PolicyDurableTasksField";

afterEach(cleanup);

function Harness({ saved, onSave }: { saved?: boolean; onSave: (enabled: boolean) => void }) {
  const form = useForm<PolicyFormValues>({ resolver: zodResolver(policyFormSchema), defaultValues: {
    ...POLICY_FORM_DEFAULTS, durable_tasks_enabled: saved ?? POLICY_FORM_DEFAULTS.durable_tasks_enabled,
  } });
  return <Form {...form}><form onSubmit={event => {void form.handleSubmit(values => {
    const {policy, error} = policyFromForm(values);
    expect(error).toBeNull();
    expect(policy).not.toBeNull();
    expect(policy).toEqual(expect.objectContaining({durable_tasks_enabled: values.durable_tasks_enabled}));
    onSave(values.durable_tasks_enabled);
  })(event);}}>
    <PolicyDurableTasksField control={form.control} />
    <button type="submit">Save policy</button>
  </form></Form>;
}

describe("Durable tasks runner policy", () => {
  it("starts enabled and saves an explicit disabled choice", async () => {
    const save = vi.fn();
    render(<Harness onSave={save} />);
    const control = screen.getByRole("switch", {name: "Durable tasks"});
    expect(control.getAttribute("aria-checked")).toBe("true");
    control.focus();
    await userEvent.setup().keyboard(" ");
    expect(control.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(screen.getByRole("button", {name: "Save policy"}));
    await waitFor(() => expect(save).toHaveBeenCalledWith(false));
  });

  it("reopens a disabled policy without silently enabling it on save", async () => {
    const save = vi.fn();
    render(<Harness saved={false} onSave={save} />);
    expect(screen.getByRole("switch", {name: "Durable tasks"}).getAttribute("aria-checked")).toBe("false");
    fireEvent.click(screen.getByRole("button", {name: "Save policy"}));
    await waitFor(() => expect(save).toHaveBeenCalledWith(false));
  });

  it("saves re-enabling a disabled runner", async () => {
    const save = vi.fn();
    render(<Harness saved={false} onSave={save} />);
    fireEvent.click(screen.getByRole("switch", {name: "Durable tasks"}));
    fireEvent.click(screen.getByRole("button", {name: "Save policy"}));
    await waitFor(() => expect(save).toHaveBeenCalledWith(true));
  });
});
