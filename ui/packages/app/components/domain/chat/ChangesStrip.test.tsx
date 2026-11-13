import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ChangesStrip } from "./ChangesStrip";
import type { FleetTaskProps } from "./FleetTaskProvider";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

const state = vi.hoisted(() => ({view: null as Pick<FleetTaskProps, "subject" | "events"> | null}));
vi.mock("./FleetTaskProvider", () => ({useFleetTaskView: () => state.view}));
afterEach(() => {cleanup(); state.view = null;});

it("shows no change claim without an identity or confirmed writes", () => {
  const view = render(<ChangesStrip />);
  state.view = {subject: "operator"};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("button")).toBeNull();
  state.view = {subject: "operator", events: [evt({tools: [
    {name: "file_write", args: {path: "pending.ts"}, done: false, startedAtMs: 0, ms: null},
    {name: "file_write", args: {path: "failed.ts"}, done: true, startedAtMs: 0, ms: 1, status: "failed"},
  ]})]};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("button")).toBeNull();
});

it("discloses deduplicated confirmed paths and removes them when identity is lost", async () => {
  const write = (path: string) => ({name: "file_write", args: {path}, done: true, startedAtMs: 0, ms: 1, status: "succeeded" as const});
  state.view = {subject: "operator", events: [evt({tools: [write("src/one.ts"), write("src/one.ts"),
    {name: "file_write", args: {path: "src/one.ts"}, done: false, startedAtMs: 1, ms: null},
  ]})]};
  const view = render(<ChangesStrip />);
  expect(screen.getByRole("button", {name: "1 file changed"})).toBeTruthy();
  state.view = {subject: "operator", events: [evt({tools: [write("src/one.ts"), write("src/two.ts")]})]};
  view.rerender(<ChangesStrip />);
  fireEvent.click(screen.getByRole("button", {name: "2 files changed"}));
  const dialog = await screen.findByRole("dialog", {name: "Recorded changes"});
  expect(dialog.textContent).toContain("src/one.ts");
  expect(dialog.textContent).toContain("src/two.ts");
  expect(screen.queryByRole("link")).toBeNull();
  state.view = {subject: null, events: state.view.events};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});
