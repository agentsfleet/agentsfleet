import { expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Popover, PopoverContent, PopoverTrigger } from "./Popover";

const DETAILS = "Run details";

it("dismisses contextual details with Escape and returns focus to the trigger", async () => {
  render(<Popover>
    <PopoverTrigger>{DETAILS}</PopoverTrigger>
    <PopoverContent aria-label={DETAILS}>Duration: 1s</PopoverContent>
  </Popover>);
  const trigger = screen.getByRole("button", { name: DETAILS });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  trigger.focus();
  fireEvent.click(trigger);
  const popup = screen.getByRole("dialog", { name: DETAILS });
  expect(popup).toHaveTextContent("Duration: 1s");
  // Contextual details leave the rest of the page available to the reader.
  expect(document.body.style.overflow).not.toBe("hidden");
  fireEvent.keyDown(popup, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});
