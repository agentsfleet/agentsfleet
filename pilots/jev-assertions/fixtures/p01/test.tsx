import { expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { CopyButton } from "./implementation";

const VALUE = "019f2866-2172-7696-9166-c69f309a559a";
const LABEL = "Copy workspace ID";
const BUTTON_ROLE = "button";

function stubClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText }, configurable: true,
  });
}

beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
afterEach(() => vi.useRealTimers());

it("checks clipboard write", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  stubClipboard(writeText);
  render(<CopyButton value={VALUE} label={LABEL} />);
  await act(async () => {
    fireEvent.click(screen.getByRole(BUTTON_ROLE, { name: LABEL }));
  });
  expect(writeText).toHaveBeenCalledTimes(1);
});
