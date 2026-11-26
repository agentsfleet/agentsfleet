import type { DurableTaskSummary } from "@/lib/api/tasks";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DurableTaskPanel } from "./TaskPanel";
import { useFleetTurnStream } from "../conversation/stream/useFleetTurnStream";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { __resetRegistryForTests, streamWrapper } from "@/tests/helpers/fleet-streams";
import { DURABLE_TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";
vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: async () => ({ok: false, status: 503, error: "Unavailable"})}));
const SCOPE = {workspaceId: "workspace", fleetId: "linkwarden", subject: "indy", revision: 0};
beforeEach(() => {FakeEventSource.install(); __resetRegistryForTests();});
afterEach(() => {cleanup(); __resetRegistryForTests(); FakeEventSource.uninstall(); vi.unstubAllGlobals();});
function LiveWork() {
  const stream = useFleetTurnStream(SCOPE.workspaceId, SCOPE.fleetId, []);
  return <DurableTaskPanel {...SCOPE} revision={stream.durableTasksSeq} turns={stream.turns} />;
}
function page(items: DurableTaskSummary[]): Response {return Response.json({items, next_cursor: null});}
function requestPath(input: RequestInfo | URL): string {
  return typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
}

it("reconciles stream readiness and opens the first task without a click", async () => {
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(page([]))
    .mockResolvedValueOnce(page([summary()])).mockResolvedValueOnce(Response.json(detail()));
  vi.stubGlobal("fetch", network);
  render(<LiveWork />, {wrapper: streamWrapper});
  await screen.findByText("No outputs yet.");
  act(() => {FakeEventSource.instances[0]!.open(); FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.HELLO, fleet_ids: [SCOPE.fleetId]});});
  const task = await screen.findByRole("button", {name: /PR #123 repair/});
  expect(task.getAttribute("aria-expanded")).toBe("true");
  expect(await within(screen.getByLabelText("PR #123 repair details")).findByText(detail().context.objective)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Refresh current work"})).toBeNull();
  await userEvent.click(task);
  expect(task.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByText("What counts as done?")).toBeNull();
  expect(network).toHaveBeenCalledTimes(3);
});

it("keeps work keyboard-expandable and rereads the shared evidence on native changes", async () => {
  const tasks = [summary(), summary(DURABLE_TASK_IDS[1], "running"), summary(DURABLE_TASK_IDS[2], "ready")];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const task = tasks.find(task => requestPath(input).endsWith(task.id));
    return task ? Response.json(detail(task)) : page(tasks);
  });
  vi.stubGlobal("fetch", network);
  render(<LiveWork />, {wrapper: streamWrapper});
  await waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  const waiting = screen.getByRole("button", {name: /PR #123 repair/});
  waiting.focus(); await userEvent.keyboard("{Enter}");
  expect(waiting.getAttribute("aria-expanded")).toBe("false");
  await userEvent.keyboard("{Enter}");
  expect(waiting.getAttribute("aria-expanded")).toBe("true");
  await waitFor(() => expect(network.mock.calls.some(([path]) => requestPath(path).endsWith(DURABLE_TASK_IDS[0]))).toBe(true));
  const before = network.mock.calls.length;
  act(() => {
    for (const malformed of [{durable_task_id: "invalid", resource_version: 2}, {durable_task_id: tasks[0]!.id, resource_version: -1}]) {
      FakeEventSource.instances[0]!.emitNamed(FRAME_KIND.DURABLE_TASK_CHANGED, JSON.stringify({kind: FRAME_KIND.DURABLE_TASK_CHANGED, ...malformed}));
    }
  });
  expect(network).toHaveBeenCalledTimes(before);
  tasks[0] = summary(DURABLE_TASK_IDS[0], "running", 2);
  act(() => FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.DURABLE_TASK_CHANGED, durable_task_id: tasks[0]!.id, resource_version: 2}));
  await waitFor(() => expect(network).toHaveBeenCalledTimes(before + 4));
  expect(screen.getAllByRole("button", {expanded: false})).toHaveLength(2);
});

