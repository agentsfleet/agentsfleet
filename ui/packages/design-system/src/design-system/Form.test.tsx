import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Form,
  FormField,
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
  useFormField,
} from "./Form";
import { Input } from "./Input";

const schema = z.object({
  name: z.string().min(2, "Name must be at least 2 characters"),
});
type FormValues = z.infer<typeof schema>;

function NameForm({ onSubmit }: { onSubmit: (v: FormValues) => void }) {
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "" },
  });
  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)}>
        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Name</FormLabel>
              <FormControl>
                <Input data-testid="name-input" {...field} />
              </FormControl>
              <FormDescription>Your full name.</FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        <button type="submit">Submit</button>
      </form>
    </Form>
  );
}

describe("Form", () => {
  it("associates label with input via for/id", () => {
    render(<NameForm onSubmit={() => {}} />);
    const label = screen.getByText("Name");
    const input = screen.getByLabelText("Name");
    expect(label.tagName).toBe("LABEL");
    expect(input.tagName).toBe("INPUT");
    expect(label.getAttribute("for")).toBe(input.getAttribute("id"));
  });

  it("links FormDescription via aria-describedby", () => {
    render(<NameForm onSubmit={() => {}} />);
    const input = screen.getByLabelText("Name");
    const desc = screen.getByText("Your full name.");
    expect(input.getAttribute("aria-describedby")).toContain(desc.id);
  });

  it("renders a validation error and sets aria-invalid on submit", async () => {
    render(<NameForm onSubmit={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));
    const message = await screen.findByText(/Name must be at least 2/);
    const input = screen.getByLabelText("Name");
    expect(message).toBeTruthy();
    expect(input.getAttribute("aria-invalid")).toBe("true");
  });

  it("calls onSubmit with parsed values when valid", async () => {
    const submitted: FormValues[] = [];
    render(<NameForm onSubmit={(v) => submitted.push(v)} />);
    fireEvent.change(screen.getByTestId("name-input"), { target: { value: "Ada" } });
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));
    await waitFor(() => expect(submitted).toEqual([{ name: "Ada" }]));
  });

  it("renders no FormMessage node when the field is valid and has no children", () => {
    render(<NameForm onSubmit={() => {}} />);
    // FormMessage returns null when there is no error and no children body.
    expect(document.querySelector('[id$="-form-item-message"]')).toBeNull();
  });

  it("renders nothing when the field error carries no message string", async () => {
    function MessagelessForm() {
      const form = useForm<FormValues>({ defaultValues: { name: "" } });
      return (
        <Form {...form}>
          <FormField
            control={form.control}
            name="name"
            render={() => (
              <FormItem>
                <FormMessage />
              </FormItem>
            )}
          />
          <button
            type="button"
            onClick={() => form.setError("name", { type: "manual" })}
          >
            Fail
          </button>
        </Form>
      );
    }
    render(<MessagelessForm />);
    fireEvent.click(screen.getByRole("button", { name: "Fail" }));
    // error is truthy but error.message is undefined -> String(undefined ?? "")
    // collapses to "" so FormMessage stays null (no empty <p> emitted).
    await waitFor(() =>
      expect(document.querySelector('[id$="-form-item-message"]')).toBeNull(),
    );
  });

  it("renders explicit FormMessage children when there is no field error", () => {
    function HelperForm() {
      const form = useForm<FormValues>({
        resolver: zodResolver(schema),
        defaultValues: { name: "" },
      });
      return (
        <Form {...form}>
          <FormField
            control={form.control}
            name="name"
            render={() => (
              <FormItem>
                <FormMessage>Static hint</FormMessage>
              </FormItem>
            )}
          />
        </Form>
      );
    }
    render(<HelperForm />);
    expect(screen.getByText("Static hint")).toBeTruthy();
  });
});

describe("useFormField guards", () => {
  function HookProbe() {
    useFormField();
    return null;
  }

  // Form provider present (so useFormContext resolves) but no FormField:
  // the fieldContext-null guard must throw.
  function NoFieldHarness() {
    const form = useForm({ defaultValues: {} });
    return (
      <Form {...form}>
        <HookProbe />
      </Form>
    );
  }

  // FormField present (fieldContext set) but no FormItem wrapper:
  // the itemContext-null guard must throw.
  function NoItemHarness() {
    const form = useForm<FormValues>({ defaultValues: { name: "" } });
    return (
      <Form {...form}>
        <FormField
          control={form.control}
          name="name"
          render={() => <HookProbe />}
        />
      </Form>
    );
  }

  it("throws when used outside <FormField>", () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(() => render(<NoFieldHarness />)).toThrow(
      /must be used within <FormField>/,
    );
    errSpy.mockRestore();
  });

  it("throws when used outside <FormItem>", () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(() => render(<NoItemHarness />)).toThrow(
      /must be used within <FormItem>/,
    );
    errSpy.mockRestore();
  });
});
