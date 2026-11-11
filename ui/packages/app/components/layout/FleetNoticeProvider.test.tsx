import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetNoticeProvider, useFleetNotice } from "./FleetNoticeProvider";

const identity = vi.hoisted(() => ({userId: "owner" as string | null}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));
function Controls() {
  const show = useFleetNotice();
  return <><input aria-label="Current work" /><button onClick={() => show({outcome: "archived", historyHref: "/w/release/fleets/watch"})}>Archive example</button>
    <button onClick={() => show({outcome: "deleted", historyHref: ""})}>Delete example</button>
    <button onClick={() => show({outcome: "absent", historyHref: ""})}>Missing example</button></>;
}
const surface = () => <FleetNoticeProvider><Controls /></FleetNoticeProvider>;
const advance = async (ms: number) => act(async () => vi.advanceTimersByTime(ms));
beforeEach(() => {vi.useFakeTimers(); identity.userId = "owner";});
afterEach(() => {cleanup(); vi.useRealTimers();});

it("announces a confirmed result politely, preserves focus and dismisses after eight seconds", async () => {
  render(surface());
  screen.getByRole("textbox").focus();
  fireEvent.click(screen.getByText("Archive example"));
  expect(document.activeElement).toBe(screen.getByRole("textbox"));
  expect(screen.getByTestId("fleet-notice").getAttribute("aria-live")).toBe("polite");
  expect(screen.getByRole("link", {name: "View history"})).toHaveProperty("pathname", "/w/release/fleets/watch");
  expect(screen.queryByRole("button", {name: "Undo"})).toBeNull();
  await advance(7_999);
  expect(screen.getByTestId("fleet-notice").getAttribute("aria-hidden")).toBe("false");
  await advance(1);
  expect(screen.getByTestId("fleet-notice").getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByTestId("fleet-notice").hasAttribute("inert")).toBe(true);
});

it("pauses dismissal while hovered or focused and resumes the remaining time", async () => {
  render(surface()); fireEvent.click(screen.getByText("Archive example"));
  await advance(7_000);
  const notice = screen.getByTestId("fleet-notice");
  fireEvent.mouseEnter(notice);
  await advance(20_000);
  await act(async () => screen.getByRole("link", {name: "View history"}).focus());
  await act(async () => screen.getByRole("button", {name: "Dismiss notification"}).focus());
  fireEvent.mouseLeave(notice);
  await advance(20_000);
  expect(notice.getAttribute("aria-hidden")).toBe("false");
  await act(async () => screen.getByRole("textbox").focus());
  await advance(999); expect(notice.getAttribute("aria-hidden")).toBe("false");
  await advance(1); expect(notice.getAttribute("aria-hidden")).toBe("true");
});

it("restarts the deadline for a later result and clears another account’s history link", async () => {
  const view = render(surface()); fireEvent.click(screen.getByText("Archive example"));
  await advance(7_000); fireEvent.click(screen.getByText("Delete example"));
  await advance(2_000);
  expect(screen.getByText("Fleet permanently deleted")).toBeTruthy();
  expect(screen.getByTestId("fleet-notice").getAttribute("aria-hidden")).toBe("false");
  fireEvent.click(screen.getByText("Archive example"));
  identity.userId = "another-account"; view.rerender(surface());
  expect(screen.queryByRole("link", {name: "View history"})).toBeNull();
  expect(screen.queryByTestId("fleet-notice")).toBeNull();
});

it("ignores a delayed notice callback captured by the previous account", () => {
  let complete: (() => void) | undefined;
  function DelayedAction() {
    const show = useFleetNotice();
    return <button onClick={() => {complete = () => show({outcome: "archived", historyHref: "/w/old/fleets/private"});}}>Begin archive</button>;
  }
  const surface = () => <FleetNoticeProvider><DelayedAction /></FleetNoticeProvider>;
  const view = render(surface()); fireEvent.click(screen.getByText("Begin archive"));
  identity.userId = "another-account"; view.rerender(surface());
  if (!complete) throw new Error("The original account must capture its action callback");
  act(complete);
  expect(screen.queryByTestId("fleet-notice")).toBeNull();
  expect(screen.queryByRole("link", {name: "View history"})).toBeNull();
  identity.userId = "owner"; view.rerender(surface()); act(complete);
  expect(screen.queryByTestId("fleet-notice")).toBeNull();
});

it("preserves the dashboard draft when the loading identity resolves to its signed-in user", () => {
  identity.userId = null;
  const view = render(surface()); const input = screen.getByRole("textbox");
  fireEvent.change(input, {target: {value: "Unsent deployment question"}});
  identity.userId = "owner"; view.rerender(surface());
  expect(screen.getByRole("textbox")).toBe(input);
  expect(screen.getByRole("textbox")).toHaveProperty("value", "Unsent deployment question");
});

it("distinguishes an already absent fleet from a confirmed deletion", () => {
  render(surface()); fireEvent.click(screen.getByText("Missing example"));
  expect(screen.getByText("Fleet is no longer available")).toBeTruthy();
  expect(screen.queryByRole("link", {name: "View history"})).toBeNull();
});
