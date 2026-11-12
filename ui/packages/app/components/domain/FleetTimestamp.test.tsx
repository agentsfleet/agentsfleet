import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ConversationDate, Timestamp } from "./FleetTimestamp";

afterEach(cleanup);
const INSTANT = new Date("2026-11-12T11:42:15Z");
it("shows a minute clock while retaining the exact accessible instant", () => {
  const {container} = render(<Timestamp createdAt={INSTANT} compact />);
  expect(screen.getByText("11:42")).toBeTruthy();
  expect(container.querySelector("time")?.dateTime).toBe(INSTANT.toISOString());
  expect(screen.getByText(/Occurred .*UTC/)).toBeTruthy();
});
it("adds a calendar marker only for a new valid UTC day", () => {
  const view = render(<ConversationDate createdAt={INSTANT} />);
  expect(view.container.querySelector("time")?.dateTime).toBe(INSTANT.toISOString());
  view.rerender(<ConversationDate createdAt={INSTANT} previous={new Date("2026-11-12T00:00:00Z")} />);
  expect(view.container.textContent).toBe("");
  view.rerender(<ConversationDate createdAt={INSTANT} previous={new Date("2026-11-11T23:59:59Z")} />);
  expect(view.container.querySelector("time")).toBeTruthy();
  view.rerender(<ConversationDate createdAt={INSTANT} previous={new Date("invalid")} />);
  expect(view.container.querySelector("time")).toBeTruthy();
  view.rerender(<ConversationDate createdAt={new Date("invalid")} />);
  expect(view.container.textContent).toBe("");
});
