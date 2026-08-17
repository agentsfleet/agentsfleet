import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Form } from "@agentsfleet/design-system";
import { PolicyBindsField } from "./PolicyBindsField";
import { POLICY_FORM_DEFAULTS, policyFormSchema, type PolicyFormValues } from "./PolicyFields";
import { MAX_EXTRA_BINDS } from "./policy-binds";

afterEach(() => cleanup());

// A minimal host form — the field owns append/remove and the cap, and none of
// that is reachable without a react-hook-form context to drive it.
function Harness({ initial }: { initial?: PolicyFormValues["extra_binds"] }) {
  const form = useForm<PolicyFormValues>({
    resolver: zodResolver(policyFormSchema),
    defaultValues: { ...POLICY_FORM_DEFAULTS, extra_binds: initial ?? [] },
  });
  // No form element wraps this: the field needs the react-hook-form context
  // the Form provider supplies, not a submit target — nothing here submits.
  return (
    <Form {...form}>
      <PolicyBindsField control={form.control} />
    </Form>
  );
}

const ROW = { path: "/srv/models", mode: "read_only" as const, note: "gpu weights" };

describe("PolicyBindsField", () => {
  it("states the baseline-only case when the runner carries no binds", () => {
    render(<Harness />);
    expect(screen.getByText(/No extra binds/)).toBeTruthy();
    expect(screen.queryByLabelText("Bind path 1")).toBeNull();
  });

  it("adds an empty read-only row — access never widens by omission", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Add bind" }));
    expect((screen.getByLabelText("Bind path 1") as HTMLInputElement).value).toBe("");
    expect(screen.getByLabelText("Bind mode 1").textContent).toContain("Read-only");
    expect(screen.queryByText(/No extra binds/)).toBeNull();
  });

  it("renders a stored assignment's rows with their path, mode and note", () => {
    render(<Harness initial={[ROW]} />);
    expect((screen.getByLabelText("Bind path 1") as HTMLInputElement).value).toBe("/srv/models");
    expect((screen.getByLabelText("Bind note 1") as HTMLInputElement).value).toBe("gpu weights");
    expect(screen.getByLabelText("Bind mode 1").textContent).toContain("Read-only");
  });

  it("names what read-write actually widens rather than leaving it a label", () => {
    render(<Harness initial={[{ ...ROW, mode: "read_write" }]} />);
    expect(screen.getByText(/write through to the host path/)).toBeTruthy();
  });

  it("removes the row the operator pointed at, not the last one", () => {
    render(<Harness initial={[ROW, { path: "/srv/cache", mode: "read_only", note: "" }]} />);
    fireEvent.click(screen.getByRole("button", { name: "Remove bind 1" }));
    expect((screen.getByLabelText("Bind path 1") as HTMLInputElement).value).toBe("/srv/cache");
    expect(screen.queryByLabelText("Bind path 2")).toBeNull();
  });

  it("stops offering more rows at the shared cap and says why", () => {
    const full = Array.from({ length: MAX_EXTRA_BINDS }, (_, i) => ({
      path: `/srv/models-${i}`,
      mode: "read_only" as const,
      note: "",
    }));
    render(<Harness initial={full} />);
    const add = screen.getByRole("button", { name: "Add bind" });
    expect(add.hasAttribute("disabled")).toBe(true);
    expect(screen.getByText(`At most ${MAX_EXTRA_BINDS} binds per runner.`)).toBeTruthy();
    fireEvent.click(add);
    expect(screen.queryByLabelText(`Bind path ${MAX_EXTRA_BINDS + 1}`)).toBeNull();
  });
});
