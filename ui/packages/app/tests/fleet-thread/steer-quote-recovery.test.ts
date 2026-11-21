import { mockStream, ev, renderThread, postSteerMock } from "./harness";
import { ACCEPTED, SEND_LABEL, RESEND_LABEL, SEND_UNCONFIRMED_TEXT, composerInput, operationIdOf, send } from "./steer-helpers";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

const ANSWER = "Run the scans before publishing.";
const REPLY = "Which scans?";
const PAYLOAD = `> ${ANSWER}\n\n${REPLY}`;

function quote(answer = ANSWER) {
  mockStream([ev({role: "assistant", actor: "fleet", text: "", reply: answer})]);
  renderThread();
  fireEvent.click(screen.getByRole("button", {name: "Quote reply"}));
}

it("reuses the operation identity when a quoted message is sent again after a lost response", async () => {
  postSteerMock.mockRejectedValueOnce(new Error("Response lost")).mockResolvedValueOnce(ACCEPTED("confirmed"));
  quote();
  await send(REPLY);
  await waitFor(() => expect(composerInput().value).toBe(REPLY));
  expect(screen.getByRole("button", {name: "Remove quote"})).toBeTruthy();
  expect(screen.getByText(SEND_UNCONFIRMED_TEXT)).toBeTruthy();
  await send(REPLY);
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(2));
  expect(operationIdOf(1)).toBe(operationIdOf(0));
  expect(postSteerMock.mock.calls.map(call => call[2])).toEqual([PAYLOAD, PAYLOAD]);
});

it("clears the matching quote and text together when an unresolved message is resent", async () => {
  postSteerMock.mockRejectedValueOnce(new Error("Response lost")).mockResolvedValueOnce(ACCEPTED("confirmed"));
  quote();
  await send(REPLY);
  await waitFor(() => expect(composerInput().value).toBe(REPLY));
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: RESEND_LABEL}));});
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(2));
  expect(operationIdOf(1)).toBe(operationIdOf(0));
  expect(composerInput().value).toBe("");
  expect(screen.queryByRole("button", {name: "Remove quote"})).toBeNull();
  expect(screen.getByRole("button", {name: SEND_LABEL}).hasAttribute("disabled")).toBe(true);
});

it("sends a long quote and its reply whole, leaving the length to the daemon", async () => {
  // 64 KiB of quote, eight times what the composer once refused.
  const long = "x".repeat(65_536);
  postSteerMock.mockResolvedValueOnce(ACCEPTED("long"));
  quote(long);
  fireEvent.change(composerInput(), { target: { value: "Check this" } });
  expect(screen.getByRole("button", {name: SEND_LABEL}).hasAttribute("disabled"), "no client-side limit holds Send back").toBe(false);
  await send("Check this");
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(1));
  expect(postSteerMock.mock.calls[0]?.[2]).toBe(`> ${long}\n\nCheck this`);
});

it("preserves a newer quote and draft when an earlier quoted send fails", async () => {
  const pending = Promise.withResolvers<never>();
  postSteerMock.mockReturnValueOnce(pending.promise).mockResolvedValueOnce(ACCEPTED("new-edit"));
  const newerQuote = "Check the release notes too.";
  mockStream([ANSWER, newerQuote].map((reply, index) => ev({id: `reply-${index}`, role: "assistant", actor: "fleet", text: "", reply})));
  renderThread();
  const buttons = screen.getAllByRole("button", {name: "Quote reply"});
  const first = buttons[0], second = buttons[1];
  if (!first || !second) throw new Error("Expected both reply quote actions");
  fireEvent.click(first);
  await send(REPLY);
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(1));
  fireEvent.click(second);
  fireEvent.change(composerInput(), {target: {value: "And this"}});
  await act(async () => pending.reject(new Error("Response lost")));
  const restored = `${PAYLOAD}\nAnd this`;
  await waitFor(() => expect(composerInput().value).toBe(restored));
  expect(screen.getByRole("button", {name: "Remove quote"})).toBeTruthy();
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: SEND_LABEL}));});
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(2));
  expect(postSteerMock.mock.calls[1]?.[2]).toBe(`> ${newerQuote}\n\n${restored}`);
  expect(operationIdOf(1)).not.toBe(operationIdOf(0));
});
