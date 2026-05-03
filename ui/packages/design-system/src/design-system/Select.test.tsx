import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  Select,
  SelectTrigger,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectValue,
} from "./Select";

function Harness({
  defaultValue,
  onValueChange,
}: {
  defaultValue?: string;
  onValueChange?: (v: string) => void;
}) {
  return (
    <Select defaultValue={defaultValue} onValueChange={onValueChange}>
      <SelectTrigger aria-label="provider">
        <SelectValue placeholder="Pick one" />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectLabel>Models</SelectLabel>
          <SelectItem value="gpt">GPT</SelectItem>
          <SelectItem value="claude">Claude</SelectItem>
        </SelectGroup>
        <SelectSeparator />
        <SelectItem value="gemini" disabled>
          Gemini
        </SelectItem>
      </SelectContent>
    </Select>
  );
}

describe("Select", () => {
  it("renders the trigger with placeholder when no value is set", () => {
    render(<Harness />);
    expect(screen.getByRole("combobox", { name: /provider/i })).toBeInTheDocument();
    expect(screen.getByText("Pick one")).toBeInTheDocument();
  });

  it("renders the selected value when defaultValue is provided", () => {
    render(<Harness defaultValue="claude" />);
    expect(screen.getByText("Claude")).toBeInTheDocument();
  });

  it("opens, lists items + label, and emits onValueChange on selection", () => {
    const onValueChange = vi.fn();
    render(<Harness onValueChange={onValueChange} />);
    const trigger = screen.getByRole("combobox", { name: /provider/i });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });

    const gpt = screen.getByText("GPT");
    expect(gpt).toBeInTheDocument();
    expect(screen.getByText("Models")).toBeInTheDocument();

    fireEvent.click(gpt);
    expect(onValueChange).toHaveBeenCalledWith("gpt");
  });

  it("respects disabled items", () => {
    render(<Harness />);
    const trigger = screen.getByRole("combobox", { name: /provider/i });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    const gemini = screen.getByText("Gemini").closest("[role='option']");
    expect(gemini?.getAttribute("data-disabled")).not.toBeNull();
  });
});
