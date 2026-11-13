import type { TaskSummary } from "@/lib/api/tasks";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TaskPanel } from "./TaskPanel";
import { useFleetEventStream } from "../useFleetEventStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { __resetRegistryForTests, streamWrapper } from "@/tests/helpers/fleet-streams";
import { TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";
vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: async () => ({ok: false, status: 503, error: "Unavailable"})}));
const SCOPE = {workspaceId: "workspace", fleetId: "linkwarden", subject: "indy", revision: 0};
beforeEach(() => {FakeEventSource.install(); __resetRegistryForTests();});
afterEach(() => {cleanup(); __resetRegistryForTests(); FakeEventSource.uninstall(); vi.unstubAllGlobals();});
function LiveWork() {
  const stream = useFleetEventStream(SCOPE.workspaceId, SCOPE.fleetId, []);
  return <TaskPanel {...SCOPE} revision={stream.tasksSeq} events={stream.events} />;
}
function page(items: TaskSummary[]): Response {return Response.json({items, next_cursor: null});}
function requestPath(input: RequestInfo | URL): string {
  return typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
}

it("reconciles stream readiness and keeps saved details closed until requested", async () => {
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(page([]))
    .mockResolvedValueOnce(page([summary()])).mockResolvedValueOnce(Response.json(detail()));
  vi.stubGlobal("fetch", network);
  render(<LiveWork />, {wrapper: streamWrapper});
  await screen.findByText("No outputs yet.");
  act(() => {FakeEventSource.instances[0]!.open(); FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.HELLO, fleet_ids: [SCOPE.fleetId]});});
  await userEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  const task = await screen.findByRole("button", {name: /PR #123 repair/});
  expect(task.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByText("What counts as done?")).toBeNull();
  await userEvent.click(task);
  expect(await within(screen.getByLabelText("PR #123 repair details")).findByText(detail().context.objective)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Refresh current work"})).toBeNull();
  expect(network).toHaveBeenCalledTimes(3);
});

it("keeps work keyboard-expandable and rereads the shared evidence on native changes", async () => {
  const tasks = [summary(), summary(TASK_IDS[1], "running"), summary(TASK_IDS[2], "ready")];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const task = tasks.find(task => requestPath(input).endsWith(task.id));
    return task ? Response.json(detail(task)) : page(tasks);
  });
  vi.stubGlobal("fetch", network);
  render(<LiveWork />, {wrapper: streamWrapper});
  await waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  const disclosure = screen.getByRole("button", {name: /^Tasks/});
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");
  await userEvent.click(disclosure);
  expect(screen.queryByText("What counts as done?")).toBeNull();
  const waiting = screen.getByRole("button", {name: /PR #123 repair/});
  waiting.focus(); await userEvent.keyboard("{Enter}");
  await waitFor(() => expect(network.mock.calls.some(([path]) => requestPath(path).endsWith(TASK_IDS[0]))).toBe(true));
  const before = network.mock.calls.length;
  act(() => {
    for (const malformed of [{task_id: "invalid", resource_version: 2}, {task_id: tasks[0]!.id, resource_version: -1}]) {
      FakeEventSource.instances[0]!.emitNamed(FRAME_KIND.TASK_CHANGED, JSON.stringify({kind: FRAME_KIND.TASK_CHANGED, ...malformed}));
    }
  });
  expect(network).toHaveBeenCalledTimes(before);
  tasks[0] = summary(TASK_IDS[0], "running", 2);
  act(() => FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.TASK_CHANGED, task_id: tasks[0]!.id, resource_version: 2}));
  await waitFor(() => expect(network).toHaveBeenCalledTimes(before + 4));
  expect(screen.getAllByRole("button", {expanded: false})).toHaveLength(2);
});

