import {cleanup, fireEvent, render, screen} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {DropdownMenu, DropdownMenuContent} from "@agentsfleet/design-system";
import {AppearanceMenuItem} from "@/components/layout/AppearanceMenuItem";

const theme = vi.hoisted(() => ({resolvedTheme: "dark", setTheme: vi.fn()}));
vi.mock("next-themes", () => ({useTheme: () => theme}));
afterEach(() => {cleanup(); theme.setTheme.mockReset();});
function Menu() {return <DropdownMenu open><DropdownMenuContent><AppearanceMenuItem /></DropdownMenuContent></DropdownMenu>;}

// Value: protects=reachable appearance choice; fails_when=the menu applies its current theme instead of the requested one;
// seam=theme provider; no_claim=persistent storage.
it("switches in both directions while keeping the account menu open", () => {
  theme.resolvedTheme = "dark";
  const view = render(<Menu />);
  fireEvent.click(screen.getByRole("menuitem", {name: "Appearance: switch to light"}));
  expect(theme.setTheme).toHaveBeenLastCalledWith("light");
  expect(screen.getByRole("menu")).toBeTruthy();
  theme.resolvedTheme = "light"; view.rerender(<Menu />);
  fireEvent.click(screen.getByRole("menuitem", {name: "Appearance: switch to dark"}));
  expect(theme.setTheme).toHaveBeenLastCalledWith("dark");
  expect(screen.getByText("Light")).toBeTruthy();
});
