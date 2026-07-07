import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { RadioGroup } from "./RadioGroup";
import { OptionCard } from "./OptionCard";

const OPTION_CARD_SRC_PATH = path.join(__dirname, "OptionCard.tsx");

function Picker({ onChange }: { onChange?: (value: string) => void }) {
  const [value, setValue] = useState("a");
  return (
    <RadioGroup
      value={value}
      onValueChange={(v) => {
        setValue(v);
        onChange?.(v);
      }}
      aria-label="Isolation mode"
    >
      <OptionCard value="a" label="Option A" description="First choice" />
      <OptionCard value="b" label="Option B" description="Second choice" />
    </RadioGroup>
  );
}

describe("OptionCard", () => {
  it("renders as an accessible radio item with label + description", () => {
    render(<Picker />);
    const cards = screen.getAllByRole("radio");
    expect(cards).toHaveLength(2);
    expect(screen.getByText("Option A")).toBeInTheDocument();
    expect(screen.getByText("First choice")).toBeInTheDocument();
  });

  it("marks only the selected card as data-state=checked", () => {
    render(<Picker />);
    const [cardA, cardB] = screen.getAllByRole("radio");
    expect(cardA.getAttribute("data-state")).toBe("checked");
    expect(cardB.getAttribute("data-state")).toBe("unchecked");
  });

  it("calls onValueChange with the clicked card's value", () => {
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    fireEvent.click(screen.getByText("Option B"));
    expect(onChange).toHaveBeenCalledWith("b");
  });

  it("renders an optional icon slot", () => {
    render(
      <RadioGroup aria-label="With icon">
        <OptionCard value="a" label="Icon option" icon={<span data-testid="icon">*</span>} />
      </RadioGroup>,
    );
    expect(screen.getByTestId("icon")).toBeInTheDocument();
  });

  it("uses only mapped design-system tokens — no arbitrary hex or arbitrary-value utility in its class strings", () => {
    const src = readFileSync(OPTION_CARD_SRC_PATH, "utf8");
    expect(src).not.toMatch(/#[0-9a-fA-F]{3,6}/);
    // Arbitrary-VALUE brackets (e.g. `bg-[#fff]`, `w-[42px]`) are the
    // violation; `data-[state=checked]:` is a standard Tailwind arbitrary
    // VARIANT selector (same idiom RadioGroupItem.tsx already uses), so
    // brackets immediately followed by `:` are excluded.
    expect(src).not.toMatch(/\[[^\]]+\](?!:)/);
  });
});
