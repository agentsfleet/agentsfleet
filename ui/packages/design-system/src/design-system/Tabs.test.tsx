import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./Tabs";

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

  it("applies semantic utilities to TabsList and TabsTrigger", () => {
    render(<Sample />);
    expect(screen.getByRole("tablist").className).toContain("bg-muted");
    expect(screen.getByRole("tab", { name: "One" }).className).toContain("rounded-md");
  });
});
