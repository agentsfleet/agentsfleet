import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { Switch } from "./Switch";
import { Label } from "./Label";

const NAME = "Use shared memory";

function Controlled({ initial = false, onChange }: { initial?: boolean; onChange?: (next: boolean) => void }) {
  const [on, setOn] = useState(initial);
  return (
    <>
      <Switch
        id="setting"
        checked={on}
        onCheckedChange={(next) => {
          onChange?.(next);
          setOn(next);
        }}
      />
      <Label htmlFor="setting">{NAME}</Label>
    </>
  );
}

describe("Switch", () => {
  it("a switch announces its state and flips on click", () => {
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const toggle = screen.getByRole("switch", { name: NAME });
    expect(toggle.getAttribute("aria-checked")).toBe("false");

    fireEvent.click(toggle);
    expect(onChange).toHaveBeenLastCalledWith(true);
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    expect(toggle.getAttribute("data-state")).toBe("checked");

    fireEvent.click(toggle);
    expect(onChange).toHaveBeenLastCalledWith(false);
    expect(toggle.getAttribute("aria-checked")).toBe("false");
  });

  it("a disabled switch keeps its state", () => {
    const onCheckedChange = vi.fn();
    render(<Switch aria-label={NAME} checked disabled onCheckedChange={onCheckedChange} />);
    const toggle = screen.getByRole("switch", { name: NAME });

    fireEvent.click(toggle);
    expect(onCheckedChange).not.toHaveBeenCalled();
    expect(toggle.getAttribute("aria-checked")).toBe("true");
  });

  it("a coarse pointer gets a 44px hit area without a larger track", () => {
    render(<Switch aria-label={NAME} />);
    const toggle = screen.getByRole("switch", { name: NAME });
    for (const reach of ["after:absolute", "pointer-coarse:after:-inset-y-xl", "pointer-coarse:after:-inset-x-md"]) {
      expect(toggle.className).toContain(reach);
    }
    expect(toggle.className).toContain("h-5 w-9");
  });

  it("an aria-disabled switch reads as disabled and stays focusable", () => {
    render(<Switch aria-label={NAME} aria-disabled />);
    const toggle = screen.getByRole("switch", { name: NAME });
    toggle.focus();
    expect(document.activeElement).toBe(toggle);
    expect(toggle.className).toContain("aria-disabled:opacity-50");
  });

  it("a caller's class joins the track's own", () => {
    render(<Switch aria-label={NAME} className="ml-auto" />);
    const toggle = screen.getByRole("switch", { name: NAME });
    expect(toggle.className).toContain("ml-auto");
    expect(toggle.className).toContain("data-[state=checked]:bg-primary");
  });
});
