import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TaskControls } from "@/components/domain/chat/TaskControls";
import { controlTask, type TaskChange } from "@/lib/api/tasks";
import { detail, summary } from "./helpers/task-reads";
vi.mock("@/lib/api/tasks", async original => ({...await original<typeof import("@/lib/api/tasks")>(), controlTask: vi.fn()}));
const change = vi.mocked(controlTask);
const task = () => ({...detail(), etag: `"${"a".repeat(64)}"`});
const props = () => ({workspaceId: "w", fleetId: "f", detail: task(), enabled: true, onChanged: vi.fn()});
afterEach(cleanup);
beforeEach(() => change.mockReset());

it("serializes pause and resumes only after confirmed refresh", async () => {
  let finish!: (result: TaskChange) => void;
  change.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
  const p = props();
  const view = render(<TaskControls {...p} />);
  fireEvent.click(screen.getByRole("button", {name: "Pause"}));
  expect(screen.getByRole("status").textContent).toContain("Pause");
  fireEvent.click(screen.getByRole("button", {name: "Cancel work"}));
  view.rerender(<TaskControls {...p} detail={{...p.detail}} />);
  expect(change).toHaveBeenCalledTimes(1);
  await act(async () => finish({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).toHaveBeenCalledTimes(1);
  view.rerender(<TaskControls {...p} detail={{...p.detail, context: {...p.detail.context, state: "paused"}}} />);
  change.mockResolvedValueOnce({kind: "confirmed", value: p.detail});
  fireEvent.click(screen.getByRole("button", {name: "Resume"}));
  await act(async () => {});
  expect(change.mock.calls[1]?.[3]).toBe("ready");
});

it.each(["access", "changed", "limited", "invalid", "unconfirmed"] as const)("requires refresh after %s refusal", async reason => {
  change.mockResolvedValueOnce({kind: "failed", reason});
  const p = props();
  const view = render(<TaskControls {...p} />);
  fireEvent.click(screen.getByRole("button", {name: "Cancel work"}));
  await act(async () => {});
  expect(screen.getByRole("status").textContent).toBeTruthy();
  expect(screen.getByRole("button", {name: "Pause"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Refresh progress"}));
  expect(p.onChanged).toHaveBeenCalledTimes(1);
  view.rerender(<TaskControls {...p} detail={{...p.detail}} />);
  expect(screen.getByRole("button", {name: "Pause"})).toHaveProperty("disabled", false);
});

it("cancels owned mutations on unmount and hides controls for terminal work", async () => {
  let finish!: (result: TaskChange) => void;
  change.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
  const p = props();
  const view = render(<TaskControls {...p} />);
  fireEvent.click(screen.getByRole("button", {name: "Pause"}));
  const signal = change.mock.calls[0]![4];
  view.unmount();
  expect(signal.aborted).toBe(true);
  await act(async () => finish({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).not.toHaveBeenCalled();
  for (const status of ["done", "cancelled", "exhausted"] as const) {
    const terminal = render(<TaskControls {...p} detail={detail(summary(undefined, status))} />);
    expect(screen.queryByRole("button")).toBeNull();
    terminal.unmount();
  }
  render(<TaskControls {...p} detail={detail()} enabled={false} />);
  expect(screen.getByRole("button", {name: "Pause"})).toHaveProperty("disabled", true);
  expect(screen.getByRole("button", {name: "Refresh progress"})).toBeTruthy();
});

it("admits only one control when two activations arrive before the disabled state renders", async () => {
  let finish!: (result: TaskChange) => void;
  change.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
  const p = props();
  render(<TaskControls {...p} />);
  const button = screen.getByRole("button", {name: "Pause"});
  act(() => {
    button.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    button.dispatchEvent(new MouseEvent("click", {bubbles: true}));
  });
  expect(change).toHaveBeenCalledTimes(1);
  await act(async () => finish({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).toHaveBeenCalledTimes(1);
});
