import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ChatSummaryProvider, ChatSummaryToggle } from "./ChatSummary";
import { FleetWorkSidebar } from "../FleetWorkSidebar";

afterEach(() => {cleanup(); vi.restoreAllMocks();});

function workspace() {
  return <ChatSummaryProvider><ChatSummaryToggle /><textarea aria-label="Draft" defaultValue="Keep my draft" />
    <FleetWorkSidebar><h3>Durable tasks</h3><p>Validate the repair</p></FleetWorkSidebar></ChatSummaryProvider>;
}

it("opens the mobile summary with one header action and restores focus on close", async () => {
  render(workspace());
  const trigger = screen.getByRole("button", {name: "Toggle fleet summary"});
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(trigger);
  expect(screen.getByRole("dialog").textContent).toContain("Durable tasks");
  expect(screen.queryByRole("button", {name: "Work context"})).toBeNull();
  expect(screen.getByRole("textbox", {name: "Draft", hidden: true})).toHaveProperty("value", "Keep my draft");
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("toggles one desktop summary while preserving the composer", () => {
  const original = window.matchMedia;
  vi.spyOn(window, "matchMedia").mockImplementation(query => ({...original(query), matches: true}));
  render(workspace());
  const draft = screen.getByRole("textbox");
  const toggle = screen.getByRole("button", {name: "Toggle fleet summary"});
  expect(screen.queryByRole("complementary")).toBeNull();
  fireEvent.click(toggle);
  expect(screen.getByRole("complementary", {name: "Fleet summary"}).textContent).toContain("Validate the repair");
  fireEvent.click(toggle);
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(screen.getByRole("textbox")).toBe(draft);
  expect(draft).toHaveProperty("value", "Keep my draft");
});

it("does not add a summary action outside a chat owner", () => {
  render(<ChatSummaryToggle />);
  expect(screen.queryByRole("button")).toBeNull();
});