it("opens the next task when the open task leaves current work", async () => {
  const next = summary(DURABLE_TASK_IDS[1], "running");
  let tasks = [summary(), next];
  vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockImplementation(async input => {
    const task = tasks.find(item => requestPath(input).endsWith(item.id));
    return task ? Response.json(detail(task)) : page(tasks);
  }));
  const view = render(<DurableTaskPanel {...SCOPE} />);
  await screen.findByRole("button", {name: /PR #123 repair/, expanded: true});
  expect(screen.getByRole("button", {name: /PR #124 repair/, expanded: false})).toBeTruthy();
  tasks = [next];
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByRole("button", {name: /PR #124 repair/, expanded: true})).toBeTruthy();
  expect(screen.queryByRole("button", {name: /PR #123 repair/})).toBeNull();
});

it("retains confirmed work during a failed read, then clears it on access loss", async () => {
  let list: Response = page([summary()]);
  const network = vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? list.clone() : Response.json(detail()));
  vi.stubGlobal("fetch", network);
  const view = render(<DurableTaskPanel {...SCOPE} />);
  await screen.findByText("What counts as done?");
  list = new Response(null, {status: 503});
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByText(/Last confirmed progress is shown/)).toBeTruthy();
  expect(within(screen.getByLabelText("PR #123 repair details")).getByText(detail().context.objective)).toBeTruthy();
  list = new Response(null, {status: 403});
  await userEvent.click(screen.getByRole("button", {name: "Reload current work"}));
  expect(await screen.findByText("Current work is unavailable.")).toBeTruthy();
  expect(screen.queryByText("PR #123 repair")).toBeNull();
  list = page([]);
  view.rerender(<DurableTaskPanel {...SCOPE} revision={2} />);
  await screen.findByText("No outputs yet.");
});

it("shows attached files beside the repositories approved for the fleet", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(page([])));
  const sources = {repositories: ["agentsfleet/docs"], access: "read", publication: "existing_pr"} as const;
  render(<DurableTaskPanel {...SCOPE} sources={sources} files={[{id: "file", name: "release.txt"}]} />);
  const section = await screen.findByRole("region", {name: "Sources"});
  await userEvent.click(section.querySelector("summary")!);
  expect(within(section).getByText("release.txt")).toBeTruthy();
  expect(within(section).getByRole("link", {name: sources.repositories[0]})).toBeTruthy();
});

it.each([[1, "Request details unavailable."], [2, "Request details unavailable."]] as const)("keeps %s stream-confirmed approvals through task failures but clears them on access loss", async (count, label) => {
  const network = vi.fn<typeof fetch>().mockResolvedValue(new Response(null, {status: 503}));
  vi.stubGlobal("fetch", network);
  const approvals = {count, href: "/w/workspace/fleets/fleet#fleet-approvals"};
  const view = render(<DurableTaskPanel {...SCOPE} approvals={approvals} />);
  expect(await screen.findByText(label)).toBeTruthy();
  await screen.findByText("Current work is unavailable.");
  expect(screen.getByText(label)).toBeTruthy();
  network.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} approvals={approvals} />);
  await waitFor(() => expect(screen.queryByText(label)).toBeNull());
  const pending = Promise.withResolvers<Response>(); network.mockReturnValue(pending.promise);
  await userEvent.click(screen.getByRole("button", {name: "Reload current work"}));
  expect(screen.queryByText(label)).toBeNull();
  await act(async () => pending.resolve(new Response(null, {status: 503})));
  expect(screen.queryByText(label)).toBeNull();
  network.mockResolvedValue(page([]));
  await userEvent.click(screen.getByRole("button", {name: "Reload current work"}));
  expect(await screen.findByText(label)).toBeTruthy();
  view.rerender(<DurableTaskPanel {...SCOPE} subject={null} approvals={approvals} />);
  expect(screen.queryByText(label)).toBeNull();
});

it("recovers failed details and preserves previous evidence until a missing response removes it", async () => {
  let response = new Response(null, {status: 503});
  vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? page([summary()]) : response.clone()));
  const view = render(<DurableTaskPanel {...SCOPE} />);
  expect(await screen.findByText("Couldn’t load task details.")).toBeTruthy();
  response = Response.json(detail());
  await userEvent.click(screen.getByRole("button", {name: "Reload details"}));
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  response = new Response(null, {status: 503});
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByText(/Last confirmed details are shown/)).toBeTruthy();
  response = new Response(null, {status: 404});
  view.rerender(<DurableTaskPanel {...SCOPE} revision={2} />);
  expect(await screen.findByText("Couldn’t load task details.")).toBeTruthy();
  expect(screen.queryByText("What counts as done?")).toBeNull();
});

