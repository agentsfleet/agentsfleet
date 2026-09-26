import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
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

  it("puts a refused send back into a composer that mounts empty, and offers Resend", () => {
    const view = render(<SteerComposer failure={SEND_FAILURE} />);
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
    // The runtime re-renders subscribers on a text change; the mock does not.
    view.rerender(<SteerComposer failure={SEND_FAILURE} />);
    expect(screen.getByText("Message not sent.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Resend" }).getAttribute("type")).toBe("submit");
  });

  it("leaves a refusal while mounted to assistant-ui, and never overwrites a draft", () => {
    // Mounted clean: a refusal arriving later is returned by the runtime's
    // MessageNotSentError handling, not by this component.
    const view = render(<SteerComposer failure={null} />);
    view.rerender(<SteerComposer failure={SEND_FAILURE} />);
    expect(composer.setText).not.toHaveBeenCalled();
    cleanup();
    // Mounted over a draft: the draft stays, and with no refused text in it
    // there is nothing for Resend to send.
    draft("and roll back staging");
    render(<SteerComposer failure={SEND_FAILURE} />);
    expect(composer.setText).not.toHaveBeenCalled();
    expect(screen.getByText("Message not sent.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
  });

  it("sends an expired session to sign in and still restores the text", () => {
    render(<SteerComposer failure={{ text: "deploy the canary", kind: DELIVERY_FAILURE.SESSION }} />);
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe("/sign-in");
    expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
  });
});
