import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CopyButton } from "./CopyButton";

const VALUE = "019f2866-2172-7696-9166-c69f309a559a";
const LABEL = "Copy workspace ID";

function stubClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("CopyButton", () => {
  it("writes the value to the clipboard and flips to Copied", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));

    expect(writeText).toHaveBeenCalledWith(VALUE);
    await screen.findByRole("button", { name: "Copied" });
  });

  it("reverts the accessible name after the reset window", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));
    await screen.findByRole("button", { name: "Copied" });

    vi.advanceTimersByTime(2_000); // pin test: literal is the contract
    await waitFor(() => {
      expect(screen.getByRole("button", { name: LABEL })).toBeTruthy();
    });
  });

  it("stays on the idle label when the clipboard write rejects", async () => {
    stubClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));

    // The rejection is swallowed; no "Copied" flash lies about the failure.
    await waitFor(() => {
      expect(screen.getByRole("button", { name: LABEL })).toBeTruthy();
    });
    expect(screen.queryByRole("button", { name: "Copied" })).toBeNull();
  });
});
