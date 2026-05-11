import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { RadioGroup, RadioGroupItem } from "./RadioGroup";

function Uncontrolled({ defaultValue }: { defaultValue?: string }) {
  return (
    <RadioGroup defaultValue={defaultValue} aria-label="Sample">
      <label>
        <RadioGroupItem value="a" />
        Option A
      </label>
      <label>
        <RadioGroupItem value="b" />
        Option B
      </label>
      <label>
        <RadioGroupItem value="c" />
        Option C
      </label>
    </RadioGroup>
  );
}

function Controlled({ onChange }: { onChange?: (value: string) => void }) {
  const [value, setValue] = useState("a");
  return (
    <RadioGroup
      value={value}
      onValueChange={(v) => {
        setValue(v);
        onChange?.(v);
      }}
      aria-label="Controlled"
    >
      <label>
        <RadioGroupItem value="a" />
        Option A
      </label>
      <label>
        <RadioGroupItem value="b" />
        Option B
      </label>
    </RadioGroup>
  );
}

describe("RadioGroup", () => {
  it("renders a radiogroup with the listed items", () => {
    render(<Uncontrolled />);
    expect(screen.getByRole("radiogroup", { name: "Sample" })).toBeTruthy();
    expect(screen.getAllByRole("radio").length).toBe(3);
  });

  it("uncontrolled defaultValue selects the matching item", () => {
    render(<Uncontrolled defaultValue="b" />);
    const radios = screen.getAllByRole("radio");
    expect(radios[1]?.getAttribute("data-state")).toBe("checked");
    expect(radios[0]?.getAttribute("data-state")).toBe("unchecked");
    expect(radios[2]?.getAttribute("data-state")).toBe("unchecked");
  });

  it("controlled value reflects the parent's state", () => {
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const [a, b] = screen.getAllByRole("radio");
    expect(a?.getAttribute("data-state")).toBe("checked");
    fireEvent.click(b!);
    expect(onChange).toHaveBeenCalledWith("b");
    // Parent rerendered with value="b" — Radix flipped data-state.
    expect(b?.getAttribute("data-state")).toBe("checked");
    expect(a?.getAttribute("data-state")).toBe("unchecked");
  });

  it("arrow keys move selection between items (Radix roving tabindex)", () => {
    render(<Uncontrolled defaultValue="a" />);
    const radios = screen.getAllByRole("radio");
    radios[0]!.focus();
    fireEvent.keyDown(radios[0]!, { key: "ArrowDown" });
    expect(radios[1]?.getAttribute("data-state")).toBe("checked");
    fireEvent.keyDown(radios[1]!, { key: "ArrowDown" });
    expect(radios[2]?.getAttribute("data-state")).toBe("checked");
    fireEvent.keyDown(radios[2]!, { key: "ArrowUp" });
    expect(radios[1]?.getAttribute("data-state")).toBe("checked");
  });

  it("disabled item does not receive a state flip on click", () => {
    render(
      <RadioGroup defaultValue="a" aria-label="Disabled sample">
        <label>
          <RadioGroupItem value="a" />
          A
        </label>
        <label>
          <RadioGroupItem value="b" disabled />
          B
        </label>
      </RadioGroup>,
    );
    const [a, b] = screen.getAllByRole("radio");
    fireEvent.click(b!);
    expect(b?.getAttribute("data-state")).toBe("unchecked");
    expect(a?.getAttribute("data-state")).toBe("checked");
  });
});
