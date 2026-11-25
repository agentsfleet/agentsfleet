import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DurableTaskPanel } from "@/components/domain/tasks/TaskPanel";
import { readDurableTask, readDurableTasks, controlDurableTask, editDurableTaskStatements } from "@/lib/api/tasks";
import { detail, summary } from "./helpers/task-reads";
vi.mock("@/lib/api/tasks", async original => ({...await original<typeof import("@/lib/api/tasks")>(), readDurableTask: vi.fn(), readDurableTasks: vi.fn(), controlDurableTask: vi.fn(), editDurableTaskStatements: vi.fn()}));
afterEach(() => {cleanup(); vi.resetAllMocks();});

it("refreshes both saved task details and the parent after confirmed control and statement changes", async () => {
  const task = summary();
  const value = {...detail(task), etag: `"${"a".repeat(64)}"`};
  vi.mocked(readDurableTask).mockResolvedValue({kind: "ready", value});
  vi.mocked(readDurableTasks).mockImplementation(async () => ({kind: "ready", value: {items: [{...task}], next_cursor: null}}));
  vi.mocked(controlDurableTask).mockResolvedValue({kind: "confirmed", value});
  vi.mocked(editDurableTaskStatements).mockResolvedValue({kind: "confirmed", value});
  render(<DurableTaskPanel workspaceId="w" fleetId="f" subject="person" revision={1} />);
  fireEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  fireEvent.click(await screen.findByRole("button", {name: new RegExp(task.title)}));
  await screen.findByRole("button", {name: "Pause"});
  fireEvent.click(screen.getByRole("button", {name: "Pause"}));
  await waitFor(() => expect(readDurableTasks).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(readDurableTask).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.getByRole("button", {name: "Edit statements"})).toHaveProperty("disabled", false));
  fireEvent.click(screen.getByRole("button", {name: "Edit statements"}));
  fireEvent.change(screen.getByRole("textbox", {name: "Statement 1"}), {target: {value: "Does the regression pass on PR 88's head?"}});
  fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
  await waitFor(() => expect(readDurableTasks).toHaveBeenCalledTimes(3));
  await waitFor(() => expect(readDurableTask).toHaveBeenCalledTimes(3));
  expect(vi.mocked(editDurableTaskStatements).mock.calls[0]?.[3]).toEqual([{...value.context.questions[0],
    question: {...value.context.questions[0]?.question, instructions: "Does the regression pass on PR 88's head?"}}]);
  expect(screen.queryByRole("form", {name: "Edit completion statements"})).toBeNull();
});
