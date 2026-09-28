import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SteerComposer } from "./SteerComposer";
import { PENDING_SEND_STATE, type PendingSend } from "./useFleetPendingSends";

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

import {
  DISMISS_LABEL as DISMISS,
  RESEND_LABEL as RESEND,
  SEND_FAILED_TEXT as SEND_FAILED,
  SEND_UNCONFIRMED_TEXT as SEND_UNCONFIRMED,
  TOO_LONG_TEXT,
} from "@/tests/fleet-thread/steer-copy";
import { STEER_MESSAGE_MAX_BYTES } from "@/lib/api/fleets-types";

const SUBMITTED_AT_MS = 1_700_000_000_000;

function entry(over: Partial<PendingSend> & { operationId: string }): PendingSend {
  return { text: "deploy the canary", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: SUBMITTED_AT_MS, ...over };
}

const REFUSED = entry({ operationId: "op-refused" });
const noop = () => {};

type Handlers = { onResend?: (id: string) => void; onDismiss?: (id: string) => void; onRestored?: (id: string, text: string) => void };

function view(pending: PendingSend[], handlers: Handlers = {}) {
  return (
    <SteerComposer
      pending={pending}
      onResend={handlers.onResend ?? noop}
      onDismiss={handlers.onDismiss ?? noop}
      onRestored={handlers.onRestored ?? noop}
    />
  );
}

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
    render(view([]));
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
    render(view([entry({ operationId: "op-flight", state: PENDING_SEND_STATE.SENDING })]));
    expect(screen.queryByText(/queued/i)).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove" })).toBeNull();
    // A send still in flight is the optimistic row's to show, not the notice's.
    expect(screen.queryByRole("alert")).toBeNull();
    expect(composer.setText).not.toHaveBeenCalled();
  });

  it("puts the newest unresolved text back into a composer that mounts empty, and lists every send with Resend", () => {
    const pending = [entry({ operationId: "op-a", text: "first refused" }), entry({ operationId: "op-b", text: "second refused" })];
    const onRestored = vi.fn();
    const viewed = render(view(pending, { onRestored }));
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("second refused");
    // The delivery layer is told which send the draft is, so an unchanged Send
    // of it reuses that send's id.
    expect(onRestored).toHaveBeenCalledExactlyOnceWith("op-b", "second refused");
    // The runtime re-renders subscribers on a text change; the mock does not.
    viewed.rerender(view(pending));
    expect(screen.getAllByText(SEND_FAILED)).toHaveLength(2);
    expect(screen.getByText("first refused")).toBeTruthy();
    expect(screen.getByText("second refused")).toBeTruthy();
    const resends = screen.getAllByRole("button", { name: RESEND });
    expect(resends).toHaveLength(2);
    // Resend posts the ledger record itself; it is not the composer's submit.
    for (const button of resends) expect(button.getAttribute("type")).toBe("button");
  });

  it("leaves a refusal while mounted to assistant-ui, and never overwrites a draft", () => {
    // Mounted clean: a refusal arriving later is returned by the runtime's
    // MessageNotSentError handling, not by this component.
    const viewed = render(view([]));
    viewed.rerender(view([REFUSED]));
    expect(composer.setText).not.toHaveBeenCalled();
    expect(screen.getByText(SEND_FAILED)).toBeTruthy();
    expect(screen.getByRole("button", { name: RESEND })).toBeTruthy();
    cleanup();
    // Mounted over a draft: the draft stays. Resend still works — it sends
    // the ledger record, not the draft.
    draft("and roll back staging");
    render(view([REFUSED]));
    expect(composer.setText).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: RESEND })).toBeTruthy();
  });

  it("test_resend_posts_ledger_record_once", () => {
    const onResend = vi.fn();
    draft(REFUSED.text);
    render(view([REFUSED], { onResend }));
    fireEvent.click(screen.getByRole("button", { name: RESEND }));
    expect(onResend).toHaveBeenCalledExactlyOnceWith("op-refused");
    // The draft WAS the refused text, so it is cleared: Enter must not send
    // the same words again under a second id.
    expect(composer.setText).toHaveBeenLastCalledWith("");
    composer.setText.mockClear();

    // A draft that moved on is the operator's; Resend leaves it alone.
    draft("something else");
    fireEvent.click(screen.getByRole("button", { name: RESEND }));
    expect(onResend).toHaveBeenCalledTimes(2);
    expect(composer.setText).not.toHaveBeenCalledWith("");
  });

  it("test_unknown_delivery_notice", () => {
    render(view([entry({ operationId: "op-lost", state: PENDING_SEND_STATE.UNKNOWN })]));
    expect(screen.getByText(SEND_UNCONFIRMED)).toBeTruthy();
    expect(screen.queryByText(SEND_FAILED)).toBeNull();
    expect(screen.getByRole("button", { name: RESEND })).toBeTruthy();
    expect(screen.getByRole("alert")).toBeTruthy();
  });

  it("sends an expired session to sign in, still restores the text, and offers Resend for after", () => {
    render(view([entry({ operationId: "op-session", state: PENDING_SEND_STATE.SESSION })]));
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe("/sign-in");
    expect(screen.getByRole("button", { name: RESEND })).toBeTruthy();
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
  });

  it("says why a draft longer than the daemon takes will not send", () => {
    draft("a".repeat(STEER_MESSAGE_MAX_BYTES));
    const viewed = render(view([]));
    expect(screen.queryByText(TOO_LONG_TEXT)).toBeNull();
    draft("a".repeat(STEER_MESSAGE_MAX_BYTES + 1));
    viewed.rerender(view([]));
    expect(screen.getByText(TOO_LONG_TEXT)).toBeTruthy();
  });

  it("test_dismiss_removes_one_entry", () => {
    const onDismiss = vi.fn();
    render(view([entry({ operationId: "op-a", text: "a" }), entry({ operationId: "op-b", text: "b" })], { onDismiss }));
    const [first] = screen.getAllByRole("button", { name: DISMISS });
    fireEvent.click(first as HTMLElement);
    expect(onDismiss).toHaveBeenCalledExactlyOnceWith("op-a");
  });
});
