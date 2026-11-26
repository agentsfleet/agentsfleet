import type { DurableTaskActivity } from "@/lib/task-activity";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import {FleetDurableTaskProvider} from "@/components/domain/tasks/FleetTaskProvider";
import { DurableTaskPanel } from "@/components/domain/tasks/TaskPanel";
import { recentDurableTaskIds } from "@/components/domain/tasks/task-references";
import { loadDurableTaskDetails } from "@/components/domain/tasks/task-detail-reader";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, summary, DURABLE_TASK_IDS } from "./helpers/task-reads";
const SCOPE = {workspaceId: "workspace", fleetId: "fleet", subject: "indy", revision: 0};
const SOURCES = {repositories: ["a/b"], access: "write" as const, publication: "existing_pr" as const};
const TARGET = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function activity(id: string | null): DurableTaskActivity {
  return {kind: "durable_task_transfer_progress", progress: {activity_id: DURABLE_TASK_IDS[0], durable_task_id: id, source_turn_id: null,
    operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}};
}
function history(...ids: (string | null)[]) {
  return [evt({durableTaskActivity: {items: ids.map(activity)}})];
}
function path(input: RequestInfo | URL): string {return typeof input === "string" ? input : input instanceof URL ? input.href : input.url;}

it("retains Outputs when current work finishes and recovers them after reload from native saved activity", async () => {
  let unfinished = true;
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    if (path(input).includes("?limit=")) return Response.json({items: unfinished ? [summary()] : [], next_cursor: null});
    const saved = detail(summary(DURABLE_TASK_IDS[0], unfinished ? "running" : "done"));
    return Response.json({...saved, context: {...saved.context, target: TARGET}});
  });
  vi.stubGlobal("fetch", network);
  const turns = history(DURABLE_TASK_IDS[0]);
  const view = render(<DurableTaskPanel {...SCOPE} turns={turns} />);
  await screen.findByRole("region", {name: "Pull requests"});
  const outputs = () => within(screen.getByRole("region", {name: "Pull requests"}));
  await screen.findByRole("region", {name: "Pull requests"});
  expect(await outputs().findByRole("link")).toHaveProperty("href", TARGET.url);
  expect(network).toHaveBeenCalledTimes(2);
  view.rerender(<DurableTaskPanel {...SCOPE} turns={turns.map(turn => ({...turn, reply: "More streamed words"}))} />);
  expect(network).toHaveBeenCalledTimes(2);
  unfinished = false;
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} turns={turns} />);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  expect(screen.queryByRole("region", {name: "Tasks"})).toBeNull();
  expect(outputs().getByRole("link")).toHaveProperty("href", TARGET.url);
  view.unmount();
  render(<DurableTaskPanel {...SCOPE} turns={turns} />);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(6));
  expect(screen.queryByRole("region", {name: "Tasks"})).toBeNull();
  await screen.findByRole("region", {name: "Pull requests"});
  expect(await outputs().findByRole("link")).toHaveProperty("href", TARGET.url);
  expect(screen.queryByRole("button", {name: /PR #123 repair/})).toBeNull();
  expect(network).toHaveBeenCalledTimes(6);
});

it("hides all metadata until list authorization, including loading after access loss", async () => {
  const first = Promise.withResolvers<Response>();
  const retry = Promise.withResolvers<Response>();
  const network = vi.fn<typeof fetch>().mockReturnValueOnce(first.promise)
    .mockResolvedValueOnce(new Response(null, {status: 403})).mockReturnValueOnce(retry.promise);
  vi.stubGlobal("fetch", network);
  const turns = history(DURABLE_TASK_IDS[0]);
  const view = render(<DurableTaskPanel {...SCOPE} turns={turns} sources={SOURCES} />);
  expect(screen.queryByRole("region", {name: "Sources"})).toBeNull();
  expect(screen.queryByRole("region", {name: "Tasks"})).toBeNull();
  expect(network).toHaveBeenCalledTimes(1);
  first.resolve(Response.json({items: [], next_cursor: null}));
  await screen.findByRole("region", {name: "Sources"});
  // The historical detail request receives 403; a new list authorization then fails too.
  await waitFor(() => expect(network).toHaveBeenCalledTimes(2));
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} turns={turns} sources={SOURCES} />);
  retry.resolve(new Response(null, {status: 403}));
  await screen.findByText("Current work is unavailable.");
  const pending = Promise.withResolvers<Response>(); network.mockReturnValue(pending.promise);
  await userEvent.click(screen.getByRole("button", {name: "Reload current work"}));
  for (const name of ["Sources", "Outputs", "Changes", "Tasks"]) expect(screen.queryByRole("region", {name})).toBeNull();
  expect(network).toHaveBeenCalledTimes(4);
  pending.resolve(Response.json({items: [], next_cursor: null}));
  view.unmount();
});

it("lists every recent native task past the sixteen it once kept, ignores unowned or prose IDs and excludes current duplicates", () => {
  const ids = Array.from({length: 20}, (_, i) => `019a42b0-0000-7000-8000-${String(i).padStart(12, "0")}`);
  const turns = [evt({reply: DURABLE_TASK_IDS[1]}), ...history(...ids, null, DURABLE_TASK_IDS[0], ids[0]!)];
  const recent = recentDurableTaskIds(turns, [summary()]);
  expect(recent).toHaveLength(20);
  expect(recent[0]).toBe(ids[0]);
  expect(recent.at(-1)).toBe(ids[1]);
  expect(recent).not.toContain(DURABLE_TASK_IDS[0]);
  expect(recent).not.toContain(DURABLE_TASK_IDS[1]);
});

it("validates historical task identity without inventing a minimum resource version", async () => {
  const network = vi.fn<typeof fetch>().mockResolvedValue(Response.json(detail()));
  vi.stubGlobal("fetch", network);
  const observe = vi.fn();
  await loadDurableTaskDetails("workspace", "fleet", [{kind: "historical", id: DURABLE_TASK_IDS[0]}], new AbortController().signal, observe);
  expect(observe).toHaveBeenLastCalledWith({kind: "historical", id: DURABLE_TASK_IDS[0]}, {kind: "ready", value: detail()});
});

it("keeps archived task history readable without stale approval or task mutation controls", async () => {
  const task = summary(DURABLE_TASK_IDS[0], "running");
  const saved = detail(task);
  const network = vi.fn<typeof fetch>().mockImplementation(async input =>
    Response.json(path(input).includes("?limit=") ? {items: [task], next_cursor: null} : saved));
  vi.stubGlobal("fetch", network);
  render(<FleetDurableTaskProvider {...SCOPE} readOnly approvals={{count: 1, href: "/old-approvals"}}>
    <DurableTaskPanel {...SCOPE} />
  </FleetDurableTaskProvider>);
  await screen.findByRole("region", {name: "Tasks"});
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  expect(screen.getByText(saved.context.objective)).toBeTruthy();
  for (const name of ["Approve", "Deny", "Pause", "Resume", "Stop", "Review latest saved criteria"])
    expect(screen.queryByRole("button", {name})).toBeNull();
  expect(network.mock.calls.every(([input]) => path(input).includes("/durable_tasks"))).toBe(true);
});
