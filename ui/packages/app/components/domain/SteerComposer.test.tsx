import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SteerComposer } from "./SteerComposer";

vi.mock("@assistant-ui/react", () => ({
  ComposerPrimitive: {
    Root: ({ children, ...rest }: { children: React.ReactNode }) => <div {...rest}>{children}</div>,
    Input: ({ children, placeholder, submitMode }: { children: React.ReactElement; placeholder?: string; submitMode?: string }) =>
      React.cloneElement(children, { placeholder, "data-submit-mode": submitMode } as Record<string, unknown>),
    Send: ({ children }: { children: React.ReactElement }) => children,
  },
}));

afterEach(() => cleanup());

describe("SteerComposer", () => {
  it("renders the approved composer surface", () => {
    render(<SteerComposer failureKind={null} onRetry={vi.fn()} />);
    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.disabled).toBe(false);
    expect(textarea.placeholder).toBe("Message this fleet…");
    expect(textarea.dataset.submitMode).toBe("enter");
    expect(screen.queryByText("Enter to send")).toBeNull();
    expect(screen.getByRole("button", { name: "Send" })).toBeTruthy();
  });

  it("exposes no pending hold — a submitted message is sent, never parked", () => {
    // The browser-side queue is gone: ordering belongs to the fleet's own
    // event stream, and a held message was indistinguishable from a lost one.
    render(<SteerComposer failureKind={null} onRetry={vi.fn()} />);
    expect(screen.queryByText(/queued/i)).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove" })).toBeNull();
    expect(screen.queryByText(/will queue/i)).toBeNull();
  });

  it("offers Retry after a send failure", async () => {
    const retry = vi.fn();
    render(<SteerComposer failureKind="send" onRetry={retry} />);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("sends an expired session to sign in", () => {
    render(<SteerComposer failureKind="session" onRetry={vi.fn()} />);
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe("/sign-in");
  });
});
