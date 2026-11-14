import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import {renderToString} from "react-dom/server";
import { ConversationDate, Timestamp } from "./FleetTimestamp";

afterEach(cleanup);
const INSTANT = new Date("2026-11-12T11:42:15Z");
it("defers local day markers and clock labels until the browser can resolve them", () => {
  const server = document.createElement("div");
  server.innerHTML = renderToString(<><ConversationDate createdAt={INSTANT} /><Timestamp createdAt={INSTANT} compact /></>);
  expect(server.querySelector("[data-conversation-date]")).toBeNull();
  expect(server.querySelector("time")?.textContent).toBe("—");
  expect(server.querySelector("time")?.dateTime).toBe(INSTANT.toISOString());
  render(<Timestamp createdAt={INSTANT} compact />);
  expect(screen.getByText("11:42 AM")).toBeTruthy();
});
it("shows a minute clock while retaining the exact accessible instant", () => {
  const {container} = render(<Timestamp createdAt={INSTANT} compact />);
  expect(screen.getByText("11:42 AM")).toBeTruthy();
  expect(container.querySelector("time")?.dateTime).toBe(INSTANT.toISOString());
  expect(screen.getByText("Occurred Nov 12, 2026, 11:42 AM")).toBeTruthy();
});
it("adds a calendar marker only for a new valid local day", () => {
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
