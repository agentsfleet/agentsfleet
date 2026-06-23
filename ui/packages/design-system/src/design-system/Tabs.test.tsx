import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./Tabs";
import { TAB_LIST_CLASS, TAB_TRIGGER_CLASS_RADIX } from "./tab-styles";

function Sample() {
  return (
    <Tabs defaultValue="one">
      <TabsList aria-label="Sections">
        <TabsTrigger value="one">One</TabsTrigger>
        <TabsTrigger value="two">Two</TabsTrigger>
      </TabsList>
      <TabsContent value="one">Panel One</TabsContent>
      <TabsContent value="two">Panel Two</TabsContent>
    </Tabs>
  );
}

describe("Tabs", () => {
  it("renders a tablist with two tab triggers", () => {
    render(<Sample />);
    const list = screen.getByRole("tablist", { name: "Sections" });
    expect(list).toBeTruthy();
    expect(screen.getByRole("tab", { name: "One" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Two" })).toBeTruthy();
  });

  it("shows the default panel content first", () => {
    render(<Sample />);
    expect(screen.getByText("Panel One")).toBeTruthy();
    // Inactive panels are removed from the DOM by Radix.
    expect(screen.queryByText("Panel Two")).toBeNull();
  });

  it("switches panels via the controlled `value` prop", () => {
    function Controlled({ value }: { value: "one" | "two" }) {
      return (
        <Tabs value={value} onValueChange={() => {}}>
          <TabsList>
            <TabsTrigger value="one">One</TabsTrigger>
            <TabsTrigger value="two">Two</TabsTrigger>
          </TabsList>
          <TabsContent value="one">Panel One</TabsContent>
          <TabsContent value="two">Panel Two</TabsContent>
        </Tabs>
      );
    }
    const { rerender } = render(<Controlled value="one" />);
    expect(screen.getByText("Panel One")).toBeTruthy();
    rerender(<Controlled value="two" />);
    expect(screen.getByText("Panel Two")).toBeTruthy();
    expect(screen.queryByText("Panel One")).toBeNull();
  });

  it("marks the active trigger with data-state=active", () => {
    render(<Sample />);
    const tabOne = screen.getByRole("tab", { name: "One" });
    expect(tabOne.getAttribute("data-state")).toBe("active");
  });

  it("applies the shared underline tab classes to list + trigger", () => {
    render(<Sample />);
    const list = screen.getByRole("tablist");
    expect(list.className).toContain("border-b");
    expect(list.className).toContain("border-border");
    expect(screen.getByRole("tab", { name: "One" }).className).toContain("border-b-2");
  });

  // The one tab style: underline-active, pill retired.
  it("test_tabs_underline_no_pill: active underline present, zero pill cues", () => {
    render(<Sample />);
    const list = screen.getByRole("tablist");
    const tab = screen.getByRole("tab", { name: "One" });
    // active lights to the --pulse underline
    expect(tab.className).toContain("data-[state=active]:border-pulse");
    // the retired pill cues are gone, on both list and trigger
    for (const pill of ["bg-muted", "rounded-lg"]) {
      expect(list.className).not.toContain(pill);
    }
    for (const pill of ["data-[state=active]:bg-background", "data-[state=active]:shadow-sm", "rounded-md"]) {
      expect(tab.className).not.toContain(pill);
    }
    // the shared constants carry the underline verbatim, not a pill (RULE UFS)
    expect(TAB_TRIGGER_CLASS_RADIX).toContain("data-[state=active]:border-pulse");
    expect(TAB_TRIGGER_CLASS_RADIX).not.toContain("bg-background");
    expect(TAB_LIST_CLASS).toContain("border-b");
    expect(TAB_LIST_CLASS).not.toContain("bg-muted");
  });
});
