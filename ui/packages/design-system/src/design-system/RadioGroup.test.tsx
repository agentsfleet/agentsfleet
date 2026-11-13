import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RadioGroup, RadioGroupItem } from "./RadioGroup";

describe("RadioGroup", () => {
  it("selects one key mode and announces both choices", () => {
    const onValueChange = vi.fn();
    const ref = {current: null as HTMLButtonElement | null};
    render(<RadioGroup aria-label="Provider API key" defaultValue="new" onValueChange={onValueChange}>
      <RadioGroupItem value="new" aria-label="Add new key" />
      <RadioGroupItem value="saved" aria-label="Use saved key" ref={ref} />
    </RadioGroup>);
    const create = screen.getByRole("radio", {name: "Add new key"});
    const saved = screen.getByRole("radio", {name: "Use saved key"});
    expect(screen.getByRole("radiogroup", {name: "Provider API key"})).toBeInTheDocument();
    expect(ref.current).toBe(saved);
    expect(create).toBeChecked();
    expect(saved).not.toBeChecked();
    fireEvent.click(saved);
    expect(onValueChange).toHaveBeenCalledExactlyOnceWith("saved");
    expect(create).not.toBeChecked();
    expect(saved).toBeChecked();
  });

  it("does not select a disabled key mode", () => {
    const onValueChange = vi.fn();
    render(<RadioGroup aria-label="Provider API key" defaultValue="new" onValueChange={onValueChange}>
      <RadioGroupItem value="new" aria-label="Add new key" />
      <RadioGroupItem value="saved" aria-label="Use saved key" disabled />
    </RadioGroup>);
    const saved = screen.getByRole("radio", {name: "Use saved key"});
    fireEvent.click(saved);
    expect(saved).toBeDisabled();
    expect(saved).not.toBeChecked();
    expect(screen.getByRole("radio", {name: "Add new key"})).toBeChecked();
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it("leaves controlled selection unchanged until the caller accepts it", () => {
    const onValueChange = vi.fn();
    render(<RadioGroup aria-label="Provider API key" value="new" onValueChange={onValueChange}>
      <RadioGroupItem value="new" aria-label="Add new key" />
      <RadioGroupItem value="saved" aria-label="Use saved key" />
    </RadioGroup>);
    fireEvent.click(screen.getByRole("radio", {name: "Use saved key"}));
    expect(onValueChange).toHaveBeenCalledExactlyOnceWith("saved");
    expect(screen.getByRole("radio", {name: "Add new key"})).toBeChecked();
    expect(screen.getByRole("radio", {name: "Use saved key"})).not.toBeChecked();
  });
});
