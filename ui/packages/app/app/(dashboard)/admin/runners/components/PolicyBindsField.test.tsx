import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Form, TooltipProvider } from "@agentsfleet/design-system";
import { PolicyBindsField } from "./PolicyBindsField";
import { POLICY_FORM_DEFAULTS, policyFormSchema, type PolicyFormValues } from "./PolicyFields";
import { BASELINE_RO_PATHS, MAX_EXTRA_BINDS } from "./policy-binds";

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
    <TooltipProvider delayDuration={0}>
      <Form {...form}>
        <PolicyBindsField control={form.control} />
      </Form>
    </TooltipProvider>
  );
}

const ROW = { path: "/srv/models", mode: "read_only" as const, note: "gpu weights" };

// The editor is a disclosure, collapsed when the runner carries no binds — it
// is the only unbounded surface in the assignment form, and left open it grew
// the dialog to the full viewport for an edit that never touches binds. A test
// that wants the controls has to open it, exactly as an operator does.
function openBinds() {
  fireEvent.click(screen.getByRole("button", { name: /Sandbox mounts/ }));
}

describe("PolicyBindsField", () => {
  it("renders no rows and no count when the runner carries no mounts", () => {
    render(<Harness />);
    expect(screen.queryByText(/assigned/)).toBeNull();
    expect(screen.queryByLabelText("Mount path 1")).toBeNull();
  });

  it("stays collapsed when there are no binds, so it cannot dominate the dialog", () => {
    render(<Harness />);
    expect(screen.queryByRole("button", { name: "Add mount" })).toBeNull();
    openBinds();
    expect(screen.getByRole("button", { name: "Add mount" })).toBeTruthy();
  });

  it("opens already expanded when the runner carries binds", () => {
    // An operator editing a runner that HAS mounts must see them without
    // hunting for a disclosure — hiding assigned state is how a wipe goes
    // unnoticed, which is the bug this whole field exists to prevent.
    render(<Harness initial={[ROW]} />);
    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("/srv/models");
  });

  it("adds an empty read-only row — access never widens by omission", () => {
    render(<Harness />);
    openBinds();
    fireEvent.click(screen.getByRole("button", { name: "Add mount" }));
    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("");
    expect(screen.getByLabelText("Mount mode 1").textContent).toContain("Read-only");
  });

  it("renders a stored assignment's rows with their path, mode and note", () => {
    render(<Harness initial={[ROW]} />);
    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("/srv/models");
    expect((screen.getByLabelText("Mount note 1") as HTMLInputElement).value).toBe("gpu weights");
    expect(screen.getByLabelText("Mount mode 1").textContent).toContain("Read-only");
  });

  it("renders a read-write row's mode on its select, not behind a description", () => {
    render(<Harness initial={[{ ...ROW, mode: "read_write" }]} />);
    expect(screen.getByLabelText("Mount mode 1").textContent).toContain("Read-write");
  });

  it("reveals every default mount in the header tooltip, none of it editable", async () => {
    render(<Harness />);
    // The baseline lives behind the header's hover, not in the body: focusing
    // the info affordance opens it, and every contract path is listed with no
    // "Mount path N" control attached — informational, never a row.
    fireEvent.focus(screen.getByLabelText("Default mounts"));
    for (const path of BASELINE_RO_PATHS) {
      expect((await screen.findAllByText(path)).length).toBeGreaterThan(0);
    }
    expect(screen.queryByLabelText("Mount path 1")).toBeNull();
  });

  it("removes the row the operator pointed at, not the last one", () => {
    render(<Harness initial={[ROW, { path: "/srv/cache", mode: "read_only", note: "" }]} />);
    fireEvent.click(screen.getByRole("button", { name: "Remove mount 1" }));
    expect((screen.getByLabelText("Mount path 1") as HTMLInputElement).value).toBe("/srv/cache");
    expect(screen.queryByLabelText("Mount path 2")).toBeNull();
  });

  it("stops offering more rows at the shared cap and says why", () => {
    const full = Array.from({ length: MAX_EXTRA_BINDS }, (_, i) => ({
      path: `/srv/models-${i}`,
      mode: "read_only" as const,
      note: "",
    }));
    render(<Harness initial={full} />);
    const add = screen.getByRole("button", { name: "Add mount" });
    expect(add.hasAttribute("disabled")).toBe(true);
    expect(screen.getByText(`At most ${MAX_EXTRA_BINDS} mounts per runner.`)).toBeTruthy();
    fireEvent.click(add);
    expect(screen.queryByLabelText(`Mount path ${MAX_EXTRA_BINDS + 1}`)).toBeNull();
  });
});
