import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { recentTaskIds } from "@/components/domain/chat/task-references";
import { loadTaskDetails } from "@/components/domain/chat/task-detail-reader";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, summary, TASK_IDS } from "./helpers/task-reads";
import type { TaskActivity } from "@/lib/task-activity";
const SCOPE = {workspaceId: "workspace", fleetId: "fleet", subject: "indy", revision: 0};
const SOURCES = {repositories: ["a/b"], access: "write" as const, publication: "existing_pr" as const};
const TARGET = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function activity(id: string | null): TaskActivity {
  return {kind: "task_transfer_progress", progress: {activity_id: TASK_IDS[0], task_id: id, source_admission_id: null,
    operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}};
}
function history(...ids: (string | null)[]) {
  return [evt({taskActivity: {items: ids.map(activity), truncated: false}})];
}
function path(input: RequestInfo | URL): string {return typeof input === "string" ? input : input instanceof URL ? input.href : input.url;}

it("retains Outputs when current work finishes and recovers them after reload from native saved activity", async () => {
  let unfinished = true;
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    if (path(input).includes("?limit=")) return Response.json({items: unfinished ? [summary()] : [], next_cursor: null});
    const saved = detail(summary(TASK_IDS[0], unfinished ? "running" : "done"));
    return Response.json({...saved, context: {...saved.context, target: TARGET}});
  });
  vi.stubGlobal("fetch", network);
  const events = history(TASK_IDS[0]);
  const view = render(<TaskPanel {...SCOPE} events={events} />);
  await screen.findByRole("region", {name: "Outputs"});
  const outputs = () => within(screen.getByRole("region", {name: "Outputs"}));
  expect(await outputs().findByRole("link")).toHaveProperty("href", TARGET.url);
  expect(network).toHaveBeenCalledTimes(2);
  view.rerender(<TaskPanel {...SCOPE} events={events.map(event => ({...event, reply: "More streamed words"}))} />);
  expect(network).toHaveBeenCalledTimes(2);
  unfinished = false;
  view.rerender(<TaskPanel {...SCOPE} revision={1} events={events} />);
  await screen.findByText("No unfinished tasks.");
  expect(outputs().getByRole("link")).toHaveProperty("href", TARGET.url);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  view.unmount();
  render(<TaskPanel {...SCOPE} events={events} />);
  await screen.findByText("No unfinished tasks.");
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
  const events = history(TASK_IDS[0]);
  const view = render(<TaskPanel {...SCOPE} events={events} sources={SOURCES} />);
  expect(screen.queryByRole("region", {name: "Sources"})).toBeNull();
  expect(screen.queryByRole("region", {name: "Durable tasks"})).toBeNull();
  expect(network).toHaveBeenCalledTimes(1);
  first.resolve(Response.json({items: [], next_cursor: null}));
  await screen.findByRole("region", {name: "Sources"});
  // The historical detail request receives 403; a new list authorization then fails too.
  await waitFor(() => expect(network).toHaveBeenCalledTimes(2));
  view.rerender(<TaskPanel {...SCOPE} revision={1} events={events} sources={SOURCES} />);
  retry.resolve(new Response(null, {status: 403}));
  await screen.findByText("Current work is unavailable.");
  const pending = Promise.withResolvers<Response>(); network.mockReturnValue(pending.promise);
  await userEvent.click(screen.getByRole("button", {name: "Retry connection"}));
  for (const name of ["Sources", "Outputs", "Changes", "Durable tasks"]) expect(screen.queryByRole("region", {name})).toBeNull();
  expect(network).toHaveBeenCalledTimes(4);
  pending.resolve(Response.json({items: [], next_cursor: null}));
  view.unmount();
});

it("bounds recent native history, ignores unowned or prose IDs and excludes current duplicates", () => {
  const ids = Array.from({length: 20}, (_, i) => `019a42b0-0000-7000-8000-${String(i).padStart(12, "0")}`);
  const events = [evt({reply: TASK_IDS[1]}), ...history(...ids, null, TASK_IDS[0], ids[0]!)];
  const recent = recentTaskIds(events, [summary()]);
  expect(recent).toHaveLength(16);
  expect(recent[0]).toBe(ids[0]);
  expect(recent).not.toContain(TASK_IDS[0]);
  expect(recent).not.toContain(TASK_IDS[1]);
  expect(recent).not.toContain(ids[1]);
});

it("validates historical task identity without inventing a minimum resource version", async () => {
  const network = vi.fn<typeof fetch>().mockResolvedValue(Response.json(detail()));
  vi.stubGlobal("fetch", network);
  const observe = vi.fn();
  await loadTaskDetails("workspace", "fleet", [{kind: "historical", id: TASK_IDS[0]}], new AbortController().signal, observe);
  expect(observe).toHaveBeenLastCalledWith({kind: "historical", id: TASK_IDS[0]}, {kind: "ready", value: detail()});
});
