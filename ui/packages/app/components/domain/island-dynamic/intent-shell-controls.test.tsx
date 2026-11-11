import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ShellControls, SidebarExpandControl } from "@/components/layout/ShellControls";
import { shellSidebarState, useShellSidebarCollapsed } from "@/components/layout/shell-sidebar-state";

afterEach(() => { cleanup(); shellSidebarState.reset(); });

function NavigationControls() {
  const collapsed = useShellSidebarCollapsed();
  return <TooltipProvider>
    <ShellControls workspaceId="workspace" collapsed={collapsed} />
    <SidebarExpandControl collapsed={collapsed} />
    <label>Unsent message<input defaultValue="Keep my draft" /></label>
  </TooltipProvider>;
}

it("keeps the home route usable without downloading a navigation drawer", () => {
  render(<NavigationControls />);
  expect(screen.getByRole("link", {name: "agentsfleet home"}).getAttribute("href")).toBe("/w/workspace/fleets/new");
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("expands and collapses navigation without replacing the draft", async () => {
  render(<NavigationControls />);
  const draft = screen.getByRole("textbox", {name: "Unsent message"});
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", {name: "Expand navigation"}));
  expect(screen.getByRole("button", {name: "Collapse navigation"}).getAttribute("aria-expanded")).toBe("true");
  await user.click(screen.getByRole("button", {name: "Collapse navigation"}));
  expect(screen.getByRole("textbox", {name: "Unsent message"})).toBe(draft);
  expect((draft as HTMLInputElement).value).toBe("Keep my draft");
});
