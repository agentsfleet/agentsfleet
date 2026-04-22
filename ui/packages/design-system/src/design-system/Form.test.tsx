import { describe, it, expect } from "vitest";
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
});