it("aborts stale account/fleet reads and ignores late replies", async () => {
  const pending = Promise.withResolvers<Response>();
  const network = vi.fn<typeof fetch>().mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce(page([summary()])).mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce(page([]));
  vi.stubGlobal("fetch", network);
  const view = render(<DurableTaskPanel {...SCOPE} subject={null} />);
  expect(network).not.toHaveBeenCalled();
  view.rerender(<DurableTaskPanel {...SCOPE} />);
  const listSignal = network.mock.calls[0]?.[1]?.signal;
  view.rerender(<DurableTaskPanel {...SCOPE} fleetId="other" />);
  expect(listSignal?.aborted).toBe(true);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(3));
  const detailSignal = network.mock.calls[2]?.[1]?.signal;
  view.rerender(<DurableTaskPanel {...SCOPE} subject="other-owner" />);
  expect(detailSignal?.aborted).toBe(true);
  await act(async () => pending.resolve(page([summary(DURABLE_TASK_IDS[1])])));
  expect(screen.queryByText("PR #124 repair")).toBeNull();
});

it("shares exact native target and wait evidence between closed rows and outputs without an expansion fetch", async () => {
  const value = detail();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
  const dependency = {active: true, head: target.head, updated_at: "2026-11-03T10:00:00Z", checks: [{workflow_id: 42, run_id: 84, run_number: 42, run_attempt: 1, status: "queued", conclusion: null}]};
  let saved = {...value, context: {...value.context, target}, dependency};
  let current = summary();
  let refreshFailed = false;
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    if (requestPath(input).includes("?limit=")) return page([current]);
    return refreshFailed ? new Response(null, {status: 503}) : Response.json(saved);
  });
  vi.stubGlobal("fetch", network);
  const view = render(<DurableTaskPanel {...SCOPE} />);
  const row = await screen.findByRole("button", {name: /Waiting for CI run #42 on aaaaaaa/});
  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(within(screen.getByRole("region", {name: "Pull requests"})).getByRole("link").getAttribute("href")).toBe(target.url);
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(2);
  await userEvent.click(row);
  expect(row.getAttribute("aria-expanded")).toBe("false");
  saved = {...saved, dependency: {...dependency, checks: []}};
  view.rerender(<DurableTaskPanel {...SCOPE} revision={1} />);
  expect(await screen.findByRole("button", {name: /Waiting for the fleet to confirm the checks/})).toBeTruthy();
  saved = {...saved, dependency: {...dependency, checks: [{...dependency.checks[0]!, run_number: 0}]}};
  view.rerender(<DurableTaskPanel {...SCOPE} revision={2} />);
  expect(await screen.findByRole("button", {name: /Waiting for recorded checks/})).toBeTruthy();
  refreshFailed = true;
  view.rerender(<DurableTaskPanel {...SCOPE} revision={3} />);
  expect(await screen.findByRole("button", {name: /Last confirmed.*Waiting for recorded checks/})).toBeTruthy();
  current = summary(DURABLE_TASK_IDS[0], "paused", 2);
  view.rerender(<DurableTaskPanel {...SCOPE} revision={4} />);
  const paused = await screen.findByRole("button", {name: /PR #123 repair.*Paused/});
  expect(paused.textContent).not.toContain("Waiting");
});

it("distinguishes paused and attention-needed work while retaining a confirmed recovery point", async () => {
  const tasks = [summary(DURABLE_TASK_IDS[0], "paused"), summary(DURABLE_TASK_IDS[1], "blocked"), summary(DURABLE_TASK_IDS[2], "exhausted")];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const task = tasks.find(task => requestPath(input).endsWith(task.id));
    return task ? Response.json(detail(task, DURABLE_TASK_IDS[0])) : page(tasks);
  });
  vi.stubGlobal("fetch", network);
  render(<DurableTaskPanel {...SCOPE} />);
  const taskList = await screen.findByRole("region", {name: "Tasks"});
  const titles = within(taskList).getAllByRole("button", {name: /PR #\d+ repair/}).map(button => button.textContent);
  expect(titles).toEqual([expect.stringMatching(/PR #124 repair.*Blocked/),
    expect.stringMatching(/PR #125 repair.*Attempts exhausted/), expect.stringMatching(/PR #123 repair.*Paused/)]);
  const paused = within(taskList).getByRole("button", {name: /PR #123 repair.*Paused/});
  await userEvent.click(paused);
  expect(await screen.findByText(/Saved recovery point available/)).toBeTruthy();
  expect(screen.queryByText(/No recovery point has been saved/)).toBeNull();
});


it("shows one blocked task with its next step before opening details", async () => {
  const task = summary(DURABLE_TASK_IDS[0], "blocked");
  const network = vi.fn<typeof fetch>().mockImplementation(async input => requestPath(input).includes("?limit=") ? page([task]) : Response.json(detail(task)));
  vi.stubGlobal("fetch", network);
  render(<DurableTaskPanel {...SCOPE} />);
  const taskList = await screen.findByRole("region", {name: "Tasks"});
  const row = within(taskList).getByRole("button", {name: /PR #123 repair.*Blocked.*Check the conversation/});
  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getAllByText(task.title)).toHaveLength(1);
  await waitFor(() => expect(network).toHaveBeenCalledTimes(2));
  expect(await screen.findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(2);
  expect(row.getAttribute("aria-expanded")).toBe("true");
});

it("keeps open work details usable when another queued task arrives before its detail", async () => {
  const second = Promise.withResolvers<Response>();
  let tasks = [summary()];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const path = requestPath(input);
    if (path.includes("?limit=")) return page(tasks);
    return path.endsWith(DURABLE_TASK_IDS[1]) ? second.promise : Response.json(detail(tasks[0]!));
  });
  vi.stubGlobal("fetch", network);
  const turns: FleetTurn[] = [{id: "assessment", role: "assistant", actor: "fleet", text: "", reply: "", outcome: "", failureLabel: null, failureDetail: null,
    createdAt: new Date(), status: "processed", durableTaskActivity: {items: [{kind: "durable_task_decision_completed" as const, progress: {
      activity_id: DURABLE_TASK_IDS[0], durable_task_id: DURABLE_TASK_IDS[0], source_turn_id: null, operation: "goal" as const,
      phase: "recorded" as const, provider: null, model: null, completed: 1, total: 1}}]}}];
  const view = render(<DurableTaskPanel {...SCOPE} turns={turns} />);
  const first = await screen.findByRole("button", {name: /PR #123 repair/});
  expect(await screen.findByText("Last activity · Assessment recorded · goal progress")).toBeTruthy();
  tasks = [...tasks, summary(DURABLE_TASK_IDS[1], "ready")];
  view.rerender(<DurableTaskPanel {...SCOPE} turns={turns} revision={1} />);
  const queued = await screen.findByRole("button", {name: /PR #124 repair.*Queued/});
  expect(first.getAttribute("aria-expanded")).toBe("true");
  expect(queued.getAttribute("aria-expanded")).toBe("false");
  await userEvent.click(queued);
  expect(await within(screen.getByLabelText("PR #124 repair details")).findByText("Loading task details…")).toBeTruthy();
  await act(async () => second.resolve(Response.json(detail(tasks[1]!))));
  expect(await within(screen.getByLabelText("PR #124 repair details")).findByText("What counts as done?")).toBeTruthy();
  expect(network).toHaveBeenCalledTimes(5);
});
