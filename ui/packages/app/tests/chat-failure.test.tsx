import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFailure } from "@/components/domain/chat/ChatFailure";

const TITLE = "Couldn’t complete this request";
const DESCRIPTION = "The model did not finish its response.";
const RETRY = "Try again";
afterEach(cleanup);

it("keeps the original failure and allows recovery after a rejected retry", async () => {
  const retry = vi.fn().mockRejectedValueOnce(new Error("transport unavailable")).mockResolvedValueOnce(undefined);
  render(<ChatFailure title={TITLE} description={DESCRIPTION} onRetry={retry} />);
  fireEvent.click(screen.getByRole("button", {name: RETRY}));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Couldn’t retry this request. Try again.");
  expect(screen.getByText(DESCRIPTION)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: RETRY}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(retry).toHaveBeenCalledTimes(2);
});

it("admits one retry while the previous attempt is unresolved", async () => {
  let release = (): void => {};
  const retry = vi.fn(() => new Promise<void>(resolve => {release = resolve;}));
  render(<ChatFailure title={TITLE} description={DESCRIPTION} onRetry={retry} />);
  const button = screen.getByRole("button", {name: RETRY});
  act(() => {button.click(); button.click();});
  expect(button).toHaveProperty("disabled", true);
  expect(retry).toHaveBeenCalledTimes(1);
  await act(async () => release());
  expect(button).toHaveProperty("disabled", false);
});

it("keeps unavailable actions disabled and diagnostic text behind disclosure", () => {
  const retry = vi.fn();
  render(<ChatFailure title={TITLE} description={DESCRIPTION} onRetry={retry} disabled details={<p>Request reference</p>} />);
  fireEvent.click(screen.getByRole("button", {name: RETRY}));
  expect(retry).not.toHaveBeenCalled();
  expect(screen.queryByText("Request reference")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "View details"}));
  expect(screen.getByText("Request reference")).toBeTruthy();
});
