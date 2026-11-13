import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("announces and changes the selection through the native control", () => {
    const onCheckedChange = vi.fn();
    const ref = {current: null as HTMLButtonElement | null};
    render(<Checkbox aria-label="Make active" onCheckedChange={onCheckedChange} ref={ref} />);
    const checkbox = screen.getByRole("checkbox", {name: "Make active"});
    expect(ref.current).toBe(checkbox);
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(onCheckedChange).toHaveBeenCalledExactlyOnceWith(true);
    fireEvent.click(checkbox);
    expect(checkbox).not.toBeChecked();
    expect(onCheckedChange).toHaveBeenLastCalledWith(false);
  });

  it("keeps a disabled selection unchanged", () => {
    const onCheckedChange = vi.fn();
    render(<Checkbox aria-label="Make active" checked disabled onCheckedChange={onCheckedChange} />);
    const checkbox = screen.getByRole("checkbox", {name: "Make active"});
    fireEvent.click(checkbox);
    expect(checkbox).toBeDisabled();
    expect(checkbox).toBeChecked();
    expect(onCheckedChange).not.toHaveBeenCalled();
  });

  it("leaves controlled state owned by the caller until it confirms the change", () => {
    const onCheckedChange = vi.fn();
    const view = render(<Checkbox aria-label="Make active" checked={false} onCheckedChange={onCheckedChange} />);
    const checkbox = screen.getByRole("checkbox", {name: "Make active"});
    fireEvent.click(checkbox);
    expect(onCheckedChange).toHaveBeenCalledExactlyOnceWith(true);
    expect(checkbox).not.toBeChecked();
    view.rerender(<Checkbox aria-label="Make active" checked onCheckedChange={onCheckedChange} />);
    expect(checkbox).toBeChecked();
  });
});
