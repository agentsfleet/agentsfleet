import { clientUser, ev, mockStream, renderThread, postSteerMock, SUBJECT, WS, ZID } from "./harness";
import { ACCEPTED, OPERATION_ID } from "./steer-helpers";
import { expect, it } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { OUTCOME } from "@/lib/events/event-summary";
import { MS_PER_SECOND } from "@/lib/utils";

const RETRY = "Try again";
const REQUEST = "Check the nightly build";
const ACTOR = `steer:${SUBJECT}`;
const REPORTED_COST_NANOS = "1000000";
function failed(actor = ACTOR, requestJson: string | null = JSON.stringify({message: REQUEST}), running = false) {
  const events = [ev({actor, role: "user", text: REQUEST, status: "fleet_error", outcome: OUTCOME.FAILED,
    reply: "I started checking.", custom: {requestJson}, tokens: null, costNanos: null, wallMs: null})];
  if (running) events.push(ev({actor: ACTOR, role: "user", text: "Another request", status: "received", submittedAtMs: Date.now()}));
  mockStream(events);
  return renderThread();
}

it("retries the original request through the normal delivery queue without losing its partial answer", async () => {
  postSteerMock.mockResolvedValue(ACCEPTED("retry-event"));
  failed();
  fireEvent.click(screen.getByRole("button", {name: RETRY}));
  await waitFor(() => expect(postSteerMock).toHaveBeenCalledWith(WS, ZID, REQUEST, OPERATION_ID, expect.any(AbortSignal)));
  expect(screen.getByText("I started checking.")).toBeTruthy();
});

it.each([null, "{", "{}", "null", '{"message":42}', '{"message":"  "}'])("offers no retry for unusable request metadata: %s", request => {
  failed(ACTOR, request);
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
  expect(screen.getByText("Couldn’t complete this request")).toBeTruthy();
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("does not replay another user's request", () => {
  failed("steer:another-user");
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
});

it("does not retry while the thread is already running", () => {
  failed(ACTOR, JSON.stringify({message: REQUEST}), true);
  const retry = screen.getByRole("button", {name: RETRY});
  expect(retry).toHaveProperty("disabled", true);
  fireEvent.click(retry);
  expect(postSteerMock).not.toHaveBeenCalled();
});

it("shows unavailable usage honestly and does not create zero-valued figures", () => {
  failed();
  expect(screen.getByText("Final usage isn’t available for this request.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "View details"}));
  expect(screen.getByText("No further details were reported.")).toBeTruthy();
  expect(screen.queryByTestId("fleet-reply-figures")).toBeNull();
});

it("keeps reported usage and failure diagnostics behind View details", () => {
  const reason = "Provider refused the request";
  const detail = "The upstream model ended this request before producing an answer.";
  mockStream([ev({actor: ACTOR, role: "user", text: REQUEST, status: "fleet_error", outcome: OUTCOME.FAILED,
    custom: {requestJson: JSON.stringify({message: REQUEST})}, failureLabel: reason, failureDetail: detail,
    tokens: 120, wallMs: MS_PER_SECOND, costNanos: REPORTED_COST_NANOS})]);
  renderThread();
  expect(screen.queryByText("Final usage isn’t available for this request.")).toBeNull();
  expect(screen.queryByText(detail)).toBeNull();
  expect(screen.queryByTestId("fleet-reply-figures")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "View details"}));
  expect(screen.getByText(`Reason: ${reason}`)).toBeTruthy();
  expect(screen.getByText(detail)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Run details"}));
  expect(screen.getByRole("dialog", {name: "Run details"}).textContent).toContain("$0.001");
  expect(screen.queryByText("No further details were reported.")).toBeNull();
});

it("does not offer retry before the signed-in owner is known", () => {
  clientUser.userId = null;
  failed();
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
  expect(postSteerMock).not.toHaveBeenCalled();
});
