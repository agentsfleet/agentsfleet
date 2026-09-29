import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SteerComposer } from "./SteerComposer";
import { PENDING_SEND_STATE, type PendingSend } from "./useFleetPendingSends";

const { composer, aui } = vi.hoisted(() => {
  const state = { text: "", isEmpty: true };
  const setText = vi.fn((text: string) => {
    state.text = text;
    state.isEmpty = text.length === 0;
  });
  // One client for every render, as the runtime's is: an effect keyed on it
  // must not re-run merely because the component rendered.
  return {
    composer: { state, setText },
    aui: { composer: () => ({ getState: () => state, setText }) },
  };
});

const FLEET_PATH = "/w/ws_1/fleets/fleet_1";
vi.mock("next/navigation", () => ({ usePathname: () => FLEET_PATH }));

vi.mock("@assistant-ui/react", () => ({
  ComposerPrimitive: {
    Root: ({ children, ...rest }: { children: React.ReactNode }) => <div {...rest}>{children}</div>,
    Input: ({ children, placeholder, submitMode }: { children: React.ReactElement; placeholder?: string; submitMode?: string }) =>
      React.cloneElement(children, { placeholder, "data-submit-mode": submitMode } as Record<string, unknown>),
    Send: ({ children }: { children: React.ReactElement }) => children,
  },
  useAui: () => aui,
  useAuiState: <T,>(selector: (s: { composer: typeof composer.state }) => T) => selector({ composer: composer.state }),
}));

import {
  DISMISS_LABEL as DISMISS,
  RESEND_LABEL as RESEND,
  SEND_LABEL as SEND,
  SEND_AS_NEW_LABEL as SEND_AS_NEW,
  SEND_CONFLICT_TEXT as SEND_CONFLICT,
  SEND_FAILED_TEXT as SEND_FAILED,
  SEND_UNCONFIRMED_TEXT as SEND_UNCONFIRMED,
  TOO_LONG_TEXT,
} from "@/tests/fleet-thread/steer-copy";
import { STEER_MESSAGE_MAX_BYTES } from "@/lib/api/fleets-types";

