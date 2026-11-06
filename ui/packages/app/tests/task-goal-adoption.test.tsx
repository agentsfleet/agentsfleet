import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TaskGoalAdoption } from "@/components/domain/chat/TaskGoalAdoption";
import { adoptTaskGoal, type TaskChange } from "@/lib/api/tasks";
import { setupRequest } from "@/lib/api/draft-request";
import { detail, TASK_IDS, SETUP_ID } from "./helpers/task-reads";
vi.mock("@/lib/api/tasks", async original => ({...await original<typeof import("@/lib/api/tasks")>(), adoptTaskGoal: vi.fn()}));
vi.mock("@/lib/api/draft-request", () => ({setupRequest: vi.fn()}));
const POLICY: unknown = JSON.parse(readFileSync(resolve(process.cwd(), "../../../tests/fixtures/fleets/goal-repair/decision-policy.json"), "utf8"));
const saved = (revision: string = TASK_IDS[1], config: unknown = {"x-agentsfleet": {decision_policy: POLICY}}) => ({setup_revision_id: revision, setup_revision: "3", definition: {config}});
const props = () => ({workspaceId: "w", fleetId: "f", detail: {...detail(), etag: `"${"a".repeat(64)}"`}, enabled: true, onChanged: vi.fn()});
const request = vi.mocked(setupRequest);
const adopt = vi.mocked(adoptTaskGoal);
afterEach(cleanup);
beforeEach(() => {request.mockReset(); adopt.mockReset();});
const review = async () => {fireEvent.click(screen.getByRole("button", {name: "Review latest saved criteria"})); await act(async () => {});};

it("requires explicit review and preserves the current goal until native confirmation", async () => {
  request.mockResolvedValue(saved());
  let finish!: (result: TaskChange) => void;
  adopt.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
  const p = props();
  render(<TaskGoalAdoption {...p} />);
  await review();
  expect(screen.getByText("Success criteria from setup v3")).toBeTruthy();
  expect(adopt).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Keep current criteria"}));
  expect(screen.queryByText("Success criteria from setup v3")).toBeNull();
  await review();
  fireEvent.click(screen.getByRole("button", {name: "Use these criteria"}));
  expect(screen.getByRole("button", {name: "Applying criteria…"})).toHaveProperty("disabled", true);
  expect(adopt.mock.calls[0]?.slice(0, 4)).toEqual(["w", "f", p.detail, TASK_IDS[1]]);
  expect(adopt.mock.calls[0]?.[4]).toMatch(/^[0-9a-f-]{36}$/u);
  await act(async () => finish({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("Success criteria from setup v3")).toBeNull();
});

it("distinguishes unchanged, unreadable and unreachable saved criteria", async () => {
  request.mockResolvedValueOnce(saved(SETUP_ID)).mockResolvedValueOnce(saved(TASK_IDS[1], {})).mockRejectedValueOnce(new Error("offline"));
  render(<TaskGoalAdoption {...props()} />);
  await review();
  expect(screen.getByRole("status").textContent).toContain("already uses");
  await review();
  expect(screen.getByRole("status").textContent).toContain("no readable success criteria");
  await review();
  expect(screen.getByRole("status").textContent).toContain("could not be read");
  expect(adopt).not.toHaveBeenCalled();
});

it("does not retry an unconfirmed adoption and asks for refreshed criteria", async () => {
  request.mockResolvedValue(saved());
  adopt.mockResolvedValue({kind: "failed", reason: "changed"});
  render(<TaskGoalAdoption {...props()} />);
  await review();
  fireEvent.click(screen.getByRole("button", {name: "Use these criteria"}));
  await act(async () => {});
  expect(screen.getByRole("status").textContent).toContain("could not be confirmed");
  expect(adopt).toHaveBeenCalledTimes(1);
});

it("aborts a pending read when its owner unmounts", async () => {
  let reject!: (error: Error) => void;
  request.mockImplementationOnce(() => new Promise((_, fail) => {reject = fail;}));
  const view = render(<TaskGoalAdoption {...props()} />);
  fireEvent.click(screen.getByRole("button", {name: "Review latest saved criteria"}));
  expect(screen.getByRole("status").textContent).toContain("Reading");
  const signal = request.mock.calls[0]![2]!;
  view.unmount();
  expect(signal.aborted).toBe(true);
  await act(async () => reject(new Error("aborted")));
  expect(adopt).not.toHaveBeenCalled();
});

it("aborts pending adoption and cannot adopt without a confirmed version", async () => {
  request.mockResolvedValue(saved());
  let finish!: (result: TaskChange) => void;
  adopt.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
  const p = props();
  const view = render(<TaskGoalAdoption {...p} />);
  await review();
  fireEvent.click(screen.getByRole("button", {name: "Use these criteria"}));
  const signal = adopt.mock.calls[0]![5];
  view.unmount();
  expect(signal.aborted).toBe(true);
  await act(async () => finish({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).not.toHaveBeenCalled();
  const missing = render(<TaskGoalAdoption {...p} detail={detail()} />);
  await review();
  expect(screen.getByRole("button", {name: "Use these criteria"})).toHaveProperty("disabled", true);
  missing.unmount();
  render(<TaskGoalAdoption {...p} enabled={false} />);
  expect(screen.getByRole("button", {name: "Review latest saved criteria"})).toHaveProperty("disabled", true);
});

it("serializes repeated review and apply activations before React renders their disabled states", async () => {
  let finishRead!: (value: ReturnType<typeof saved>) => void;
  request.mockImplementationOnce(() => new Promise(resolve => {finishRead = resolve;}));
  let finishApply!: (result: TaskChange) => void;
  adopt.mockImplementationOnce(() => new Promise(resolve => {finishApply = resolve;}));
  const p = props();
  render(<TaskGoalAdoption {...p} />);
  const reviewButton = screen.getByRole("button", {name: "Review latest saved criteria"});
  act(() => {
    reviewButton.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    reviewButton.dispatchEvent(new MouseEvent("click", {bubbles: true}));
  });
  expect(request).toHaveBeenCalledTimes(1);
  await act(async () => finishRead(saved()));
  const applyButton = screen.getByRole("button", {name: "Use these criteria"});
  act(() => {
    applyButton.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    applyButton.dispatchEvent(new MouseEvent("click", {bubbles: true}));
  });
  expect(adopt).toHaveBeenCalledTimes(1);
  await act(async () => finishApply({kind: "confirmed", value: p.detail}));
  expect(p.onChanged).toHaveBeenCalledTimes(1);
});

it("offers no criteria adoption for terminal work", () => {
  const p = props();
  for (const state of ["done", "cancelled", "exhausted"] as const) {
    const view = render(<TaskGoalAdoption {...p} detail={{...p.detail, context: {...p.detail.context, state}}} />);
    expect(screen.queryByRole("button")).toBeNull();
    view.unmount();
  }
  expect(request).not.toHaveBeenCalled();
});