it("retains confirmed work during a failed read, then clears it on access loss", async () => {
  let list: Response = page([summary()]);
  const network = vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? list.clone() : Response.json(detail()));
  vi.stubGlobal("fetch", network);
  const view = render(<TaskPanel {...SCOPE} />);
  await userEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  await userEvent.click(await screen.findByRole("button", {name: /PR #123 repair/}));
  await screen.findByText("What counts as done?");
  list = new Response(null, {status: 503});
  view.rerender(<TaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByText(/Last confirmed progress is shown/)).toBeTruthy();
  expect(within(screen.getByLabelText("PR #123 repair details")).getByText(detail().context.objective)).toBeTruthy();
  list = new Response(null, {status: 403});
  await userEvent.click(screen.getByRole("button", {name: "Retry connection"}));
  expect(await screen.findByText("Current work is unavailable.")).toBeTruthy();
  expect(screen.queryByText("PR #123 repair")).toBeNull();
  list = page([]);
  view.rerender(<TaskPanel {...SCOPE} revision={2} />);
  await screen.findByText("No outputs yet.");
});

it.each([[1, "Request details unavailable."], [2, "Request details unavailable."]] as const)("keeps %s stream-confirmed approvals through task failures but clears them on access loss", async (count, label) => {
  const network = vi.fn<typeof fetch>().mockResolvedValue(new Response(null, {status: 503}));
  vi.stubGlobal("fetch", network);
  const approvals = {count, href: "/w/workspace/fleets/fleet#fleet-approvals"};
  const view = render(<TaskPanel {...SCOPE} approvals={approvals} />);
  expect(await screen.findByText(label)).toBeTruthy();
  await screen.findByText("Current work is unavailable.");
  expect(screen.getByText(label)).toBeTruthy();
  network.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<TaskPanel {...SCOPE} revision={1} approvals={approvals} />);
  await waitFor(() => expect(screen.queryByText(label)).toBeNull());
  const pending = Promise.withResolvers<Response>(); network.mockReturnValue(pending.promise);
  await userEvent.click(screen.getByRole("button", {name: "Retry connection"}));
  expect(screen.queryByText(label)).toBeNull();
  await act(async () => pending.resolve(new Response(null, {status: 503})));
  expect(screen.queryByText(label)).toBeNull();
  network.mockResolvedValue(page([]));
  await userEvent.click(screen.getByRole("button", {name: "Retry connection"}));
  expect(await screen.findByText(label)).toBeTruthy();
  view.rerender(<TaskPanel {...SCOPE} subject={null} approvals={approvals} />);
  expect(screen.queryByText(label)).toBeNull();
});

it("recovers failed details and preserves previous evidence until a missing response removes it", async () => {
  let response = new Response(null, {status: 503});
  vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? page([summary()]) : response.clone()));
  const view = render(<TaskPanel {...SCOPE} />);
  await userEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  await userEvent.click(await screen.findByRole("button", {name: /PR #123 repair/}));
  expect(await screen.findByText("Saved progress is unavailable.")).toBeTruthy();
  response = Response.json(detail());
  await userEvent.click(screen.getByRole("button", {name: "Retry details"}));
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  response = new Response(null, {status: 503});
  view.rerender(<TaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByText(/Last confirmed details are shown/)).toBeTruthy();
  response = new Response(null, {status: 404});
  view.rerender(<TaskPanel {...SCOPE} revision={2} />);
  expect(await screen.findByText("Saved progress is unavailable.")).toBeTruthy();
  expect(screen.queryByText("What counts as done?")).toBeNull();
});

it("aborts stale account/fleet reads and ignores late replies", async () => {
  const pending = Promise.withResolvers<Response>();
  const network = vi.fn<typeof fetch>().mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce(page([summary()])).mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce(page([]));
  vi.stubGlobal("fetch", network);
  const view = render(<TaskPanel {...SCOPE} subject={null} />);
  expect(network).not.toHaveBeenCalled();
  view.rerender(<TaskPanel {...SCOPE} />);
  const listSignal = network.mock.calls[0]?.[1]?.signal;
  view.rerender(<TaskPanel {...SCOPE} fleetId="other" />);
  expect(listSignal?.aborted).toBe(true);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(3));
  const detailSignal = network.mock.calls[2]?.[1]?.signal;
  view.rerender(<TaskPanel {...SCOPE} subject="other-owner" />);
  expect(detailSignal?.aborted).toBe(true);
  await act(async () => pending.resolve(page([summary(TASK_IDS[1])])));
  expect(screen.queryByText("PR #124 repair")).toBeNull();
});

it("shares exact native target and wait evidence between closed rows and outputs without an expansion fetch", async () => {
  const value = detail();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
  const dependency = {active: true, head: target.head, updated_at: "2026-11-03T10:00:00Z", checks: [{workflow_id: 42, run_id: 84, run_number: 42, run_attempt: 1, status: "queued", conclusion: null}]};
  let saved = {...value, context: {...value.context, target}, dependency};
  const network = vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? page([summary()]) : Response.json(saved));
  vi.stubGlobal("fetch", network);
  const view = render(<TaskPanel {...SCOPE} />);
  await userEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  const row = await screen.findByRole("button", {name: /Waiting for CI run #42 on aaaaaaa/});
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(within(screen.getByRole("region", {name: "Pull requests"})).getByRole("link").getAttribute("href")).toBe(target.url);
  await userEvent.click(row);
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(2);
  saved = {...saved, dependency: {...dependency, checks: []}};
  view.rerender(<TaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByRole("button", {name: /Recorded check wait is still active/})).toBeTruthy();
  saved = {...saved, dependency: {...dependency, checks: [{...dependency.checks[0]!, run_number: 0}]}};
  view.rerender(<TaskPanel {...SCOPE} revision={2} />);
  expect(await screen.findByRole("button", {name: /Waiting for recorded checks/})).toBeTruthy();
});

it("distinguishes paused and attention-needed work while retaining a confirmed recovery point", async () => {
  const tasks = [summary(TASK_IDS[0], "paused"), summary(TASK_IDS[1], "blocked"), summary(TASK_IDS[2], "exhausted")];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const task = tasks.find(task => requestPath(input).endsWith(task.id));
    return task ? Response.json(detail(task, TASK_IDS[0])) : page(tasks);
  });
  vi.stubGlobal("fetch", network);
  render(<TaskPanel {...SCOPE} />);
  const disclosure = await screen.findByRole("button", {name: /^Tasks/});
  expect(disclosure.querySelector('[aria-label="Needs attention"]')).toBeTruthy();
  await userEvent.click(disclosure);
  const paused = await screen.findByRole("button", {name: /PR #123 repair/});
  expect(paused.querySelector('[aria-label="Paused"]')).toBeTruthy();
  for (const title of ["PR #124 repair", "PR #125 repair"]) {
    expect(within(screen.getByRole("region", {name: /^Tasks/})).getByRole("button", {name: new RegExp(title)}).querySelector('[aria-label="Needs attention"]')).toBeTruthy();
  }
  await userEvent.click(paused);
  expect(await screen.findByText(/Saved recovery point available/)).toBeTruthy();
  expect(screen.queryByText(/No recovery point has been saved/)).toBeNull();
});


it("promotes a blocked task and opens its existing saved detail without another reader", async () => {
  const task = summary(TASK_IDS[0], "blocked");
  const network = vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? page([task]) : Response.json(detail(task)));
  vi.stubGlobal("fetch", network);
  render(<TaskPanel {...SCOPE} />);
  const attention = await screen.findByRole("region", {name: "Needs attention"});
  await waitFor(() => expect(network).toHaveBeenCalledTimes(2));
  await userEvent.click(within(attention).getByRole("button", {name: /Blocked/}));
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(2);
  expect(within(attention).getByRole("button").getAttribute("aria-expanded")).toBe("true");
});

it("keeps open work details usable when another queued task arrives before its detail", async () => {
  const second = Promise.withResolvers<Response>();
  let tasks = [summary()];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const path = requestPath(input);
    if (path.includes("?limit=")) return page(tasks);
    return path.endsWith(TASK_IDS[1]) ? second.promise : Response.json(detail(tasks[0]!));
  });
  vi.stubGlobal("fetch", network);
  const events: FleetEvent[] = [{id: "assessment", role: "assistant", actor: "fleet", text: "", reply: "", outcome: "", failureLabel: null, failureDetail: null,
    createdAt: new Date(), status: "processed", taskActivity: {truncated: false, items: [{kind: "task_decision_completed" as const, progress: {
      activity_id: TASK_IDS[0], task_id: TASK_IDS[0], source_admission_id: null, operation: "goal" as const,
      phase: "recorded" as const, provider: null, model: null, completed: 1, total: 1}}]}}];
  const view = render(<TaskPanel {...SCOPE} events={events} />);
  await userEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  expect(await screen.findByText("Assessment recorded · goal progress")).toBeTruthy();
  tasks = [...tasks, summary(TASK_IDS[1], "ready")];
  view.rerender(<TaskPanel {...SCOPE} events={events} revision={1} />);
  const queued = await screen.findByRole("button", {name: /PR #124 repair.*Reading saved progress/});
  expect(queued.getAttribute("aria-expanded")).toBe("false");
  await userEvent.click(queued);
  expect(await within(screen.getByLabelText("PR #124 repair details")).findByText("Reading saved progress…")).toBeTruthy();
  await act(async () => second.resolve(Response.json(detail(tasks[1]!))));
  expect(await within(screen.getByLabelText("PR #124 repair details")).findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(5);
});
