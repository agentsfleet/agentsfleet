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

  // The values that pass through this button include a one-time API key and a
  // runner enrollment token — shown once, unrecoverable. A copy that silently did
  // nothing, on a button that looked like it worked, costs the user that value
  // permanently. So a rejection is REPORTED, not swallowed.
  it("reports a failed clipboard write instead of swallowing it", async () => {
    stubClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));

    const failed = await screen.findByRole("button", { name: /copy failed/i });
    expect(failed.getAttribute("data-outcome")).toBe("failed");
    // Never the success flash: that is the lie this branch exists to prevent.
    expect(screen.queryByRole("button", { name: "Copied" })).toBeNull();
  });

  it("announces the failure in a live region, not by an icon swap alone", async () => {
    stubClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));

    const status = await screen.findByRole("status");
    expect(status.textContent).toMatch(/copy failed/i);
  });

  it("reverts from failed to idle after the reset window", async () => {
    stubClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value={VALUE} label={LABEL} />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));
    await screen.findByRole("button", { name: /copy failed/i });

    vi.advanceTimersByTime(2_000); // pin test: literal is the contract
    await waitFor(() => {
      expect(screen.getByRole("button", { name: LABEL })).toBeTruthy();
    });
  });

  // The labelled variant: where copying IS the page's action (the CLI code, a
  // one-time secret), the label renders visibly and doubles as the live region.
  it("renders the label visibly in showLabel mode and flips it to Copied", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    render(<CopyButton value={VALUE} label={LABEL} showLabel />);

    const button = screen.getByRole("button", { name: LABEL });
    expect(button.textContent).toContain(LABEL);

    fireEvent.click(button);
    await waitFor(() => expect(button.textContent).toContain("Copied"));
    // One node carries the outcome — never a duplicated string in one button.
    expect(button.textContent?.match(/Copied/g)).toHaveLength(1);
  });

  it("announces a failure visibly in showLabel mode", async () => {
    stubClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value={VALUE} label={LABEL} showLabel />);

    fireEvent.click(screen.getByRole("button", { name: LABEL }));
    const failed = await screen.findByRole("button", { name: /copy failed/i });
    expect(failed.textContent).toMatch(/copy failed/i);
  });
});

