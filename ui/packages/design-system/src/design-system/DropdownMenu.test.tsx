import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
} from "./DropdownMenu";

const UPPERCASE = "uppercase";

describe("DropdownMenu", () => {
  it("trigger renders and menu is closed by default", () => {
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Menu</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem>Item 1</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    expect(screen.getByText("Menu")).toBeInTheDocument();
    expect(screen.queryByText("Item 1")).not.toBeInTheDocument();
  });

  it("an open menu exposes its items and separator by role", () => {
    render(
      <DropdownMenu open>
        <DropdownMenuContent>
          <DropdownMenuLabel>Workspace</DropdownMenuLabel>
          <DropdownMenuItem inset>
            Rename<DropdownMenuShortcut>⌘R</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem>Delete</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Rename⌘R", "Delete"]);
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("an inset label takes the same indent an inset item adds", () => {
    render(
      <DropdownMenu open>
        <DropdownMenuContent>
          <DropdownMenuLabel inset data-testid="label">Workspace</DropdownMenuLabel>
          <DropdownMenuItem inset>Rename</DropdownMenuItem>
          <DropdownMenuItem>Delete</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    const [insetItem, plainItem] = screen.getAllByRole("menuitem");
    const indent = [...(insetItem?.classList ?? [])].filter((cls) => !plainItem?.classList.contains(cls));
    expect(indent).not.toEqual([]);
    expect([...screen.getByTestId("label").classList]).toEqual(expect.arrayContaining(indent));
  });

  it("keeps section labels and proper names in sentence case", () => {
    render(
      <DropdownMenu open>
        <DropdownMenuContent>
          <DropdownMenuLabel data-testid="default">Workspace</DropdownMenuLabel>
          <DropdownMenuLabel data-testid="name" variant="name">
            John&apos;s account
          </DropdownMenuLabel>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    expect(screen.getByTestId("default").className.split(" ")).not.toContain(UPPERCASE);
    expect(screen.getByTestId("name").className.split(" ")).not.toContain(UPPERCASE);
  });
});

it("keeps native submenu keyboard entry and returns focus to its identity trigger", async () => {
  render(<DropdownMenu open><DropdownMenuContent>
    <DropdownMenuSub><DropdownMenuSubTrigger>Switch workspace</DropdownMenuSubTrigger>
      <DropdownMenuSubContent><DropdownMenuItem>Release workspace</DropdownMenuItem></DropdownMenuSubContent>
    </DropdownMenuSub>
  </DropdownMenuContent></DropdownMenu>);
  const trigger = screen.getByRole("menuitem", {name: "Switch workspace"});
  trigger.focus();
  fireEvent.keyDown(trigger, {key: "ArrowRight"});
  const item = await screen.findByRole("menuitem", {name: "Release workspace"});
  await waitFor(() => expect(document.activeElement).toBe(item));
  fireEvent.keyDown(item, {key: "ArrowLeft"});
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});
