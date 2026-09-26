import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SteerComposer } from "./SteerComposer";
import { DELIVERY_FAILURE } from "./useFleetDeliveryFailure";

const { composer } = vi.hoisted(() => {
  const state = { text: "", isEmpty: true };
  return {
    composer: {
      state,
      setText: vi.fn((text: string) => {
        state.text = text;
        state.isEmpty = text.length === 0;
      }),
    },
  };
});

vi.mock("@assistant-ui/react", () => ({
  ComposerPrimitive: {
    Root: ({ children, ...rest }: { children: React.ReactNode }) => <div {...rest}>{children}</div>,
    Input: ({ children, placeholder, submitMode }: { children: React.ReactElement; placeholder?: string; submitMode?: string }) =>
      React.cloneElement(children, { placeholder, "data-submit-mode": submitMode } as Record<string, unknown>),
    Send: ({ children }: { children: React.ReactElement }) => children,
  },
  useAui: () => ({ composer: () => ({ getState: () => composer.state, setText: composer.setText }) }),
  useAuiState: <T,>(selector: (s: { composer: typeof composer.state }) => T) => selector({ composer: composer.state }),
}));

const SEND_FAILURE = { text: "deploy the canary", kind: DELIVERY_FAILURE.SEND };

function draft(text: string) {
  composer.state.text = text;
  composer.state.isEmpty = text.length === 0;
}

afterEach(() => {
  cleanup();
  draft("");
  composer.setText.mockClear();
});

describe("SteerComposer", () => {
  it("renders the approved composer surface", () => {
    render(<SteerComposer failure={null} />);
    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.disabled).toBe(false);
    expect(textarea.placeholder).toBe("Message this fleet…");
    expect(textarea.dataset.submitMode).toBe("enter");
    expect(screen.queryByText("Enter to send")).toBeNull();
    expect(screen.getByRole("button", { name: "Send" })).toBeTruthy();
    expect(composer.setText).not.toHaveBeenCalled();
  });

  it("exposes no pending hold — a submitted message is sent, never parked", () => {
    // The browser-side queue is gone: ordering belongs to the fleet's own
    // event stream, and a held message was indistinguishable from a lost one.
    render(<SteerComposer failure={null} />);
    expect(screen.queryByText(/queued/i)).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove" })).toBeNull();
    expect(screen.queryByText(/will queue/i)).toBeNull();
  });

  it("puts a refused send back into an empty composer and offers Resend", () => {
    const view = render(<SteerComposer failure={SEND_FAILURE} />);
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
    // The runtime re-renders subscribers on a text change; the mock does not.
    view.rerender(<SteerComposer failure={SEND_FAILURE} />);
    expect(screen.getByText("Message not sent.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Resend" }).getAttribute("type")).toBe("submit");
    expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  });

  it("keeps a newer draft and restores the refused text above it on request", async () => {
    draft("and roll back staging");
    render(<SteerComposer failure={SEND_FAILURE} />);
    expect(composer.setText).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Restore" }));
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary\n\nand roll back staging");
  });

  it("sends an expired session to sign in and still restores the text", () => {
    render(<SteerComposer failure={{ text: "deploy the canary", kind: DELIVERY_FAILURE.SESSION }} />);
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe("/sign-in");
    expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
  });
});
