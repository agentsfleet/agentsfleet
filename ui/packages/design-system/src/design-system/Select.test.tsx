import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  Select,
  SelectTrigger,
  SelectContent,
  SelectItem,
  SelectValue,
  SuggestionList,
} from "./Select";

function Harness({
  defaultValue,
  onValueChange,
  appearance,
  position,
}: {
  defaultValue?: string;
  onValueChange?: (v: string) => void;
  appearance?: "default" | "composer";
  position?: "popper" | "item-aligned";
}) {
  return (
    <Select defaultValue={defaultValue} onValueChange={onValueChange}>
      <SelectTrigger aria-label="provider" appearance={appearance}>
        <SelectValue placeholder="Pick one" />
      </SelectTrigger>
      <SelectContent position={position}>
        <SelectItem value="gpt">GPT</SelectItem>
        <SelectItem value="claude">Claude</SelectItem>
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

  it.each(["default", "composer"] as const)("opens %s, lists choices and emits the selected value", (appearance) => {
    const onValueChange = vi.fn();
    render(<Harness onValueChange={onValueChange} appearance={appearance} />);
    const trigger = screen.getByRole("combobox", { name: /provider/i });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });

    const gpt = screen.getByText("GPT");
    expect(gpt).toBeInTheDocument();

    fireEvent.click(gpt);
    expect(onValueChange).toHaveBeenCalledWith("gpt");
  });

  // A list longer than the window used to render past the bottom edge with no
  // way to reach the rest: the panel clips its overflow, and nothing bounded
  // its height or made the items scroll. Both halves are asserted, because
  // either one alone leaves the list unreachable — a bound with no scroll
  // clips it, and a scroll with no bound never has anything to scroll.
  it("bounds a popper list to the window and lets its items scroll", () => {
    render(<Harness />);
    fireEvent.click(screen.getByLabelText("provider"));

    // Radix's own hooks rather than the classes under test, so the assertions
    // below are about what the component sets and not about how it was found.
    const viewport = document.querySelector("[data-radix-select-viewport]");
    expect(viewport, "the open list renders a viewport").not.toBeNull();
    expect(viewport?.className).toContain("overflow-y-auto");

    const panel = viewport?.parentElement;
    expect(panel, "the viewport sits inside the panel").not.toBeNull();
    expect(panel?.className).toContain(
      "max-h-[var(--radix-select-content-available-height)]",
    );
  });

  it("respects disabled items", () => {
    render(<Harness />);
    const trigger = screen.getByRole("combobox", { name: /provider/i });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    const gemini = screen.getByText("Gemini").closest("[role='option']");
    expect(gemini?.getAttribute("data-disabled")).not.toBeNull();
  });

  it("keeps item-aligned positioning without the popper height and translation", () => {
    // jsdom has no scrolling implementation; Radix still calls the browser API.
    const scrollIntoView = HTMLElement.prototype.scrollIntoView;
    HTMLElement.prototype.scrollIntoView = vi.fn();
    try {
      render(<Harness position="item-aligned" />);
      fireEvent.click(screen.getByLabelText("provider"));
      const panel = document.querySelector("[data-radix-select-viewport]")?.parentElement;
      expect(panel).not.toBeNull();
      expect(panel?.className).not.toContain("--radix-select-content-available-height");
      expect(panel?.className).not.toContain("translate-y-1");
    } finally { HTMLElement.prototype.scrollIntoView = scrollIntoView; }
  });
});

describe("SuggestionList", () => {
  const options = [
    {key: "one", value: "alice", label: "@alice"},
    {key: "two", value: "bob", label: "@bob"},
  ];

  it("exposes the active option and keeps editor focus when a pointer chooses", () => {
    const choose = vi.fn();
    render(<><input aria-label="Message" /><SuggestionList id="suggestions" label="Workspace fleets"
      options={options} active={1} onChoose={choose} /></>);
    const input = screen.getByRole("textbox");
    input.focus();
    const list = screen.getByRole("listbox", {name: "Workspace fleets"});
    expect(list).toHaveValue("bob");
    expect(list).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("option", {name: "@bob"})).toHaveAttribute("id", "suggestions-1");
    const option = screen.getByRole("option", {name: "@alice"});
    const down = new MouseEvent("mousedown", {bubbles: true, cancelable: true});
    fireEvent(option, down);
    expect(down.defaultPrevented).toBe(true);
    fireEvent.click(option);
    expect(choose).toHaveBeenCalledExactlyOnceWith("alice");
    expect(document.activeElement).toBe(input);
  });

  it("supports native change events without treating a click on the list as a choice", () => {
    const choose = vi.fn();
    render(<SuggestionList id="suggestions" label="Workspace fleets" options={options} active={0} onChoose={choose} />);
    const list = screen.getByRole("listbox");
    fireEvent.click(list);
    expect(choose).not.toHaveBeenCalled();
    fireEvent.change(list, {target: {value: "bob"}});
    expect(choose).toHaveBeenCalledExactlyOnceWith("bob");
  });

  it("hides an empty list and keeps a single suggestion at the touch target height", () => {
    const view = render(<SuggestionList id="suggestions" label="Workspace fleets" options={[]} active={-1} onChoose={vi.fn()} />);
    expect(screen.queryByRole("listbox")).toBeNull();
    view.rerender(<SuggestionList id="suggestions" label="Workspace fleets" options={options.slice(0, 1)} active={0} onChoose={vi.fn()} />);
    const list = screen.getByRole("listbox");
    expect(list).toHaveValue("alice");
    expect(list).toHaveClass("h-11");
    expect(list).toHaveAttribute("size", "2");
  });

  it("bounds a long list to six visible rows", () => {
    const rows = Array.from({length: 8}, (_, index) => ({key: String(index), value: String(index), label: `Fleet ${index}`}));
    render(<SuggestionList id="suggestions" label="Workspace fleets" options={rows} active={7} onChoose={vi.fn()} />);
    expect(screen.getByRole("listbox")).toHaveAttribute("size", "6");
    expect(screen.getAllByRole("option")).toHaveLength(8);
  });
});