const SUBMITTED_AT_MS = 1_700_000_000_000;
// A reply streaming in notifies the composer's store on every flush.
const STORE_NOTIFICATIONS = 100;

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
      onDraft={noop}
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

  it("test_sign_in_returns_to_the_fleet: an expired session signs in back to this fleet, restores the text, and offers Resend", () => {
    render(view([entry({ operationId: "op-session", state: PENDING_SEND_STATE.SESSION })]));
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe(`/sign-in?redirect_url=${encodeURIComponent(FLEET_PATH)}`);
    expect(screen.getByRole("button", { name: RESEND })).toBeTruthy();
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith("deploy the canary");
  });

  it("test_byte_limit_counter_and_disabled_send", () => {
    const send = () => screen.getByRole("button", { name: SEND }) as HTMLButtonElement;
    // Short of nine tenths: no count, even for a draft whose length alone could reach it.
    draft("a".repeat(3_000));
    const viewed = render(view([]));
    expect(screen.queryByText(/ \/ 8,192 bytes$/)).toBeNull();
    draft("a".repeat(7_400));
    viewed.rerender(view([]));
    expect(screen.getByText("7,400 / 8,192 bytes")).toBeTruthy();
    expect(send().disabled).toBe(false);
    draft("a".repeat(STEER_MESSAGE_MAX_BYTES + 1));
    viewed.rerender(view([]));
    expect(screen.getByText("8,193 / 8,192 bytes")).toBeTruthy();
    expect(send().disabled).toBe(true);
  });

  it("counts bytes, not characters: 2,500 three-byte characters are 7,500 bytes, and Send still works", () => {
    draft("€".repeat(2_500));
    render(view([]));
    expect(screen.getByText("7,500 / 8,192 bytes")).toBeTruthy();
    expect((screen.getByRole("button", { name: SEND }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("test_draft_encoded_once_per_text: encodes a draft once, however often the store notifies while it stands still", () => {
    const encode = vi.spyOn(TextEncoder.prototype, "encode");
    // Long enough that its length alone cannot settle its size.
    draft("b".repeat(5_000));
    const viewed = render(view([]));
    for (let flush = 0; flush < STORE_NOTIFICATIONS; flush += 1) viewed.rerender(view([]));
    expect(encode).toHaveBeenCalledTimes(1);
    // A changed draft is counted again.
    draft("c".repeat(5_000));
    viewed.rerender(view([]));
    expect(encode).toHaveBeenCalledTimes(2);
    encode.mockRestore();
  });

  it("says why a draft longer than the daemon takes will not send", () => {
    draft("a".repeat(STEER_MESSAGE_MAX_BYTES));
    const viewed = render(view([]));
    expect(screen.queryByText(TOO_LONG_TEXT)).toBeNull();
    draft("a".repeat(STEER_MESSAGE_MAX_BYTES + 1));
    viewed.rerender(view([]));
    expect(screen.getByText(TOO_LONG_TEXT)).toBeTruthy();
    // Bytes, not characters: three-byte "€" crosses the limit at 2,731.
    draft("€".repeat(2_730));
    viewed.rerender(view([]));
    expect(screen.queryByText(TOO_LONG_TEXT)).toBeNull();
    draft("€".repeat(2_731));
    viewed.rerender(view([]));
    expect(screen.getByText(TOO_LONG_TEXT)).toBeTruthy();
  });

  it("restores once per ledger, so the signed-in user's own ledger is restored when it arrives", () => {
    // Before the user is known the ledger is memory-only and empty.
    const signedOut = vi.fn();
    const viewed = render(view([], { onRestored: signedOut }));
    expect(composer.setText).not.toHaveBeenCalled();
    const signedIn = vi.fn();
    viewed.rerender(view([REFUSED], { onRestored: signedIn }));
    expect(composer.setText).toHaveBeenCalledExactlyOnceWith(REFUSED.text);
    expect(signedIn).toHaveBeenCalledExactlyOnceWith("op-refused", REFUSED.text);
    // A later refusal on the same ledger is assistant-ui's to return, not this.
    draft("");
    viewed.rerender(view([REFUSED, entry({ operationId: "op-later", text: "later" })], { onRestored: signedIn }));
    expect(composer.setText).toHaveBeenCalledTimes(1);
    expect(signedOut).not.toHaveBeenCalled();
  });

  it("returns focus to the draft after Resend or Dismiss, and names the message Resend acts on", () => {
    draft("typing on");
    render(view([REFUSED]));
    const resend = screen.getByRole("button", { name: RESEND });
    const described = document.getElementById(resend.getAttribute("aria-describedby") ?? "");
    expect(described?.textContent).toBe(REFUSED.text);
    fireEvent.click(resend);
    expect(document.activeElement).toBe(screen.getByRole("textbox"));
    resend.focus();
    fireEvent.click(screen.getByRole("button", { name: DISMISS }));
    expect(document.activeElement).toBe(screen.getByRole("textbox"));
  });

  it("offers a conflict no Resend, only Dismiss or Send as new", () => {
    const onResend = vi.fn();
    render(view([entry({ operationId: "op-spent", state: PENDING_SEND_STATE.CONFLICT })], { onResend }));
    expect(screen.getByText(SEND_CONFLICT)).toBeTruthy();
    expect(screen.queryByRole("button", { name: RESEND })).toBeNull();
    expect(screen.getByRole("button", { name: DISMISS })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: SEND_AS_NEW }));
    expect(onResend).toHaveBeenCalledExactlyOnceWith("op-spent");
  });

  it("shows nothing for a dismissed send, and never restores its text", () => {
    render(view([entry({ operationId: "op-gone", text: "", state: PENDING_SEND_STATE.DISMISSED })]));
    expect(screen.queryByRole("list")).toBeNull();
    expect(composer.setText).not.toHaveBeenCalled();
  });

  it("test_dismiss_removes_one_entry", () => {
    const onDismiss = vi.fn();
    render(view([entry({ operationId: "op-a", text: "a" }), entry({ operationId: "op-b", text: "b" })], { onDismiss }));
    const [first] = screen.getAllByRole("button", { name: DISMISS });
    fireEvent.click(first as HTMLElement);
    expect(onDismiss).toHaveBeenCalledExactlyOnceWith("op-a");
  });
});
